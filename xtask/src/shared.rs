// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Helpers the xtask commands share: running a program and a temporary folder.
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Run a program in `dir` with extra environment variables, failing on a non-zero exit, and
/// return its output when `stdout` is piped. Its stderr reaches the terminal.
pub fn exec(
    dir: &Path,
    env: &[(&str, &str)],
    program: impl AsRef<Path>,
    args: &[&str],
    stdout: Stdio,
) -> Result<Vec<u8>, String> {
    exec_with(dir, env, program, args, None, stdout)
}

/// Run a program in `dir` with extra environment variables and `input` piped to stdin.
pub fn exec_stdin(
    dir: &Path,
    env: &[(&str, &str)],
    program: impl AsRef<Path>,
    args: &[&str],
    input: &[u8],
) -> Result<Vec<u8>, String> {
    exec_with(dir, env, program, args, Some(input), Stdio::piped())
}

fn exec_with(
    dir: &Path,
    env: &[(&str, &str)],
    program: impl AsRef<Path>,
    args: &[&str],
    input: Option<&[u8]>,
    stdout: Stdio,
) -> Result<Vec<u8>, String> {
    let program = program.as_ref();
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(dir)
        .envs(env.iter().copied())
        .stdout(stdout)
        .stderr(Stdio::inherit());
    let out = match input {
        Some(bytes) => {
            let mut child = command
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|error| format!("Cannot run {}: {error}", program.display()))?;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(bytes);
            }
            child
                .wait_with_output()
                .map_err(|error| format!("Cannot run {}: {error}", program.display()))?
        }
        None => command
            .output()
            .map_err(|error| format!("Cannot run {}: {error}", program.display()))?,
    };
    if !out.status.success() {
        return Err(format!(
            "{} {} failed: {}",
            program.display(),
            args.join(" "),
            out.status
        ));
    }
    Ok(out.stdout)
}

/// Print the first line a program writes, as `| head -n 1` does.
pub fn first_line(
    dir: &Path,
    env: &[(&str, &str)],
    program: impl AsRef<Path>,
    args: &[&str],
) -> Result<(), String> {
    let out = exec(dir, env, program, args, Stdio::piped())?;
    println!(
        "{}",
        String::from_utf8_lossy(&out)
            .lines()
            .next()
            .unwrap_or_default()
    );
    Ok(())
}

pub fn fail(path: &Path) -> impl Fn(io::Error) -> String + '_ {
    move |error| format!("{}: {error}", path.display())
}

/// A folder under the temporary folder, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    /// Refuses a path that already exists, so a planted folder or link is never used. On Unix
    /// only the owner can use it, as with `mktemp -d`.
    pub fn new(name: &str) -> Result<Self, String> {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir -- create fails on an existing path, so a planted folder or link stops the run
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&path).map_err(fail(&path))?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_already_there_is_refused_and_kept() {
        let name = "apollinaire-shared-test";
        // nosemgrep: rust.lang.security.temp-dir.temp-dir -- the path TempDir uses, planted by the test
        let planted = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        fs::create_dir_all(&planted).unwrap();
        fs::write(planted.join("planted.el"), "").unwrap();
        assert!(TempDir::new(name).is_err());
        assert!(planted.join("planted.el").exists());
        fs::remove_dir_all(&planted).unwrap();
        let fresh = TempDir::new(name).unwrap();
        assert!(fresh.0.is_dir());
        drop(fresh);
        assert!(!planted.exists());
    }

    /// As `mktemp -d`: only the owner can use the folder, whatever the umask.
    #[cfg(unix)]
    #[test]
    fn the_folder_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("apollinaire-shared-mode").unwrap();
        let mode = fs::metadata(&dir.0).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "{mode:o}");
    }
}
