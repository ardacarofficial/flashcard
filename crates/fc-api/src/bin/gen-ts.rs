//! Writes (or with `--check`, verifies) the generated TypeScript API types.
//!
//! ```text
//! cargo run -p fc-api --bin gen-ts            # write apps/desktop/src/lib/api/generated
//! cargo run -p fc-api --bin gen-ts -- --check # exit 1 if out of date (CI)
//! cargo run -p fc-api --bin gen-ts -- --out <dir>
//! ```
#![allow(clippy::print_stdout)]

use fc_api::tsgen;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut check = false;
    let mut out = tsgen::workspace_root().join(tsgen::DEFAULT_OUT_DIR);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--check" => check = true,
            "--out" => match args.next() {
                Some(p) => out = PathBuf::from(p),
                None => {
                    eprintln!("--out needs a directory");
                    return ExitCode::from(2);
                }
            },
            other => {
                eprintln!("unknown argument {other:?} (use --check, --out <dir>)");
                return ExitCode::from(2);
            }
        }
    }
    let result = if check {
        tsgen::check(&out)
            .map(|()| println!("TypeScript API types are up to date ({})", out.display()))
    } else {
        tsgen::write(&out).map(|files| println!("wrote {} files to {}", files.len(), out.display()))
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
