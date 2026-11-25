// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

#[cfg(unix)]
mod unix {
    use super::super::{anchor_workspace_root, open_contained_file, read_contained_utf8};
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

    struct FlipperGuard {
        stop: Arc<AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for FlipperGuard {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    fn check_root_race_outcome(res: apollinaire_ascii::Result<(String, std::fs::File)>) {
        if let Ok((_, file)) = res {
            let text = read_to_string(file).unwrap_or_default();
            assert!(!text.contains("SECRET_OUT"), "leaked secret: {text}");
        }
    }

    #[test]
    fn open_contained_file_reports_unopenable_workspace_root() {
        let missing = Path::new("/definitely_missing_apollinaire_root");
        let err = open_contained_file(missing, &["doc.md".to_owned()], None)
            .expect_err("missing root must fail");
        assert!(err.contains("Cannot resolve workspace folder"), "{err}");
        let anchor_err = read_contained_utf8("doc.md", missing, 1024)
            .expect_err("missing anchor root must fail");
        assert!(
            anchor_err.contains("Cannot resolve workspace folder"),
            "{anchor_err}"
        );
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
    ) -> FlipperGuard {
        let stop_bg = stop.clone();
        let handle = std::thread::spawn(move || {
            let link_tmp = base.join("real_link");
            let live = base.join("real");
            let bak = base.join("real_bak");
            while !stop_bg.load(Ordering::Relaxed) {
                let _ = fs::remove_file(&link_tmp);
                let _ = symlink(base.join("outside"), &link_tmp);
                if fs::rename(&live, &bak).is_ok() {
                    if fs::rename(&link_tmp, &live).is_ok() {
                        did_swap.store(true, Ordering::Release);
                        let _ = fs::remove_file(&live);
                    }
                    let _ = fs::rename(&bak, &live);
                }
            }
        });
        FlipperGuard {
            stop,
            handle: Some(handle),
        }
    }

    fn spawn_unix_dir_flipper(
        base: PathBuf,
        stop: Arc<AtomicBool>,
        did_swap: Arc<AtomicBool>,
    ) -> FlipperGuard {
        let stop_bg = stop.clone();
        let handle = std::thread::spawn(move || {
            let live = base.join("real");
            let outside = base.join("outside");
            let bak = base.join("real_bak");
            while !stop_bg.load(Ordering::Relaxed) {
                if fs::rename(&live, &bak).is_ok() {
                    if fs::rename(&outside, &live).is_ok() {
                        did_swap.store(true, Ordering::Release);
                        let _ = fs::rename(&live, &outside);
                    }
                    let _ = fs::rename(&bak, &live);
                }
            }
        });
        FlipperGuard {
            stop,
            handle: Some(handle),
        }
    }

    #[test]
    fn open_contained_file_rejects_ordinary_root_dir_swap() {
        let (base, real_parent, real_ws) = setup_unix_race_tree("unix-id");
        let anchor = anchor_workspace_root(&real_ws).unwrap();
        let fixed_root = fs::canonicalize(&real_ws).unwrap();
        let bak = base.join("real_bak");
        fs::rename(&real_parent, &bak).unwrap();
        fs::rename(base.join("outside"), &real_parent).unwrap();
        let err = open_contained_file(&fixed_root, &["doc.md".to_owned()], Some(anchor.id))
            .expect_err("ordinary directory swap must fail");
        assert!(err.contains("root identity changed"), "{err}");
        fs::rename(&real_parent, base.join("outside")).unwrap();
        fs::rename(&bak, &real_parent).unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let did_swap = Arc::new(AtomicBool::new(false));
        let guard = spawn_unix_dir_flipper(base.clone(), stop, did_swap.clone());
        let mut count = 0_usize;
        while count < 40 || !did_swap.load(Ordering::Acquire) {
            check_root_race_outcome(open_contained_file(
                &fixed_root,
                &["doc.md".to_owned()],
                Some(anchor.id),
            ));
            count += 1;
            std::thread::yield_now();
        }
        drop(guard);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn open_contained_file_rejects_and_races_workspace_root_ancestor_symlink() {
        let (base, real_parent, real_ws) = setup_unix_race_tree("unix");
        let link = base.join("link");
        symlink(&real_parent, &link).unwrap();
        let err = open_contained_file(&link.join("ws"), &["doc.md".to_owned()], None)
            .expect_err("symlink ancestor above workspace root must fail");
        assert!(err.contains("Cannot resolve workspace folder"), "{err}");

        check_root_race_outcome(Err(err));
        let fixed_root = fs::canonicalize(&real_ws).unwrap();
        check_root_race_outcome(open_contained_file(
            &fixed_root,
            &["doc.md".to_owned()],
            None,
        ));

        let stop = Arc::new(AtomicBool::new(false));
        let did_swap = Arc::new(AtomicBool::new(false));
        let _guard = spawn_unix_flipper(base.clone(), stop, did_swap.clone());
        let mut count = 0_usize;
        while count < 40 || !did_swap.load(Ordering::Acquire) {
            check_root_race_outcome(open_contained_file(
                &fixed_root,
                &["doc.md".to_owned()],
                None,
            ));
            count += 1;
            std::thread::yield_now();
        }
        drop(_guard);
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(windows)]
mod windows {
    use super::super::{anchor_workspace_root, open_contained_file, read_contained_utf8};
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

    struct FlipperGuard {
        stop: Arc<AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for FlipperGuard {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

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
        let outside_ws = base.join("outside").join("ws").join("sub");
        fs::create_dir_all(&outside_ws).unwrap();
        fs::write(base.join("outside").join("ws").join("doc.md"), "SECRET_OUT").unwrap();
        fs::write(outside_ws.join("doc.md"), "SECRET_OUT").unwrap();

        let real_parent = base.join("real");
        let real_ws = real_parent.join("ws");
        fs::create_dir_all(real_ws.join("sub")).unwrap();
        fs::write(real_ws.join("doc.md"), "IN_SAFE").unwrap();
        fs::write(real_ws.join("sub").join("doc.md"), "IN_SAFE").unwrap();
        (base, real_parent, real_ws)
    }

    fn spawn_win_flipper(
        base: PathBuf,
        stop: Arc<AtomicBool>,
        did_swap: Arc<AtomicBool>,
    ) -> FlipperGuard {
        let stop_bg = stop.clone();
        let handle = std::thread::spawn(move || {
            let link_tmp = base.join("real_link");
            let live = base.join("real");
            let bak = base.join("real_bak");
            let outside = base.join("outside");
            while !stop_bg.load(Ordering::Relaxed) {
                let _ = fs::remove_dir(&link_tmp);
                if mk_junction(&link_tmp, &outside) && fs::rename(&live, &bak).is_ok() {
                    if fs::rename(&link_tmp, &live).is_ok() {
                        did_swap.store(true, Ordering::Release);
                        let _ = fs::remove_dir(&live);
                    }
                    let _ = fs::rename(&bak, &live);
                }
                let _ = fs::remove_dir(&link_tmp);
            }
        });
        FlipperGuard {
            stop,
            handle: Some(handle),
        }
    }

    #[test]
    fn open_contained_file_rejects_ordinary_root_dir_swap() {
        let (base, _real_parent, real_ws) = setup_win_race_tree("win-id");
        let fixed_root = fs::canonicalize(&real_ws).unwrap();
        let wrong_anchor = anchor_workspace_root(&base.join("outside").join("ws")).unwrap();
        let err = open_contained_file(&fixed_root, &["doc.md".to_owned()], Some(wrong_anchor.id))
            .expect_err("ordinary directory swap must be rejected");
        assert!(err.contains("root identity changed"), "{err}");
        let live_anchor = anchor_workspace_root(&real_ws).unwrap();
        assert!(
            fs::rename(&real_ws, base.join("ws_moved")).is_err(),
            "FILE_LIST_DIRECTORY anchor must block directory rename"
        );
        drop(live_anchor);
        let sub_path = real_ws.join("sub").join("doc.md");
        let (_, text) = read_contained_utf8(sub_path.to_str().unwrap(), &real_ws, 1024).unwrap();
        assert_eq!(text, "IN_SAFE");
        drop(wrong_anchor);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn open_contained_file_rejects_and_races_workspace_root_junction() {
        let (base, real_parent, real_ws) = setup_win_race_tree("win");
        let junction = base.join("junc");
        assert!(mk_junction(&junction, &real_parent));
        let err = open_contained_file(&junction.join("ws"), &["doc.md".to_owned()], None)
            .expect_err("junction ancestor above workspace root must fail");
        assert!(err.contains("reparse point refused"), "{err}");

        let fixed_root = fs::canonicalize(&real_ws).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let did_swap = Arc::new(AtomicBool::new(false));
        let _guard = spawn_win_flipper(base.clone(), stop, did_swap.clone());
        let mut count = 0_usize;
        let parts = ["sub".to_owned(), "doc.md".to_owned()];
        while count < 20 || !did_swap.load(Ordering::Acquire) {
            if let Ok((_, file)) = open_contained_file(&fixed_root, &parts, None) {
                let text = read_to_string(file).unwrap_or_default();
                assert!(!text.contains("SECRET_OUT"), "leaked secret: {text}");
            }
            count += 1;
            std::thread::yield_now();
        }
        drop(_guard);
        let _ = fs::remove_dir_all(&base);
    }
}
