//! Assembles the `ErrorCode` list from `src/error_codes/*.in` (one file per domain, in
//! file-name order) into `OUT_DIR/error_codes.rs`, which `src/error.rs` includes. A feature
//! adds its codes by adding its own file; no shared list is edited.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("src/error_codes");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files = fs::read_dir(&dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|p| p.extension().and_then(|e| e.to_str()) == Some("in"));
    files.sort();
    let mut out = String::from("error_codes! {\n");
    for file in &files {
        println!("cargo:rerun-if-changed={}", file.display());
        out.push_str(&format!("// ---- {}\n", file.display()));
        out.push_str(&fs::read_to_string(file)?);
        out.push('\n');
    }
    out.push_str("}\n");
    fs::write(
        PathBuf::from(std::env::var("OUT_DIR")?).join("error_codes.rs"),
        out,
    )?;
    Ok(())
}
