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

    let dir_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let leaf_flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    let map_open_err = |label: &str, err: Errno| -> String {
        let detail = if err == Errno::NOTDIR {
            "component is not a real directory or is a symlink".to_string()
        } else {
            io::Error::from(err).to_string()
        };
        format!("Cannot safely open {label}: {detail}")
    };

    let mut dir_fd = openat(CWD, root_real, dir_flags, Mode::empty()).map_err(|e| {
        format!(
            "Cannot resolve workspace folder {}: {}",
            root_real.display(),
            io::Error::from(e)
        )
    })?;
    let (leaf, ancestors) = parts.split_last().expect("parts is non-empty");
    for part in ancestors {
        dir_fd = openat(&dir_fd, part.as_str(), dir_flags, Mode::empty())
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
fn open_pinned_dir(path: &Path, part: &str) -> Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;

    let dir = File::options()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(FILE_SHARE_READ)
        .open(path)
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
fn open_contained_file(root_real: &Path, parts: &[String]) -> Result<(String, File)> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    let (leaf, ancestors) = parts.split_last().expect("parts is non-empty");
    let mut pinned = Vec::with_capacity(ancestors.len() + 1);
    pinned.push(open_pinned_dir(root_real, ".")?);
    let mut cur = root_real.to_path_buf();
    for part in ancestors {
        cur.push(part);
        pinned.push(open_pinned_dir(&cur, part)?);
    }
    cur.push(leaf);
    let cant_open = |e: io::Error| format!("Cannot safely open {leaf}: {e}");
    let file = File::options()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&cur)
        .map_err(cant_open)?;
    let opened_meta = file.metadata().map_err(cant_open)?;
    check_no_reparse_or_symlink(&opened_meta, leaf)?;
    check_regular_leaf(&opened_meta, leaf)?;
    let resolved = std::fs::canonicalize(&cur).map_err(cant_open)?;
    if strip_workspace_prefix(&resolved, root_real, root_real).is_none() {
        return Err(format!(
            "Refused path outside workspace ({}).",
            root_real.display()
        ));
    }
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
