// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Windows handle-pinned workspace containment helpers for [`super`].

use super::{check_regular_leaf, strip_workspace_prefix};
use apollinaire_ascii::Result;
use std::{
    fs::File,
    io,
    path::{Component, Path, PathBuf},
};

pub(super) struct RootAnchor {
    _handle: File,
    pub(super) id: (u64, u64),
}

fn check_no_reparse_or_symlink(meta: &std::fs::Metadata, part: &str) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    if meta.file_type().is_symlink() || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    {
        return Err(format!("Cannot safely open {part}: reparse point refused"));
    }
    Ok(())
}

fn file_identity(file: &File, label: &str) -> Result<(u64, u64)> {
    let info = winapi_util::file::information(file)
        .map_err(|e| format!("Cannot inspect file identity for {label}: {e}"))?;
    Ok((info.volume_serial_number(), info.file_index()))
}

fn open_attr_handle(
    path: &Path,
    access: u32,
    extra_flags: u32,
    allow_delete_share: bool,
) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
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
        .access_mode(access)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | extra_flags)
        .share_mode(share)
        .open(path)
}

fn open_pinned_dir(
    path: &Path,
    part: &str,
    allow_delete_share: bool,
    allow_attr_fallback: bool,
) -> Result<File> {
    const FILE_LIST_DIRECTORY: u32 = 0x0001;
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let dir = match open_attr_handle(
        path,
        FILE_LIST_DIRECTORY,
        FILE_FLAG_BACKUP_SEMANTICS,
        allow_delete_share,
    ) {
        Ok(f) => f,
        Err(e) if allow_attr_fallback && e.kind() == io::ErrorKind::PermissionDenied => {
            open_attr_handle(
                path,
                FILE_READ_ATTRIBUTES,
                FILE_FLAG_BACKUP_SEMANTICS,
                allow_delete_share,
            )
            .map_err(|e2| format!("Cannot safely open {part}: {e2}"))?
        }
        Err(e) => return Err(format!("Cannot safely open {part}: {e}")),
    };
    let meta = dir
        .metadata()
        .map_err(|e| format!("Cannot safely open {part}: {e}"))?;
    check_no_reparse_or_symlink(&meta, part)?;
    if !meta.file_type().is_dir() {
        return Err(format!("Parent path component is not a directory: {part}"));
    }
    Ok(dir)
}

fn pin_root_ancestors(root_real: &Path) -> Result<(Vec<File>, (u64, u64))> {
    let normal_count = root_real
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    let mut seen_normal = 0_usize;
    let mut cur = PathBuf::new();
    let mut pinned = Vec::new();
    for comp in root_real.components() {
        match comp {
            Component::Prefix(p) => cur.push(p.as_os_str()),
            Component::RootDir => {
                cur.push(comp.as_os_str());
                pinned.push(open_pinned_dir(&cur, ".", true, true)?);
            }
            Component::Normal(seg) => {
                cur.push(seg);
                seen_normal += 1;
                let is_root = seen_normal == normal_count;
                let label = seg.to_string_lossy();
                pinned.push(open_pinned_dir(&cur, &label, false, !is_root)?);
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

fn verify_handle_containment(
    root_real: &Path,
    leaf_path: &Path,
    root_id: (u64, u64),
    leaf: &str,
    leaf_id: (u64, u64),
) -> Result<()> {
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    let cant_open = |e: io::Error| format!("Cannot safely open {leaf}: {e}");
    let resolved = std::fs::canonicalize(leaf_path).map_err(cant_open)?;
    let Some(rel_parts) = strip_workspace_prefix(&resolved, root_real, root_real) else {
        return Err(format!(
            "Refused path outside workspace ({}).",
            root_real.display()
        ));
    };
    let check_root = open_pinned_dir(root_real, ".", false, false)?;
    if file_identity(&check_root, ".")? != root_id {
        return Err(format!(
            "Cannot resolve workspace folder {}: root changed during verification",
            root_real.display()
        ));
    }
    let mut verify_cur = root_real.to_path_buf();
    let mut verify_dirs = Vec::new();
    if let Some(parent) = rel_parts.parent() {
        for seg in parent.components() {
            if let Component::Normal(s) = seg {
                verify_cur.push(s);
                let label = s.to_string_lossy();
                verify_dirs.push(open_pinned_dir(&verify_cur, &label, false, false)?);
            }
        }
    }
    let check_leaf =
        open_attr_handle(&resolved, FILE_READ_ATTRIBUTES, 0, false).map_err(cant_open)?;
    if file_identity(&check_leaf, leaf)? != leaf_id {
        return Err(format!(
            "Cannot safely open {leaf}: path changed during verification"
        ));
    }
    drop(verify_dirs);
    Ok(())
}

pub(super) fn anchor_workspace_root(base_dir: &Path) -> Result<RootAnchor> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_LIST_DIRECTORY: u32 = 0x0001;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;

    let cant_resolve = |e: io::Error| {
        format!(
            "Cannot resolve workspace folder {}: {e}",
            base_dir.display()
        )
    };
    let handle = File::options()
        .access_mode(FILE_LIST_DIRECTORY)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(base_dir)
        .map_err(cant_resolve)?;
    let meta = handle.metadata().map_err(cant_resolve)?;
    if !meta.file_type().is_dir() {
        return Err(format!(
            "Cannot resolve workspace folder {}: not a directory",
            base_dir.display()
        ));
    }
    let id = file_identity(&handle, ".")?;
    Ok(RootAnchor {
        _handle: handle,
        id,
    })
}

pub(super) fn open_contained_file(
    root_real: &Path,
    parts: &[String],
    expected_root_id: Option<(u64, u64)>,
) -> Result<(String, File)> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;

    let (leaf, ancestors) = parts.split_last().expect("parts is non-empty");
    let (mut pinned, root_id) = pin_root_ancestors(root_real)?;
    if expected_root_id.is_some_and(|exp| exp != root_id) {
        return Err(format!(
            "Cannot resolve workspace folder {}: root identity changed",
            root_real.display()
        ));
    }
    let mut cur = root_real.to_path_buf();
    for part in ancestors {
        cur.push(part);
        pinned.push(open_pinned_dir(&cur, part, false, false)?);
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
