// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

#[cfg(unix)]
mod unix {
    use super::super::open_contained_file;
    use std::{
        fs,
        io::read_to_string,
        os::unix::fs::symlink,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    fn check_root_race_outcome(res: apollinaire_ascii::Result<(String, std::fs::File)>) {
        if let Ok((_, file)) = res {
            let text = read_to_string(file).unwrap_or_default();
            assert!(!text.contains("SECRET_OUT"), "leaked secret: {text}");
        }
    }

    #[test]
    fn open_contained_file_reports_unopenable_workspace_root() {
        let err = open_contained_file(
            Path::new("/definitely_missing_apollinaire_root"),
            &["doc.md".to_owned()],
        )
        .expect_err("missing root must fail");
        assert!(err.contains("Cannot resolve workspace folder"), "{err}");
    }

    fn setup_unix_race_tree(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "apollinaire-root-ancestor-{tag}-{}",
                std::process::id()
            ));
        let _ = fs::remove_dir_all(&base);
        let outside_ws = base.join("outside").join("ws");
        fs::create_dir_all(&outside_ws).unwrap();
        fs::write(outside_ws.join("doc.md"), "SECRET_OUT").unwrap();

        let real_parent = base.join("real");
        let real_ws = real_parent.join("ws");
        fs::create_dir_all(&real_ws).unwrap();
        fs::write(real_ws.join("doc.md"), "IN_SAFE").unwrap();
        (base, real_parent, real_ws)
    }

    fn spawn_unix_flipper(
        base: PathBuf,
        stop: Arc<AtomicBool>,
        did_swap: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let link_tmp = base.join("real_link");
            let live = base.join("real");
            let bak = base.join("real_bak");
            while !stop.load(Ordering::Relaxed) {
                let _ = fs::remove_file(&link_tmp);
                let _ = symlink(base.join("outside"), &link_tmp);
                if fs::rename(&live, &bak).is_ok() {
                    let _ = fs::rename(&link_tmp, &live);
                    let _ = fs::remove_file(&live);
                    let _ = fs::rename(&bak, &live);
                    did_swap.store(true, Ordering::Release);
                }
            }
        })
    }

    #[test]
    fn open_contained_file_rejects_and_races_workspace_root_ancestor_symlink() {
        let (base, real_parent, real_ws) = setup_unix_race_tree("unix");
        let link = base.join("link");
        symlink(&real_parent, &link).unwrap();
        let err = open_contained_file(&link.join("ws"), &["doc.md".to_owned()])
            .expect_err("symlink ancestor above workspace root must fail");
        assert!(err.contains("Cannot resolve workspace folder"), "{err}");

        // Exercise both `Ok` and `Err` paths cleanly, then race root-ancestor replacement.
        check_root_race_outcome(Err(err));
        let fixed_root = fs::canonicalize(&real_ws).unwrap();
        check_root_race_outcome(open_contained_file(&fixed_root, &["doc.md".to_owned()]));

        let stop = Arc::new(AtomicBool::new(false));
        let did_swap = Arc::new(AtomicBool::new(false));
        let flipper = spawn_unix_flipper(base.clone(), stop.clone(), did_swap.clone());
        let mut count = 0_usize;
        while count < 40 || !did_swap.load(Ordering::Acquire) {
            check_root_race_outcome(open_contained_file(&fixed_root, &["doc.md".to_owned()]));
            count += 1;
            std::thread::yield_now();
        }
        stop.store(true, Ordering::Relaxed);
        let _ = flipper.join();
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(windows)]
mod windows {
    use super::super::open_contained_file;
    use std::{
        fs,
        io::read_to_string,
        path::PathBuf,
        process::{Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    fn mk_junction(link: &std::path::Path, target: &std::path::Path) -> bool {
        Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &link.display().to_string(),
                &target.display().to_string(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn setup_win_race_tree(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "apollinaire-root-ancestor-{tag}-{}",
                std::process::id()
            ));
        let _ = fs::remove_dir_all(&base);
        let outside_ws = base.join("outside").join("ws");
        fs::create_dir_all(&outside_ws).unwrap();
        fs::write(outside_ws.join("doc.md"), "SECRET_OUT").unwrap();

        let real_parent = base.join("real");
        let real_ws = real_parent.join("ws");
        fs::create_dir_all(&real_ws).unwrap();
        fs::write(real_ws.join("doc.md"), "IN_SAFE").unwrap();
        (base, real_parent, real_ws)
    }

    fn spawn_win_flipper(
        base: PathBuf,
        stop: Arc<AtomicBool>,
        did_swap: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let link_tmp = base.join("real_link");
            let live = base.join("real");
            let bak = base.join("real_bak");
            let outside = base.join("outside");
            while !stop.load(Ordering::Relaxed) {
                let _ = fs::remove_dir(&link_tmp);
                if mk_junction(&link_tmp, &outside) && fs::rename(&live, &bak).is_ok() {
                    let _ = fs::rename(&link_tmp, &live);
                    let _ = fs::remove_dir(&live);
                    let _ = fs::rename(&bak, &live);
                    did_swap.store(true, Ordering::Release);
                }
                let _ = fs::remove_dir(&link_tmp);
            }
        })
    }

    #[test]
    fn open_contained_file_rejects_and_races_workspace_root_junction() {
        let (base, real_parent, real_ws) = setup_win_race_tree("win");
        let junction = base.join("junc");
        assert!(mk_junction(&junction, &real_parent));
        let err = open_contained_file(&junction.join("ws"), &["doc.md".to_owned()])
            .expect_err("junction ancestor above workspace root must fail");
        assert!(err.contains("reparse point refused"), "{err}");

        let fixed_root = fs::canonicalize(&real_ws).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let did_swap = Arc::new(AtomicBool::new(false));
        let flipper = spawn_win_flipper(base.clone(), stop.clone(), did_swap.clone());
        let mut count = 0_usize;
        while count < 20 || !did_swap.load(Ordering::Acquire) {
            if let Ok((_, file)) = open_contained_file(&fixed_root, &["doc.md".to_owned()]) {
                let text = read_to_string(file).unwrap_or_default();
                assert!(!text.contains("SECRET_OUT"), "leaked secret: {text}");
            }
            count += 1;
            std::thread::yield_now();
        }
        stop.store(true, Ordering::Relaxed);
        let _ = flipper.join();
        let _ = fs::remove_dir_all(&base);
    }
}
