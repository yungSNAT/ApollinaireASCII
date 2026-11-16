// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! 3D models read from STL and OBJ and drawn turning. The files are built here.
use apollinaire_ascii::model::{Light, MAX_DRAWS, MAX_TRIANGLES, ModelOptions, Spin, read, render};

/// A cube from -1 to 1, as 6 square faces of corner numbers counted from 1.
const CORNERS: [[f32; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
];
const FACES: [[usize; 4]; 6] = [
    [1, 2, 3, 4],
    [5, 8, 7, 6],
    [1, 5, 6, 2],
    [2, 6, 7, 3],
    [3, 7, 8, 4],
    [4, 8, 5, 1],
];

fn cube_triangles() -> Vec<[[f32; 3]; 3]> {
    FACES
        .iter()
        .flat_map(|f| [[f[0], f[1], f[2]], [f[0], f[2], f[3]]])
        .map(|t| t.map(|corner| CORNERS[corner - 1]))
        .collect()
}

fn binary_stl(triangles: &[[[f32; 3]; 3]]) -> Vec<u8> {
    let mut out = b"solid but binary".to_vec();
    out.resize(80, b' ');
    out.extend(u32::try_from(triangles.len()).unwrap().to_le_bytes());
    for triangle in triangles {
        out.extend([0u8; 12]);
        for value in triangle.as_flattened() {
            out.extend(value.to_le_bytes());
        }
        out.extend([0, 0]);
    }
    out
}

fn text_stl(triangles: &[[[f32; 3]; 3]]) -> String {
    let mut out = String::from("solid cube\n");
    for triangle in triangles {
        out += "  facet normal 0 0 0\n    outer loop\n";
        for [x, y, z] in triangle {
            out += &format!("      vertex {x:e} {y:e} {z:e}\n");
        }
        out += "    endloop\n  endfacet\n";
    }
    out + "endsolid cube\n"
}

/// The cube as OBJ quads, with texture and normal numbers that are ignored.
fn obj() -> String {
    let mut out = String::from("# cube\no cube\n");
    for [x, y, z] in CORNERS {
        out += &format!("v {x} {y} {z}\n");
    }
    out += "vn 0 0 1\n";
    for face in FACES {
        out += "f";
        for corner in face {
            out += &format!(" {corner}/1/1");
        }
        out += "\n";
    }
    out
}

fn small(frames: usize) -> ModelOptions {
    ModelOptions {
        width: 40,
        height: 20,
        frames,
        ..Default::default()
    }
}

#[test]
fn stl_and_obj_cubes_read_alike_and_draw_alike() {
    let triangles = cube_triangles();
    let binary = read("cube.stl", &binary_stl(&triangles)).unwrap();
    let text = read("CUBE.STL", text_stl(&triangles).as_bytes()).unwrap();
    let quads = read("cube.obj", obj().as_bytes()).unwrap();
    assert_eq!(binary.triangles(), 12);
    assert_eq!(binary, text);
    assert_eq!(binary, quads);
    let options = small(8);
    let drawn = render(&binary, &options).unwrap();
    assert_eq!(drawn, render(&quads, &options).unwrap());
    assert_eq!(drawn.frames().len(), 8);
    assert!(drawn.looping());
    assert!(drawn.frames().iter().all(|f| f.duration_ms == 1000 / 12));
    for entry in drawn.frames() {
        let frame = &entry.frame;
        assert_eq!((frame.width(), frame.height()), (40, 20));
        assert!(frame.cells().iter().all(|&b| b" .:-=+*#%@".contains(&b)));
    }
}

#[test]
fn a_cube_seen_face_on_is_a_centred_even_square() {
    let cube = read("cube.obj", obj().as_bytes()).unwrap();
    let options = ModelOptions {
        light: Light::Front,
        ..small(4)
    };
    let turning = render(&cube, &options).unwrap();
    let front = &turning.frames()[0].frame;
    let rows: Vec<&[u8]> = front.cells().chunks(40).collect();
    let drawn: Vec<(usize, usize)> = (0..20)
        .flat_map(|y| (0..40).map(move |x| (x, y)))
        .filter(|&(x, y)| rows[y][x] != b' ')
        .collect();
    // The cube's corners are sqrt(3) from its centre, the circle that must fit at any angle,
    // so the face is 40 / sqrt(3) columns wide and half as many rows tall.
    let (xs, ys): (Vec<_>, Vec<_>) = drawn.iter().copied().unzip();
    let (left, right) = (*xs.iter().min().unwrap(), *xs.iter().max().unwrap());
    let (top, bottom) = (*ys.iter().min().unwrap(), *ys.iter().max().unwrap());
    assert_eq!(left + right, 39, "centred across");
    assert_eq!(top + bottom, 19, "centred down");
    assert!((22..=24).contains(&(right - left + 1)), "{left}..{right}");
    assert!((11..=12).contains(&(bottom - top + 1)), "{top}..{bottom}");
    assert_eq!(
        drawn.len(),
        (right - left + 1) * (bottom - top + 1),
        "a filled square"
    );
    // Facing the light straight on, the face takes the densest character.
    assert!(drawn.iter().all(|&(x, y)| rows[y][x] == b'@'));
}

#[test]
fn the_nearest_surface_hides_the_ones_behind_it_and_light_shades_them() {
    // A small square near the viewer, tilted away from the light, before a large one facing it.
    let square = |size: f32, z: f32, tilt: f32| {
        let c = [[-size, -size], [size, -size], [size, size], [-size, size]]
            .map(|[x, y]| [x, y, z + tilt * x]);
        [[c[0], c[1], c[2]], [c[0], c[2], c[3]]]
    };
    let mut triangles = square(1.0, 0.0, 0.0).to_vec();
    triangles.extend(square(0.4, 0.5, 1.0));
    let mesh = read("scene.stl", &binary_stl(&triangles)).unwrap();
    let options = ModelOptions {
        light: Light::Front,
        width: 41,
        height: 21,
        ..small(1)
    };
    let drawn = render(&mesh, &options).unwrap();
    let frame = &drawn.frames()[0].frame;
    let rows: Vec<&[u8]> = frame.cells().chunks(41).collect();
    let centre = rows[10][20];
    assert!(
        b".:-=+*#%".contains(&centre),
        "the tilted square is dimmer: {}",
        centre as char
    );
    assert_eq!(rows[10][8], b'@', "the large square shows round it");
    // Drawn the other way round, the nearer square still wins.
    triangles.reverse();
    let reversed = read("scene.stl", &binary_stl(&triangles)).unwrap();
    assert_eq!(
        &render(&reversed, &options).unwrap().frames()[0].frame,
        frame
    );
}

#[test]
fn each_light_shades_a_turning_cube_its_own_way() {
    let cube = read("cube.obj", obj().as_bytes()).unwrap();
    let lights = [
        Light::TopLeft,
        Light::Top,
        Light::Front,
        Light::Left,
        Light::Right,
    ];
    let drawn: Vec<_> = lights
        .iter()
        .map(|&light| render(&cube, &ModelOptions { light, ..small(8) }).unwrap())
        .collect();
    for (a, first) in drawn.iter().enumerate() {
        for (b, second) in drawn.iter().enumerate().skip(a + 1) {
            assert_ne!(first, second, "{:?} and {:?}", lights[a], lights[b]);
        }
    }
}

#[test]
fn a_flat_triangle_turned_edge_on_vanishes_and_comes_back() {
    let triangle = [[[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]]];
    let mesh = read("flat.stl", &binary_stl(&triangle)).unwrap();
    for spin in [Spin::Y, Spin::X, Spin::XY] {
        let options = ModelOptions { spin, ..small(4) };
        let frames = render(&mesh, &options).unwrap();
        let ink = |index: usize| {
            frames.frames()[index]
                .frame
                .cells()
                .iter()
                .filter(|&&b| b != b' ')
                .count()
        };
        assert!(ink(0) > 0, "{spin:?}");
        assert_eq!(ink(1), 0, "{spin:?}: a quarter turn is edge on");
        assert!(ink(2) > 0, "{spin:?}");
    }
    let x = render(
        &mesh,
        &ModelOptions {
            spin: Spin::X,
            ..small(4)
        },
    )
    .unwrap();
    let y = render(&mesh, &small(4)).unwrap();
    assert_ne!(
        x.frames()[2].frame,
        y.frames()[2].frame,
        "turned over, not round"
    );
}

#[test]
fn obj_faces_take_negative_numbers_and_fan_polygons() {
    let square = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf -4 -3 -2 -1\n";
    let mesh = read("square.obj", square.as_bytes()).unwrap();
    assert_eq!(mesh.triangles(), 2);
    let pentagon = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv -1 0.5 0\nf 1 2 3 4 5\nvt 0 0\ns off\n";
    assert_eq!(read("p.obj", pentagon.as_bytes()).unwrap().triangles(), 3);
    // A face may name a corner listed after it.
    let later = "f 1 2 3\nv 0 0 0\nv 1 0 0\nv 0 1 0\n";
    assert_eq!(read("later.obj", later.as_bytes()).unwrap().triangles(), 1);
}

#[test]
fn malformed_models_are_refused_with_the_reason() {
    let refused = |name: &str, bytes: &[u8]| read(name, bytes).unwrap_err();
    assert!(refused("cube.gltf", b"{}").contains(".stl or .obj"));
    assert!(refused("x.stl", b"not a model").starts_with("Not an STL file"));
    let mut cut = binary_stl(&cube_triangles());
    cut.pop();
    assert!(refused("x.stl", &cut).starts_with("Not an STL file"));
    assert!(refused("x.stl", &binary_stl(&[])).contains("no triangles"));
    let mut nan = cube_triangles();
    nan[3][1][2] = f32::NAN;
    assert!(refused("x.stl", &binary_stl(&nan)).contains("not a finite number"));
    let two =
        "solid\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nendloop\nendfacet\n";
    assert!(refused("x.stl", two.as_bytes()).contains("ending on line 7 has 2 corners"));
    assert!(
        refused("x.stl", b"solid\nvertex 0 0 0\n").contains("Line 2 has a vertex outside a facet")
    );
    assert!(refused("x.stl", b"solid\nendfacet\n").contains("did not begin"));
    assert!(
        refused("x.stl", b"solid\nfacet\nvertex 0 0\n")
            .contains("Line 3 has fewer than 3 coordinates")
    );
    assert!(
        refused("x.stl", b"solid\nfacet\nvertex 0 zero 0\n")
            .contains("\"zero\" where a number belongs")
    );
    assert!(
        refused("x.stl", b"solid\nfacet\nvertex 0 0 inf\n")
            .contains("Line 3: a coordinate is not a finite number")
    );
    assert!(refused("x.stl", b"solid\nfacet\n").contains("ends inside a facet"));
    assert!(
        refused("x.obj", b"v 0 0 0\nf 1 2\n")
            .contains("Line 2 has a face with fewer than 3 corners")
    );
    assert!(
        refused("x.obj", b"v 0 0 0\nf 1 2 3\n")
            .contains("Line 2 names corner 2, but the file has 1")
    );
    assert!(refused("x.obj", b"v 0 0 0\nf 1 1 -2\n").contains("\"-2\" that names no corner"));
    assert!(refused("x.obj", b"v 0 0 0\nf 1 0 1\n").contains("\"0\" that names no corner"));
    assert!(refused("x.obj", b"v 0 0 0\nf 1 x 1\n").contains("\"x\" that names no corner"));
    assert!(refused("x.obj", b"\xff").contains("must be text"));
    assert!(refused("x.obj", b"# nothing\n").contains("no triangles"));
    let mut many = b"solid big".to_vec();
    many.resize(80, b' ');
    let count = u32::try_from(MAX_TRIANGLES + 1).unwrap();
    many.extend(count.to_le_bytes());
    many.resize(84 + 50 * (MAX_TRIANGLES + 1), 0);
    assert!(refused("x.stl", &many).contains("more than 1000000 triangles"));
    let faces = "f 1 2 3\n".repeat(MAX_TRIANGLES + 1);
    assert!(refused("x.obj", faces.as_bytes()).contains("more than 1000000 triangles"));
    let facets =
        "facet\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendfacet\n".repeat(MAX_TRIANGLES + 1);
    assert!(
        refused("x.stl", format!("solid\n{facets}").as_bytes())
            .contains("more than 1000000 triangles")
    );
}

#[test]
fn render_options_and_work_are_bounded() {
    let cube = read("cube.obj", obj().as_bytes()).unwrap();
    let refused = |options: ModelOptions| render(&cube, &options).unwrap_err();
    assert!(refused(small(0)).contains("frame or cell limits"));
    assert!(refused(small(601)).contains("frame or cell limits"));
    assert!(
        refused(ModelOptions {
            width: 512,
            height: 256,
            ..small(300)
        })
        .contains("cell limits")
    );
    assert!(refused(ModelOptions { fps: 0, ..small(4) }).contains("FPS"));
    assert!(
        refused(ModelOptions {
            fps: 61,
            ..small(4)
        })
        .contains("FPS")
    );
    assert!(
        refused(ModelOptions {
            ramp: "#".into(),
            ..small(4)
        })
        .contains("2..=256")
    );
    assert!(
        refused(ModelOptions {
            ramp: "a\u{e9}".into(),
            ..small(4)
        })
        .contains("printable ASCII")
    );
    assert!(
        refused(ModelOptions {
            width: 0,
            ..small(4)
        })
        .contains("Dimensions")
    );
    let point = read("p.obj", b"v 1 1 1\nf 1 1 1\n").unwrap();
    assert!(render(&point, &small(1)).unwrap_err().contains("no size"));
    // Many triangles over many frames pass the draw budget.
    let triangles = MAX_DRAWS / 600 + 1;
    let crowd = "v 0 0 0\nv 1 0 0\nv 0 1 0\n".to_owned() + &"f 1 2 3\n".repeat(triangles);
    let crowd = read("crowd.obj", crowd.as_bytes()).unwrap();
    let message = render(
        &crowd,
        &ModelOptions {
            width: 4,
            height: 4,
            ..small(600)
        },
    )
    .unwrap_err();
    assert!(message.contains("triangle draws"), "{message}");
    // Large triangles covering the frame many times over pass the cell-test budget.
    let cover = "v -1 -1 0\nv 1 -1 0\nv 0 1 0\n".to_owned() + &"f 1 2 3\n".repeat(80_000);
    let cover = read("cover.obj", cover.as_bytes()).unwrap();
    let options = ModelOptions {
        width: 512,
        height: 64,
        ..small(60)
    };
    let message = render(&cover, &options).unwrap_err();
    assert!(message.contains("cell tests"), "{message}");
}
