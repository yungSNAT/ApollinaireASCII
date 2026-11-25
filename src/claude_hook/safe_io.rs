// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Workspace-contained Markdown path validation and race-resistant file reads.

use super::read_bounded_utf8;
use apollinaire_ascii::Result;
use std::{
    fs::File,
    io,
    path::{Component, Path, PathBuf},
};

pub(super) fn has_nul(text: &str, label: &str) -> Result<()> {
    if text.contains('\0') {
        return Err(format!("Refused {label} containing NUL byte."));
    }
    Ok(())
}

#[cfg(windows)]
fn strip_prefix_win_ci<'a>(candidate: &'a Path, prefix: &Path) -> Option<&'a Path> {
    let mut cand = candidate.components();
    for p_comp in prefix.components() {
        let c_comp = cand.next()?;
        let equal = match (c_comp, p_comp) {
            (Component::Normal(a), Component::Normal(b)) => {
                a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
            }
            (Component::Prefix(a), Component::Prefix(b)) => {
                a.as_os_str().to_string_lossy().to_lowercase()
                    == b.as_os_str().to_string_lossy().to_lowercase()
            }
            _ => c_comp == p_comp,
        };
        if !equal {
            return None;
        }
    }
    Some(cand.as_path())
}

#[cfg(not(windows))]
fn strip_workspace_prefix<'a>(
    candidate: &'a Path,
    base_abs: &Path,
    root_real: &Path,
) -> Option<&'a Path> {
    candidate
        .strip_prefix(base_abs)
        .ok()
        .or_else(|| candidate.strip_prefix(root_real).ok())
}

#[cfg(windows)]
fn strip_workspace_prefix<'a>(
    candidate: &'a Path,
    base_abs: &Path,
    root_real: &Path,
) -> Option<&'a Path> {
    if let Some(rel) = strip_prefix_win_ci(candidate, base_abs)
        .or_else(|| strip_prefix_win_ci(candidate, root_real))
    {
        return Some(rel);
    }
    root_real
        .to_str()
        .and_then(|s| {
            s.strip_prefix(r"\\?")
                .and_then(|rest| rest.strip_prefix('\\'))
        })
        .filter(|s| {
            !s.strip_prefix("UNC")
                .is_some_and(|rest| rest.starts_with('\\'))
        })
        .and_then(|stripped| strip_prefix_win_ci(candidate, Path::new(stripped)))
}

fn collect_relative_parts(rel: &Path, raw_path: &str, root_real: &Path) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    for comp in rel.components() {
        match comp {
            Component::CurDir => {}
            Component::Normal(seg) => {
                let s = seg.to_string_lossy();
                #[cfg(windows)]
                if s.contains(':') {
                    return Err(format!("Refused unsafe Windows path component: {raw_path}"));
                }
                parts.push(s.into_owned());
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "Refused unsafe path outside workspace ({}): {raw_path}",
                    root_real.display()
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(format!(
            "Refused unsafe path outside workspace ({}): {raw_path}",
            root_real.display()
        ));
    }
    Ok(parts)
}

fn relative_parts(raw_path: &str, base_dir: &Path) -> Result<(PathBuf, Vec<String>)> {
    has_nul(raw_path, "path")?;
    let base_str = base_dir
        .to_str()
        .ok_or_else(|| format!("Workspace path is not valid UTF-8: {}", base_dir.display()))?;
    has_nul(base_str, "workspace path")?;
    let resolve_err = |e: io::Error| {
        format!(
            "Cannot resolve workspace folder {}: {e}",
            base_dir.display()
        )
    };
    let root_real = std::fs::canonicalize(base_dir).map_err(resolve_err)?;
    let base_abs = std::path::absolute(base_dir).map_err(resolve_err)?;
    let candidate = Path::new(raw_path);
    let rel = if candidate.is_absolute() {
        strip_workspace_prefix(candidate, &base_abs, &root_real).ok_or_else(|| {
            format!(
                "Refused path outside workspace ({}): {raw_path}",
                root_real.display()
            )
        })?
    } else {
        candidate
    };
    let parts = collect_relative_parts(rel, raw_path, &root_real)?;
    Ok((root_real, parts))
}

fn check_regular_leaf(meta: &std::fs::Metadata, leaf: &str) -> Result<()> {
    if !meta.file_type().is_file() {
        return Err(format!("Path is not a regular file: {leaf}"));
    }
    Ok(())
}

#[cfg(unix)]
fn open_contained_file(root_real: &Path, parts: &[String]) -> Result<(String, File)> {
    use rustix::fs::{CWD, Mode, OFlags, openat};
    use rustix::io::Errno;

    #[cfg(target_os = "linux")]
    let ancestor_flags = OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    #[cfg(not(target_os = "linux"))]
    let ancestor_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let leaf_flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    let map_root_err = |err: Errno| -> String {
        format!(
            "Cannot resolve workspace folder {}: {}",
            root_real.display(),
            io::Error::from(err)
        )
    };
    let map_open_err = |label: &str, err: Errno| -> String {
        let detail = if err == Errno::NOTDIR {
            "component is not a real directory or is a symlink".to_string()
        } else {
            io::Error::from(err).to_string()
        };
        format!("Cannot safely open {label}: {detail}")
    };

    let mut dir_fd = openat(CWD, "/", ancestor_flags, Mode::empty()).map_err(map_root_err)?;
    for comp in root_real.components() {
        if let Component::Normal(seg) = comp {
            dir_fd = openat(&dir_fd, seg, ancestor_flags, Mode::empty()).map_err(map_root_err)?;
        }
    }
    let (leaf, ancestors) = parts.split_last().expect("parts is non-empty");
    for part in ancestors {
        dir_fd = openat(&dir_fd, part.as_str(), ancestor_flags, Mode::empty())
            .map_err(|e| map_open_err(part, e))?;
    }
    let leaf_fd = openat(&dir_fd, leaf.as_str(), leaf_flags, Mode::empty())
        .map_err(|e| map_open_err(leaf, e))?;
    Ok((leaf.clone(), File::from(leaf_fd)))
}

#[cfg(windows)]
fn check_no_reparse_or_symlink(meta: &std::fs::Metadata, part: &str) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    if meta.file_type().is_symlink() || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    {
        return Err(format!("Cannot safely open {part}: reparse point refused"));
    }
    Ok(())
}

#[cfg(windows)]
fn file_identity(file: &File, label: &str) -> Result<(u64, u64)> {
    let info = winapi_util::file::information(file)
        .map_err(|e| format!("Cannot inspect file identity for {label}: {e}"))?;
    Ok((info.volume_serial_number(), info.file_index()))
}

#[cfg(windows)]
fn open_attr_handle(path: &Path, extra_flags: u32, allow_delete_share: bool) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;

    let share = if allow_delete_share {
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    } else {
        FILE_SHARE_READ | FILE_SHARE_WRITE
    };
    File::options()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | extra_flags)
        .share_mode(share)
        .open(path)
}

#[cfg(windows)]
fn open_pinned_dir(path: &Path, part: &str, allow_delete_share: bool) -> Result<File> {
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let dir = open_attr_handle(path, FILE_FLAG_BACKUP_SEMANTICS, allow_delete_share)
        .map_err(|e| format!("Cannot safely open {part}: {e}"))?;
    let meta = dir
        .metadata()
        .map_err(|e| format!("Cannot safely open {part}: {e}"))?;
    check_no_reparse_or_symlink(&meta, part)?;
    if !meta.file_type().is_dir() {
        return Err(format!("Parent path component is not a directory: {part}"));
    }
    Ok(dir)
}

#[cfg(windows)]
fn pin_root_ancestors(root_real: &Path) -> Result<(Vec<File>, (u64, u64))> {
    let mut cur = PathBuf::new();
    let mut pinned = Vec::new();
    for comp in root_real.components() {
        match comp {
            Component::Prefix(p) => cur.push(p.as_os_str()),
            Component::RootDir => {
                cur.push(comp.as_os_str());
                pinned.push(open_pinned_dir(&cur, ".", true)?);
            }
            Component::Normal(seg) => {
                cur.push(seg);
                let label = seg.to_string_lossy();
                pinned.push(open_pinned_dir(&cur, &label, false)?);
            }
            Component::CurDir | Component::ParentDir => {}
        }
    }
    let root_handle = pinned
        .last()
        .ok_or_else(|| format!("Cannot resolve workspace folder {}", root_real.display()))?;
    let root_id = file_identity(root_handle, ".")?;
    Ok((pinned, root_id))
}

#[cfg(windows)]
fn verify_handle_containment(
    root_real: &Path,
    leaf_path: &Path,
    root_id: (u64, u64),
    leaf: &str,
    leaf_id: (u64, u64),
) -> Result<()> {
    let cant_open = |e: io::Error| format!("Cannot safely open {leaf}: {e}");
    let resolved = std::fs::canonicalize(leaf_path).map_err(cant_open)?;
    if strip_workspace_prefix(&resolved, root_real, root_real).is_none() {
        return Err(format!(
            "Refused path outside workspace ({}).",
            root_real.display()
        ));
    }
    let check_leaf = open_attr_handle(&resolved, 0, false).map_err(cant_open)?;
    if file_identity(&check_leaf, leaf)? != leaf_id {
        return Err(format!(
            "Cannot safely open {leaf}: path changed during verification"
        ));
    }
    let check_root = open_pinned_dir(root_real, ".", false)?;
    if file_identity(&check_root, ".")? != root_id {
        return Err(format!(
            "Cannot resolve workspace folder {}: root changed during verification",
            root_real.display()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn open_contained_file(root_real: &Path, parts: &[String]) -> Result<(String, File)> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;

    let (leaf, ancestors) = parts.split_last().expect("parts is non-empty");
    let (mut pinned, root_id) = pin_root_ancestors(root_real)?;
    let mut cur = root_real.to_path_buf();
    for part in ancestors {
        cur.push(part);
        pinned.push(open_pinned_dir(&cur, part, false)?);
    }
    cur.push(leaf);
    let cant_open = |e: io::Error| format!("Cannot safely open {leaf}: {e}");
    let file = File::options()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&cur)
        .map_err(cant_open)?;
    let opened_meta = file.metadata().map_err(cant_open)?;
    check_no_reparse_or_symlink(&opened_meta, leaf)?;
    check_regular_leaf(&opened_meta, leaf)?;
    let leaf_id = file_identity(&file, leaf)?;
    verify_handle_containment(root_real, &cur, root_id, leaf, leaf_id)?;
    drop(pinned);
    Ok((leaf.clone(), file))
}

pub(super) fn read_contained_utf8(
    raw_path: &str,
    base_dir: &Path,
    max_bytes: usize,
) -> Result<(String, String)> {
    let (root_real, parts) = relative_parts(raw_path, base_dir)?;
    let (leaf, file) = open_contained_file(&root_real, &parts)?;
    let opened_meta = file
        .metadata()
        .map_err(|e| format!("Cannot safely open {leaf}: {e}"))?;
    check_regular_leaf(&opened_meta, &leaf)?;
    let too_large = format!("File exceeds the {max_bytes} byte limit: {leaf}");
    if opened_meta.len() > max_bytes as u64 {
        return Err(too_large);
    }
    let bad_utf8 = format!("File is not valid UTF-8 ({leaf})");
    let text = read_bounded_utf8(file, max_bytes, &too_large, &bad_utf8)?;
    Ok((leaf, text))
}

pub(super) fn is_markdown_path(file_path: &str) -> bool {
    #[cfg(windows)]
    let check_path = {
        let rest = if file_path.len() >= 2
            && file_path.as_bytes()[0].is_ascii_alphabetic()
            && file_path.as_bytes()[1] == b':'
        {
            &file_path[2..]
        } else {
            file_path
        };
        let trimmed = rest.trim_end_matches(['/', '\\']);
        let leaf = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
        leaf.split(':').next().unwrap_or(leaf)
    };
    #[cfg(not(windows))]
    let check_path = file_path;

    Path::new(check_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ext.eq_ignore_ascii_case("md")
                || ext.eq_ignore_ascii_case("markdown")
                || ext.eq_ignore_ascii_case("mdx")
        })
}

#[cfg(all(test, unix))]
mod tests {
    use super::open_contained_file;
    use std::{
        fs,
        io::read_to_string,
        os::unix::fs::symlink,
        path::Path,
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

    #[test]
    fn open_contained_file_rejects_and_races_workspace_root_ancestor_symlink() {
        let base = std::env::temp_dir().join(format!(
            "apollinaire-root-ancestor-unix-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let outside_ws = base.join("outside/ws");
        fs::create_dir_all(&outside_ws).unwrap();
        fs::write(outside_ws.join("doc.md"), "SECRET_OUT").unwrap();

        let real_parent = base.join("real");
        let real_ws = real_parent.join("ws");
        fs::create_dir_all(&real_ws).unwrap();
        fs::write(real_ws.join("doc.md"), "IN_SAFE").unwrap();

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
        let stop_bg = stop.clone();
        let base_bg = base.clone();
        let flipper = std::thread::spawn(move || {
            while !stop_bg.load(Ordering::Relaxed) {
                let link_tmp = base_bg.join("real_link");
                let live = base_bg.join("real");
                let bak = base_bg.join("real_bak");
                let _ = fs::remove_file(&link_tmp);
                let _ = symlink(base_bg.join("outside"), &link_tmp);
                if fs::rename(&live, &bak).is_ok() {
                    let _ = fs::rename(&link_tmp, &live);
                    let _ = fs::remove_file(&live);
                    let _ = fs::rename(&bak, &live);
                }
            }
        });
        for _ in 0..40 {
            check_root_race_outcome(open_contained_file(&fixed_root, &["doc.md".to_owned()]));
        }
        stop.store(true, Ordering::Relaxed);
        let _ = flipper.join();
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::open_contained_file;
    use std::{
        fs,
        io::read_to_string,
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

    #[test]
    fn open_contained_file_rejects_and_races_workspace_root_junction() {
        let base = std::env::temp_dir().join(format!(
            "apollinaire-root-ancestor-win-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let outside_ws = base.join("outside/ws");
        fs::create_dir_all(&outside_ws).unwrap();
        fs::write(outside_ws.join("doc.md"), "SECRET_OUT").unwrap();

        let real_parent = base.join("real");
        let real_ws = real_parent.join("ws");
        fs::create_dir_all(&real_ws).unwrap();
        fs::write(real_ws.join("doc.md"), "IN_SAFE").unwrap();

        let junction = base.join("junc");
        assert!(mk_junction(&junction, &real_parent));
        let err = open_contained_file(&junction.join("ws"), &["doc.md".to_owned()])
            .expect_err("junction ancestor above workspace root must fail");
        assert!(err.contains("reparse point refused"), "{err}");

        let fixed_root = fs::canonicalize(&real_ws).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = stop.clone();
        let base_bg = base.clone();
        let flipper = std::thread::spawn(move || {
            while !stop_bg.load(Ordering::Relaxed) {
                let link_tmp = base_bg.join("real_link");
                let live = base_bg.join("real");
                let bak = base_bg.join("real_bak");
                let _ = fs::remove_dir(&link_tmp);
                if mk_junction(&link_tmp, &base_bg.join("outside"))
                    && fs::rename(&live, &bak).is_ok()
                {
                    let _ = fs::rename(&link_tmp, &live);
                    let _ = fs::remove_dir(&live);
                    let _ = fs::rename(&bak, &live);
                }
                let _ = fs::remove_dir(&link_tmp);
            }
        });
        for _ in 0..40 {
            if let Ok((_, file)) = open_contained_file(&fixed_root, &["doc.md".to_owned()]) {
                let text = read_to_string(file).unwrap_or_default();
                assert!(!text.contains("SECRET_OUT"), "leaked secret: {text}");
            }
        }
        stop.store(true, Ordering::Relaxed);
        let _ = flipper.join();
        let _ = fs::remove_dir_all(&base);
    }
}
