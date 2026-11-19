//! Collects `migrations/NNNN_name.sql` into `OUT_DIR/migrations.rs` (the `MIGRATIONS` list,
//! ascending by number), so adding a migration is adding a file. Fails the build on a
//! malformed file name or a duplicate number. Number ranges per release are checked by the
//! `migrations_stay_in_their_release_range` test.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("migrations");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut found: BTreeMap<u32, (String, PathBuf)> = BTreeMap::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("non-UTF-8 migration file name")?
            .to_owned();
        let Some(stem) = name.strip_suffix(".sql") else {
            return Err(format!("migrations/{name}: only NNNN_name.sql files belong here").into());
        };
        let (digits, rest) = stem.split_at(stem.find('_').unwrap_or(0));
        let valid = digits.len() == 4
            && digits.bytes().all(|b| b.is_ascii_digit())
            && rest.len() > 1
            && rest[1..]
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !valid {
            return Err(format!("migrations/{name}: expected NNNN_snake_case_name.sql").into());
        }
        let version: u32 = digits.parse()?;
        if version == 0 {
            return Err(format!("migrations/{name}: numbers start at 0001").into());
        }
        if let Some((other, _)) = found.insert(version, (stem.to_owned(), path.clone())) {
            return Err(format!("migrations {other} and {stem} share number {version}").into());
        }
    }
    let mut out = String::from(
        "/// All migrations, ascending by version (generated from `migrations/` by `build.rs`).\n\
         pub const MIGRATIONS: &[Migration] = &[\n",
    );
    for (version, (name, path)) in &found {
        writeln!(
            out,
            "    Migration {{ version: {version}, name: {name:?}, sql: include_str!({:?}) }},",
            path.display().to_string()
        )?;
    }
    out.push_str("];\n");
    fs::write(
        PathBuf::from(std::env::var("OUT_DIR")?).join("migrations.rs"),
        out,
    )?;
    Ok(())
}
