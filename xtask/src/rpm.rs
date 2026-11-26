// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Build an RPM package from an unpacked Linux release folder. `gzip` compresses the manual
//! page and `rpmbuild` builds the package from the staged files; nothing is compiled.

#[cfg(not(unix))]
pub fn build(
    _src: &std::path::Path,
    _version: &str,
    _arch: &str,
    _out: &std::path::Path,
) -> Result<(), String> {
    Err("Building an .rpm needs Linux, with gzip and rpmbuild".into())
}

#[cfg(unix)]
pub use unix::build;

#[cfg(unix)]
mod unix {
    use crate::deb::unix::{Layout, stage};
    use crate::shared::fail as io;
    use std::{fs, path::Path, process::Command};

    pub fn build(src: &Path, version: &str, arch: &str, out: &Path) -> Result<(), String> {
        if !matches!(arch, "x86_64" | "aarch64") {
            return Err(format!(
                "An .rpm is built for x86_64 or aarch64, not {arch}"
            ));
        }
        // RPM sorts 0.5.0~rc.1 before 0.5.0 and refuses a '-' in a version.
        if version.is_empty()
            || !version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'~' | b'+' | b'_'))
        {
            return Err(format!(
                "An RPM version holds letters, digits, '.', '~', '+' and '_', not '{version}'"
            ));
        }
        // Only a folder this run created is removed when the build ends.
        let staging = crate::shared::TempDir::new("apollinaire-rpm")?;
        plain(&staging.0)?;
        let root = staging.0.join("root");
        stage(
            src,
            &root,
            &Layout {
                zsh: "zsh/site-functions/_apollinaire",
                licence: "licenses/apollinaire/LICENSE.md",
                licences: "licenses/apollinaire/LICENSES",
            },
        )?;
        let spec = staging.0.join("apollinaire.spec");
        fs::write(&spec, spec_text(version, &root)).map_err(io(&spec))?;
        let top = staging.0.join("top");
        let status = Command::new("rpmbuild")
            .arg("-bb")
            .args(["--target", arch])
            .arg("--define")
            .arg(format!("_topdir {}", top.display()))
            .arg("--define")
            .arg("_build_name_fmt apollinaire.rpm")
            .arg(&spec)
            .status()
            .map_err(|error| format!("Cannot run rpmbuild: {error}"))?;
        if !status.success() {
            return Err(format!("rpmbuild failed: {status}"));
        }
        let built = top.join("RPMS/apollinaire.rpm");
        fs::copy(&built, out).map_err(io(&built))?;
        Ok(())
    }

    /// The staging folder goes into the spec's shell script and an rpmbuild macro, so it must
    /// hold nothing a shell or rpm reads as syntax: a quote or a `%` would break or inject.
    pub(super) fn plain(path: &Path) -> Result<(), String> {
        let text = path.to_string_lossy();
        if text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
        {
            Ok(())
        } else {
            Err(format!(
                "An .rpm is staged only under a folder named with letters, digits, '/', '.', '_' and '-', not {text}; set TMPDIR to one"
            ))
        }
    }

    /// What the package ships: this project (MIT), the bundled FIGlet (BSD-3-Clause) and TOIlet
    /// (WTFPL) fonts, and the licences of the crates linked into the Linux binary, from
    /// `cargo metadata --filter-platform x86_64-unknown-linux-musl`. Their texts ship as
    /// `DEPENDENCY_NOTICES.txt`.
    pub(crate) const LICENCE: &str = concat!(
        "MIT AND BSD-3-Clause AND WTFPL AND Apache-2.0 AND ISC AND Unicode-3.0",
        " AND CDLA-Permissive-2.0 AND Zlib AND (MIT OR Apache-2.0)",
        " AND (0BSD OR MIT OR Apache-2.0) AND (Apache-2.0 OR ISC OR MIT)",
        " AND (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT)",
        " AND (BSD-3-Clause OR Apache-2.0) AND (MIT OR Apache-2.0 OR Zlib)",
        " AND (Unlicense OR MIT)"
    );

    /// The spec file: the staged tree copied as it is, with no build step, no stripping and no
    /// automatic dependencies, as the binary is linked statically.
    pub(super) fn spec_text(version: &str, root: &Path) -> String {
        format!(
            "Name: apollinaire
Version: {version}
Release: 1
Summary: Static art, block lettering and animation as real ASCII text
License: {LICENCE}
URL: https://yungsnat.github.io/ApollinaireASCII/
AutoReqProv: no
%define __os_install_post %{{nil}}
%define _build_id_links none
%define debug_package %{{nil}}

%description
The apollinaire command draws, letters, converts and animates artwork made
only of printable ASCII characters. Includes its manual page and bash, zsh
and fish completions.

%install
mkdir -p %{{buildroot}}
cp -a '{}'/. %{{buildroot}}/

%files
%defattr(-,root,root,-)
/usr/bin/apollinaire
/usr/share/man/man1/apollinaire.1.gz
/usr/share/bash-completion/completions/apollinaire
/usr/share/zsh/site-functions/_apollinaire
/usr/share/fish/vendor_completions.d/apollinaire.fish
/usr/share/doc/apollinaire
%license /usr/share/licenses/apollinaire
",
            root.display()
        )
    }
}

#[cfg(all(test, unix))]
mod tests {
    use crate::deb::tests::{BUILD, output, release};
    use std::{fs, process::Command};

    #[test]
    fn refuses_an_unknown_architecture_or_a_version_rpm_cannot_hold() {
        let here = std::path::Path::new(".");
        let arch = super::build(here, "0.5.0", "amd64", here).unwrap_err();
        assert_eq!(arch, "An .rpm is built for x86_64 or aarch64, not amd64");
        let version = super::build(here, "0.5.0-rc.1", "x86_64", here).unwrap_err();
        assert_eq!(
            version,
            "An RPM version holds letters, digits, '.', '~', '+' and '_', not '0.5.0-rc.1'"
        );
        let plain = |path: &str| super::unix::plain(std::path::Path::new(path));
        assert!(plain("/tmp/apollinaire-rpm-12.x_y").is_ok());
        for path in ["/tmp/it's", "/tmp/%{nil}", "/tmp/a b", "/tmp/$(id)"] {
            assert!(
                plain(path)
                    .unwrap_err()
                    .ends_with(&format!("not {path}; set TMPDIR to one"))
            );
        }
    }

    /// Needs rpmbuild, which the checks workflow installs.
    #[test]
    fn builds_a_package_rpm_reads_back() {
        let _build = BUILD.lock().unwrap_or_else(|e| e.into_inner());
        // nosemgrep: rust.lang.security.temp-dir.temp-dir -- a test fixture folder; no security decision rests on it
        let dir = std::env::temp_dir().join(format!("xtask-rpm-{}", std::process::id()));
        release(&dir.join("release"));
        let rpm = dir.join("apollinaire.rpm");
        super::build(&dir.join("release"), "0.5.0~rc.1", "aarch64", &rpm).unwrap();
        let fields = output(
            Command::new("rpm")
                .args([
                    "-qp",
                    "--queryformat",
                    "%{NAME} %{VERSION} %{RELEASE} %{ARCH}",
                ])
                .arg(&rpm),
        );
        assert_eq!(fields, "apollinaire 0.5.0~rc.1 1 aarch64");
        let licence = output(
            Command::new("rpm")
                .args(["-qp", "--queryformat", "%{LICENSE}"])
                .arg(&rpm),
        );
        assert_eq!(licence, super::unix::LICENCE);
        // Marked %license, so an install with tsflags=nodocs keeps them.
        let licences = output(Command::new("rpm").args(["-qLp"]).arg(&rpm));
        for file in ["DEPENDENCY_NOTICES.txt", "LICENSE.md", "LICENSES/MIT.txt"] {
            let path = format!("/usr/share/licenses/apollinaire/{file}");
            assert!(licences.lines().any(|line| line == path), "{licences}");
        }
        assert!(!licences.contains("/usr/share/doc/"), "{licences}");
        let requires = output(Command::new("rpm").args(["-qp", "--requires"]).arg(&rpm));
        assert!(
            requires.lines().all(|line| line.starts_with("rpmlib(")),
            "{requires}"
        );
        let listing = output(
            Command::new("rpm")
                .args([
                    "-qp",
                    "--queryformat",
                    "[%{FILEMODES:perms} %{FILEUSERNAME}/%{FILEGROUPNAME} %{FILENAMES}\\n]",
                ])
                .arg(&rpm),
        );
        let mut entries: Vec<&str> = listing.lines().collect();
        entries.sort_by_key(|line| line.rsplit(' ').next().unwrap_or_default());
        assert_eq!(
            entries,
            [
                "-rwxr-xr-x root/root /usr/bin/apollinaire",
                "-rw-r--r-- root/root /usr/share/bash-completion/completions/apollinaire",
                "drwxr-xr-x root/root /usr/share/doc/apollinaire",
                "-rw-r--r-- root/root /usr/share/doc/apollinaire/DEPENDENCY_NOTICES.txt",
                "-rw-r--r-- root/root /usr/share/doc/apollinaire/README.md",
                "-rw-r--r-- root/root /usr/share/doc/apollinaire/THIRD_PARTY_NOTICES.md",
                "-rw-r--r-- root/root /usr/share/fish/vendor_completions.d/apollinaire.fish",
                "drwxr-xr-x root/root /usr/share/licenses/apollinaire",
                "-rw-r--r-- root/root /usr/share/licenses/apollinaire/DEPENDENCY_NOTICES.txt",
                "-rw-r--r-- root/root /usr/share/licenses/apollinaire/LICENSE.md",
                "drwxr-xr-x root/root /usr/share/licenses/apollinaire/LICENSES",
                "-rw-r--r-- root/root /usr/share/licenses/apollinaire/LICENSES/MIT.txt",
                "drwxr-xr-x root/root /usr/share/licenses/apollinaire/LICENSES/nested",
                "-rw-r--r-- root/root /usr/share/licenses/apollinaire/LICENSES/nested/Apache-2.0.txt",
                "-rw-r--r-- root/root /usr/share/man/man1/apollinaire.1.gz",
                "-rw-r--r-- root/root /usr/share/zsh/site-functions/_apollinaire",
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
