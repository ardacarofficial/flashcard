//! Embeds the CLI message catalog: every `locales/<lang>/<feature>.json` file, sorted by
//! language then file name, becomes one entry of `LOCALE_FILES` (in `OUT_DIR/locales.rs`).
//! A feature adds its own file; nothing else is edited.

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files: Vec<(String, String, PathBuf)> = Vec::new();
    for lang in sorted_entries(&root)? {
        if !lang.is_dir() {
            continue;
        }
        println!("cargo:rerun-if-changed={}", lang.display());
        let lang_name = file_name(&lang)?;
        for file in sorted_entries(&lang)? {
            if file.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stem = file
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("non-UTF-8 locale file name")?
                .to_owned();
            files.push((lang_name.clone(), stem, file));
        }
    }
    let mut out = String::from(
        "/// `(language, feature, JSON)` of every catalog file, sorted.\n\
         pub(crate) const LOCALE_FILES: &[(&str, &str, &str)] = &[\n",
    );
    for (lang, feature, path) in &files {
        writeln!(
            out,
            "    ({lang:?}, {feature:?}, include_str!({:?})),",
            path.display().to_string()
        )?;
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR")?).join("locales.rs");
    fs::write(dest, out)?;
    Ok(())
}

fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut v = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    v.sort();
    Ok(v)
}

fn file_name(p: &Path) -> Result<String, Box<dyn Error>> {
    Ok(p.file_name()
        .and_then(|s| s.to_str())
        .ok_or("non-UTF-8 directory name")?
        .to_owned())
}
