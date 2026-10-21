//! What git knows about this build, for the About panel.
//!
//! `CARGO_PKG_VERSION` only moves when someone edits `Cargo.toml`: two builds
//! sixteen commits apart, or one with uncommitted changes, would look the same.
//! `git describe` tells them apart, and the commit log says who wrote the thing
//! better than an `authors` field nobody remembers to update.
//!
//! Outside a git checkout (a source tarball, a vendored build) both are left
//! empty and the panel falls back to what Cargo says.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest.join("../..");

    let describe = git(&workspace, &["describe", "--always", "--dirty", "--tags"]).unwrap_or_default();
    println!("cargo:rustc-env=CIE_GIT_DESCRIBE={}", describe.trim());
    // One line per variable, so the list travels joined: `;` because a name can
    // hold a comma ("Rossi, Mario") far more easily than a semicolon.
    println!("cargo:rustc-env=CIE_GIT_AUTHORS={}", authors(&workspace).join(";"));

    watch(&workspace);
}

/// Everyone with a commit, most commits first. `%aN` goes through `.mailmap`,
/// so one person with two addresses shows up once.
fn authors(workspace: &Path) -> Vec<String> {
    let Some(log) = git(workspace, &["shortlog", "--summary", "--numbered", "--format=%aN", "HEAD"]) else {
        return Vec::new();
    };
    log.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(_, name)| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Reruns the script when the answer could change.
///
/// Without any `rerun-if-changed` Cargo would only watch this crate, and an edit
/// in `cie-lib` would leave the `-dirty` stale. So: the refs and the index for
/// new commits and tags, and the sources themselves for the working tree.
fn watch(workspace: &Path) {
    println!("cargo:rerun-if-changed=build.rs");

    if let Some(git_dir) = git(workspace, &["rev-parse", "--absolute-git-dir"]) {
        let git_dir = PathBuf::from(git_dir.trim());
        for name in ["HEAD", "index", "refs", "packed-refs"] {
            let path = git_dir.join(name);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }

    for name in ["crates", "locales", "patches", "Cargo.toml", "Cargo.lock"] {
        let path = workspace.join(name);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}
