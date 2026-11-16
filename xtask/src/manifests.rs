// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Write a Homebrew formula and a Scoop manifest for a release from its SHA256SUMS.txt.
use std::{fs, path::Path};

const SUMMARY: &str = "Static art, block lettering and animation as real ASCII text";
const HOMEPAGE: &str = "https://yungsnat.github.io/ApollinaireASCII/";
const REPOSITORY: &str = "https://github.com/yungSNAT/ApollinaireASCII";

pub fn write(tag: &str, sums: &Path, out: &Path) -> Result<(), String> {
    let text = fs::read_to_string(sums)
        .map_err(|error| format!("Cannot read {}: {error}", sums.display()))?;
    let (formula, scoop) = render(tag, &text, &sums.display().to_string())?;
    fs::create_dir_all(out).map_err(|error| format!("Cannot create {}: {error}", out.display()))?;
    for (name, content) in [("apollinaire.rb", formula), ("apollinaire.json", scoop)] {
        let path = out.join(name);
        fs::write(&path, content)
            .map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
    }
    Ok(())
}

/// The formula and the manifest; `source` names the sums file in an error.
fn render(tag: &str, sums: &str, source: &str) -> Result<(String, String), String> {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    let base = format!("{REPOSITORY}/releases/download/{tag}");
    let sha = |target: &str| checksum(sums, &format!("apollinaire-{tag}-{target}"), source);
    let mac_arm = sha("aarch64-apple-darwin.tar.gz")?;
    let mac_intel = sha("x86_64-apple-darwin.tar.gz")?;
    let linux_arm = sha("aarch64-unknown-linux-musl.tar.gz")?;
    let linux_intel = sha("x86_64-unknown-linux-musl.tar.gz")?;
    let windows_x64 = sha("x86_64-pc-windows-msvc.zip")?;
    let windows_arm = sha("aarch64-pc-windows-msvc.zip")?;
    let formula = format!(
        r##"class Apollinaire < Formula
  desc "{SUMMARY}"
  homepage "{HOMEPAGE}"
  version "{version}"
  license "MIT"

  on_macos do
    on_arm do
      url "{base}/apollinaire-{tag}-aarch64-apple-darwin.tar.gz"
      sha256 "{mac_arm}"
    end
    on_intel do
      url "{base}/apollinaire-{tag}-x86_64-apple-darwin.tar.gz"
      sha256 "{mac_intel}"
    end
  end
  on_linux do
    on_arm do
      url "{base}/apollinaire-{tag}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "{linux_arm}"
    end
    on_intel do
      url "{base}/apollinaire-{tag}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "{linux_intel}"
    end
  end

  def install
    bin.install "apollinaire"
    man1.install "man/apollinaire.1"
    bash_completion.install "completions/apollinaire.bash" => "apollinaire"
    zsh_completion.install "completions/_apollinaire"
    fish_completion.install "completions/apollinaire.fish"
    doc.install "README.md", "THIRD_PARTY_NOTICES.md", "DEPENDENCY_NOTICES.txt", "LICENSES"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/apollinaire --version")
  end
end
"##
    );
    let windows = |target: &str, hash: &str| {
        Json::Obj(vec![
            (
                "url",
                format!("{base}/apollinaire-{tag}-{target}.zip").into(),
            ),
            ("hash", hash.into()),
            ("extract_dir", format!("apollinaire-{tag}-{target}").into()),
        ])
    };
    // Scoop fills in $version, $baseurl and $dir itself.
    let update = |target: &str| {
        Json::Obj(vec![
            (
                "url",
                format!(
                    "{REPOSITORY}/releases/download/v$version/apollinaire-v$version-{target}.zip"
                )
                .into(),
            ),
            (
                "extract_dir",
                format!("apollinaire-v$version-{target}").into(),
            ),
        ])
    };
    let scoop = Json::Obj(vec![
        ("version", version.into()),
        ("description", SUMMARY.into()),
        ("homepage", HOMEPAGE.into()),
        ("license", "MIT".into()),
        (
            "architecture",
            Json::Obj(vec![
                ("64bit", windows("x86_64-pc-windows-msvc", windows_x64)),
                ("arm64", windows("aarch64-pc-windows-msvc", windows_arm)),
            ]),
        ),
        ("bin", "apollinaire.exe".into()),
        (
            "notes",
            r#"For PowerShell completions, add this line to your profile: . "$dir\completions\apollinaire.ps1""#.into(),
        ),
        ("checkver", Json::Obj(vec![("github", REPOSITORY.into())])),
        (
            "autoupdate",
            Json::Obj(vec![
                (
                    "architecture",
                    Json::Obj(vec![
                        ("64bit", update("x86_64-pc-windows-msvc")),
                        ("arm64", update("aarch64-pc-windows-msvc")),
                    ]),
                ),
                ("hash", Json::Obj(vec![("url", "$baseurl/SHA256SUMS.txt".into())])),
            ]),
        ),
    ]);
    let mut manifest = String::new();
    scoop.write(&mut manifest, 0);
    manifest.push('\n');
    Ok((formula, manifest))
}

/// The checksum of one release archive: the first field of the one line whose second field is
/// `file`. A missing, repeated or malformed entry is an error.
fn checksum<'a>(sums: &'a str, file: &str, source: &str) -> Result<&'a str, String> {
    let mut found = sums.lines().filter_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        (fields.next() == Some(file)).then_some(hash)
    });
    match (found.next(), found.next()) {
        (Some(hash), None)
            if hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) =>
        {
            Ok(hash)
        }
        _ => Err(format!("No checksum for {file} in {source}")),
    }
}

/// Just enough JSON for the manifest, written as `jq` writes it: 2-space indents, keys in order.
enum Json {
    Str(String),
    Obj(Vec<(&'static str, Json)>),
}

impl From<&str> for Json {
    fn from(text: &str) -> Self {
        Json::Str(text.into())
    }
}

impl From<String> for Json {
    fn from(text: String) -> Self {
        Json::Str(text)
    }
}

impl Json {
    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Json::Str(text) => quote(out, text),
            Json::Obj(fields) => {
                out.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    out.push_str(if index == 0 { "\n" } else { ",\n" });
                    out.push_str(&"  ".repeat(depth + 1));
                    quote(out, key);
                    out.push_str(": ");
                    value.write(out, depth + 1);
                }
                out.push('\n');
                out.push_str(&"  ".repeat(depth));
                out.push('}');
            }
        }
    }
}

/// A JSON string as jq escapes it.
fn quote(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' || c == '\u{7f}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMS: &str = include_str!("../tests/golden/SHA256SUMS.txt");

    /// The files manifests.sh wrote from the same sums, before it was replaced.
    #[test]
    fn writes_what_the_shell_script_wrote() {
        let (formula, scoop) = render("v0.4.1", SUMS, "SHA256SUMS.txt").unwrap();
        assert_eq!(formula, include_str!("../tests/golden/apollinaire.rb"));
        assert_eq!(scoop, include_str!("../tests/golden/apollinaire.json"));
    }

    #[test]
    fn refuses_a_missing_repeated_or_malformed_checksum() {
        let line = "apollinaire-v0.4.1-aarch64-pc-windows-msvc.zip";
        let error = format!("No checksum for {line} in sums");
        let without: String = SUMS
            .lines()
            .filter(|l| !l.ends_with(line))
            .map(|l| format!("{l}\n"))
            .collect();
        let hash = "a".repeat(64);
        for sums in [
            without.clone(),
            format!("{SUMS}{hash}  {line}\n"),
            format!("{without}{}  {line}\n", "A".repeat(64)),
            format!("{without}{}  {line}\n", "a".repeat(63)),
            format!("{without}{hash}  *{line}\n"),
        ] {
            assert_eq!(render("v0.4.1", &sums, "sums").unwrap_err(), error);
        }
        // Any whitespace separates the fields, as awk splits them.
        let tabbed = format!("{without}{hash}\t{line} extra\n");
        assert!(render("v0.4.1", &tabbed, "sums").is_ok());
    }

    #[test]
    fn strings_are_escaped_as_jq_escapes_them() {
        let mut out = String::new();
        quote(&mut out, "a\"b\\c\n\t\r\u{8}\u{c}\u{1}\u{7f}é");
        assert_eq!(out, r#""a\"b\\c\n\t\r\b\f\u0001\u007fé""#);
    }

    #[test]
    fn writes_the_files_and_reports_an_unreadable_sums_file() {
        let dir = std::env::temp_dir().join(format!("xtask-manifests-{}", std::process::id()));
        let sums = dir.join("SHA256SUMS.txt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&sums, SUMS).unwrap();
        write("v0.4.1", &sums, &dir.join("out")).unwrap();
        let rb = fs::read_to_string(dir.join("out/apollinaire.rb")).unwrap();
        assert_eq!(rb, include_str!("../tests/golden/apollinaire.rb"));
        let missing = dir.join("missing.txt");
        assert!(
            write("v0.4.1", &missing, &dir)
                .unwrap_err()
                .starts_with("Cannot read")
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
