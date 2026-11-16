// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Build a Debian package from an unpacked Linux release folder. `gzip` compresses the manual
//! page and `dpkg-deb` builds the archive.

#[cfg(not(unix))]
pub fn build(
    _src: &std::path::Path,
    _version: &str,
    _arch: &str,
    _out: &std::path::Path,
) -> Result<(), String> {
    Err("Building a .deb needs Linux, with gzip and dpkg-deb".into())
}

#[cfg(unix)]
pub use unix::build;

#[cfg(unix)]
mod unix {
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    /// A staging folder, removed when dropped.
    struct Staging(PathBuf);

    impl Drop for Staging {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn io(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
        move |error| format!("{}: {error}", path.display())
    }

    /// `install -Dm<mode>`: copy a file, making its folders, and set its mode.
    fn install(from: &Path, to: &Path, mode: u32) -> Result<(), String> {
        fs::create_dir_all(to.parent().expect("A file in a folder")).map_err(io(to))?;
        fs::copy(from, to).map_err(io(from))?;
        fs::set_permissions(to, fs::Permissions::from_mode(mode)).map_err(io(to))
    }

    fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
        fs::create_dir_all(to).map_err(io(to))?;
        for entry in fs::read_dir(from).map_err(io(from))? {
            let entry = entry.map_err(io(from))?;
            let target = to.join(entry.file_name());
            if entry.file_type().map_err(io(&entry.path()))?.is_dir() {
                copy_tree(&entry.path(), &target)?;
            } else {
                fs::copy(entry.path(), &target).map_err(io(&entry.path()))?;
            }
        }
        Ok(())
    }

    /// `chmod -R u=rwX,go=rX`: folders and files anyone may run get 755, other files 644.
    fn normalise(path: &Path) -> Result<(), String> {
        let meta = fs::symlink_metadata(path).map_err(io(path))?;
        let mode = if meta.is_dir() || meta.mode() & 0o111 != 0 {
            0o755
        } else {
            0o644
        };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(io(path))?;
        if meta.is_dir() {
            for entry in fs::read_dir(path).map_err(io(path))? {
                normalise(&entry.map_err(io(path))?.path())?;
            }
        }
        Ok(())
    }

    /// What `du -sk` reports: the blocks allocated to the tree, folders included, in KiB rounded
    /// up. A file with several links in the tree counts once.
    pub(super) fn installed_kib(path: &Path) -> Result<u64, String> {
        fn walk(path: &Path, seen: &mut Vec<(u64, u64)>) -> Result<u64, String> {
            let meta = fs::symlink_metadata(path).map_err(io(path))?;
            if meta.nlink() > 1 && !meta.is_dir() {
                if seen.contains(&(meta.dev(), meta.ino())) {
                    return Ok(0);
                }
                seen.push((meta.dev(), meta.ino()));
            }
            let mut blocks = meta.blocks();
            if meta.is_dir() {
                for entry in fs::read_dir(path).map_err(io(path))? {
                    blocks += walk(&entry.map_err(io(path))?.path(), seen)?;
                }
            }
            Ok(blocks)
        }
        // st_blocks counts 512-byte units.
        Ok(walk(path, &mut Vec::new())?.div_ceil(2))
    }

    fn run(command: &mut Command) -> Result<(), String> {
        let status = command
            .status()
            .map_err(|error| format!("Cannot run {:?}: {error}", command.get_program()))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{:?} failed: {status}", command.get_program()))
        }
    }

    pub fn build(src: &Path, version: &str, arch: &str, out: &Path) -> Result<(), String> {
        let staging =
            Staging(std::env::temp_dir().join(format!("apollinaire-deb-{}", std::process::id())));
        let root = &staging.0;
        fs::create_dir(root).map_err(io(root))?;
        let usr = root.join("usr");
        let share = usr.join("share");
        install(
            &src.join("apollinaire"),
            &usr.join("bin/apollinaire"),
            0o755,
        )?;
        let man = share.join("man/man1");
        fs::create_dir_all(&man).map_err(io(&man))?;
        let page = src.join("man/apollinaire.1");
        let gz = man.join("apollinaire.1.gz");
        run(Command::new("gzip")
            .arg("-9n")
            .stdin(fs::File::open(&page).map_err(io(&page))?)
            .stdout(fs::File::create(&gz).map_err(io(&gz))?))?;
        let completions = src.join("completions");
        for (from, to) in [
            (
                "apollinaire.bash",
                "bash-completion/completions/apollinaire",
            ),
            ("_apollinaire", "zsh/vendor-completions/_apollinaire"),
            (
                "apollinaire.fish",
                "fish/vendor_completions.d/apollinaire.fish",
            ),
        ] {
            install(&completions.join(from), &share.join(to), 0o644)?;
        }
        let doc = share.join("doc/apollinaire");
        install(&src.join("LICENSE.md"), &doc.join("copyright"), 0o644)?;
        for name in [
            "README.md",
            "THIRD_PARTY_NOTICES.md",
            "DEPENDENCY_NOTICES.txt",
        ] {
            install(&src.join(name), &doc.join(name), 0o644)?;
        }
        copy_tree(&src.join("LICENSES"), &doc.join("LICENSES"))?;
        normalise(root)?;
        let control = control(version, arch, installed_kib(&usr)?);
        let debian = root.join("DEBIAN");
        fs::create_dir(&debian).map_err(io(&debian))?;
        fs::write(debian.join("control"), control).map_err(io(&debian))?;
        normalise(&debian)?;
        run(Command::new("dpkg-deb")
            .args(["--build", "--root-owner-group"])
            .arg(root)
            .arg(out)
            .stdin(Stdio::null()))
    }

    pub(super) fn control(version: &str, arch: &str, kib: u64) -> String {
        format!(
            "Package: apollinaire
Version: {version}
Architecture: {arch}
Maintainer: yungSNAT <333319867+yungSNAT@users.noreply.github.com>
Installed-Size: {kib}
Section: graphics
Priority: optional
Homepage: https://yungsnat.github.io/ApollinaireASCII/
Description: Static art, block lettering and animation as real ASCII text
 The apollinaire command draws, letters, converts and animates artwork made
 only of printable ASCII characters. Includes its manual page and bash, zsh
 and fish completions.
"
        )
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::unix::{control, installed_kib};
    use std::{fs, path::Path, process::Command};

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// A release folder as the release workflow unpacks it, with a stand-in binary.
    fn release(dir: &Path) {
        write(&dir.join("apollinaire"), "#!/bin/sh\necho apollinaire\n");
        fs::set_permissions(
            dir.join("apollinaire"),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        write(&dir.join("man/apollinaire.1"), ".TH APOLLINAIRE 1\n");
        for name in ["apollinaire.bash", "_apollinaire", "apollinaire.fish"] {
            write(&dir.join("completions").join(name), name);
        }
        for name in [
            "LICENSE.md",
            "README.md",
            "THIRD_PARTY_NOTICES.md",
            "DEPENDENCY_NOTICES.txt",
        ] {
            write(&dir.join(name), &name.repeat(500));
        }
        // install -m644 drops an execute bit a document had.
        fs::set_permissions(
            dir.join("README.md"),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        write(&dir.join("LICENSES/MIT.txt"), "MIT");
        write(&dir.join("LICENSES/nested/Apache-2.0.txt"), "Apache");
    }

    fn output(command: &mut Command) -> String {
        let out = command.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    #[test]
    fn installed_size_matches_du() {
        let dir = std::env::temp_dir().join(format!("xtask-du-{}", std::process::id()));
        release(&dir);
        write(&dir.join("big"), &"x".repeat(70_000));
        fs::hard_link(dir.join("big"), dir.join("big-link")).unwrap();
        let du = output(Command::new("du").arg("-sk").arg(&dir));
        let du: u64 = du.split_whitespace().next().unwrap().parse().unwrap();
        assert_eq!(installed_kib(&dir).unwrap(), du);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Needs dpkg-deb, as the release workflow's Linux runners have.
    #[test]
    fn builds_the_package_deb_sh_built() {
        let dir = std::env::temp_dir().join(format!("xtask-deb-{}", std::process::id()));
        release(&dir.join("release"));
        let deb = dir.join("apollinaire.deb");
        super::build(&dir.join("release"), "0.4.1~rc.1", "arm64", &deb).unwrap();
        let fields = output(Command::new("dpkg-deb").arg("--field").arg(&deb));
        let size = fields
            .lines()
            .find_map(|line| line.strip_prefix("Installed-Size: "))
            .unwrap();
        assert_eq!(
            fields,
            control("0.4.1~rc.1", "arm64", size.parse().unwrap())
        );
        let listing = output(Command::new("dpkg-deb").arg("--contents").arg(&deb));
        let mut entries: Vec<(&str, String)> = listing
            .lines()
            .map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                (
                    fields[5],
                    format!("{} {} {}", fields[0], fields[1], fields[5]),
                )
            })
            .collect();
        entries.sort();
        let entries: Vec<String> = entries.into_iter().map(|(_, entry)| entry).collect();
        let expected = [
            "drwxr-xr-x root/root ./",
            "drwxr-xr-x root/root ./usr/",
            "drwxr-xr-x root/root ./usr/bin/",
            "-rwxr-xr-x root/root ./usr/bin/apollinaire",
            "drwxr-xr-x root/root ./usr/share/",
            "drwxr-xr-x root/root ./usr/share/bash-completion/",
            "drwxr-xr-x root/root ./usr/share/bash-completion/completions/",
            "-rw-r--r-- root/root ./usr/share/bash-completion/completions/apollinaire",
            "drwxr-xr-x root/root ./usr/share/doc/",
            "drwxr-xr-x root/root ./usr/share/doc/apollinaire/",
            "-rw-r--r-- root/root ./usr/share/doc/apollinaire/DEPENDENCY_NOTICES.txt",
            "drwxr-xr-x root/root ./usr/share/doc/apollinaire/LICENSES/",
            "-rw-r--r-- root/root ./usr/share/doc/apollinaire/LICENSES/MIT.txt",
            "drwxr-xr-x root/root ./usr/share/doc/apollinaire/LICENSES/nested/",
            "-rw-r--r-- root/root ./usr/share/doc/apollinaire/LICENSES/nested/Apache-2.0.txt",
            "-rw-r--r-- root/root ./usr/share/doc/apollinaire/README.md",
            "-rw-r--r-- root/root ./usr/share/doc/apollinaire/THIRD_PARTY_NOTICES.md",
            "-rw-r--r-- root/root ./usr/share/doc/apollinaire/copyright",
            "drwxr-xr-x root/root ./usr/share/fish/",
            "drwxr-xr-x root/root ./usr/share/fish/vendor_completions.d/",
            "-rw-r--r-- root/root ./usr/share/fish/vendor_completions.d/apollinaire.fish",
            "drwxr-xr-x root/root ./usr/share/man/",
            "drwxr-xr-x root/root ./usr/share/man/man1/",
            "-rw-r--r-- root/root ./usr/share/man/man1/apollinaire.1.gz",
            "drwxr-xr-x root/root ./usr/share/zsh/",
            "drwxr-xr-x root/root ./usr/share/zsh/vendor-completions/",
            "-rw-r--r-- root/root ./usr/share/zsh/vendor-completions/_apollinaire",
        ];
        assert_eq!(entries, expected);
        let unpacked = dir.join("unpacked");
        output(
            Command::new("dpkg-deb")
                .arg("--extract")
                .arg(&deb)
                .arg(&unpacked),
        );
        let page = output(
            Command::new("gzip")
                .args(["-dc"])
                .arg(unpacked.join("usr/share/man/man1/apollinaire.1.gz")),
        );
        assert_eq!(page, ".TH APOLLINAIRE 1\n");
        let copyright = fs::read_to_string(unpacked.join("usr/share/doc/apollinaire/copyright"));
        assert_eq!(copyright.unwrap(), "LICENSE.md".repeat(500));
        fs::remove_dir_all(&dir).unwrap();
    }
}
