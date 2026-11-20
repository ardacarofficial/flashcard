//! Test support: builds the example plugins of `plugins-sdk/` (Rust → `wasm32-unknown-unknown`)
//! once per test process and packs them as `.fcplugin`.
//!
//! Needs the `wasm32-unknown-unknown` target (listed in `rust-toolchain.toml`, so rustup
//! installs it). The build uses its own target folder (`target/plugin-examples`). It does not
//! pass `--locked`: the SDK builds the app's contract crates from source, so a dependency
//! change there must not break plugin tests (the SDK lock follows).

use crate::package::{pack_dir, PackageError};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Repository root.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// Folder of example plugin `name` (`plugins-sdk/examples/<name>`).
pub fn example_dir(name: &str) -> PathBuf {
    repo_root().join("plugins-sdk/examples").join(name)
}

fn build() -> Result<PathBuf, String> {
    let root = repo_root();
    let target_dir = root.join("target/plugin-examples");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(root.join("plugins-sdk"))
        .args([
            "build",
            "--release",
            "--workspace",
            "--target",
            "wasm32-unknown-unknown",
            "--target-dir",
        ])
        .arg(&target_dir)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "building the example plugins failed (is the wasm32-unknown-unknown target installed?):\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(target_dir.join("wasm32-unknown-unknown/release"))
}

/// WASM of example plugin `name` (built on first use).
pub fn example_wasm(name: &str) -> Result<Vec<u8>, String> {
    static OUT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    let dir = OUT.get_or_init(build).clone()?;
    let file = dir.join(format!("{}.wasm", name.replace('-', "_")));
    std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))
}

/// `.fcplugin` bytes of example plugin `name` (its folder's manifest, settings, locales).
pub fn example_package(name: &str) -> Result<Vec<u8>, String> {
    let wasm = example_wasm(name)?;
    pack_dir(&example_dir(name), Some(&wasm)).map_err(|e: PackageError| e.to_string())
}

/// Binary WASM from the text format (fixtures written by hand).
pub fn wat2wasm(wat: &[u8]) -> Result<Vec<u8>, String> {
    wat::parse_bytes(wat)
        .map(|b| b.into_owned())
        .map_err(|e| e.to_string())
}

/// A `.fcplugin` written without any validation (what a hostile packer produces): `files`
/// go into the zip as given. Tests use it to check that the host, not the packer, refuses
/// a bad manifest.
pub fn pack_unchecked(files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(*name, SimpleFileOptions::default())
            .map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())?;
    }
    zip.finish()
        .map(std::io::Cursor::into_inner)
        .map_err(|e| e.to_string())
}
