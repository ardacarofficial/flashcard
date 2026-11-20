//! Plugin store infrastructure (docs/05 §9). Where packages come from is pluggable; which
//! remote source to use is an open decision (docs/08 K3). Built in: [`FileSource`] — local
//! `.fcplugin` files (a single file the user picked, or folders of packages).

use crate::manifest::{parse_version, Manifest};
use crate::package::{Package, PackageError, EXTENSION};
use std::path::{Path, PathBuf};

/// A package a source offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEntry {
    /// Plugin id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Source-specific location (a file path for [`FileSource`]).
    pub location: String,
}

/// Failure of a source.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SourceError {
    /// Nothing at that location.
    #[error("not found: {0}")]
    NotFound(String),
    /// The package is invalid.
    #[error(transparent)]
    Package(#[from] PackageError),
    /// I/O or transport failure.
    #[error("{0}")]
    Io(String),
}

/// A place plugins are installed from.
pub trait PluginRegistrySource: Send + Sync {
    /// Source id (`file`, …).
    fn id(&self) -> &str;
    /// Packages matching `query` (name/id/description substring; empty = all).
    fn search(&self, query: &str) -> Result<Vec<SourceEntry>, SourceError>;
    /// Manifest of the package at `location` (validated).
    fn get_manifest(&self, location: &str) -> Result<Manifest, SourceError>;
    /// Package bytes.
    fn download(&self, location: &str) -> Result<Vec<u8>, SourceError>;
    /// Newer versions of installed plugins (`(id, version)` pairs).
    fn check_updates(
        &self,
        installed: &[(String, String)],
    ) -> Result<Vec<SourceEntry>, SourceError>;
}

/// Local files: `location` is a path; `search`/`check_updates` scan the configured folders.
#[derive(Debug, Clone)]
pub struct FileSource {
    app_version: String,
    folders: Vec<PathBuf>,
}

impl FileSource {
    /// A source validating packages against `app_version`, scanning `folders`.
    pub fn new(app_version: impl Into<String>, folders: Vec<PathBuf>) -> Self {
        Self {
            app_version: app_version.into(),
            folders,
        }
    }

    /// Reads and validates the package at `location`.
    pub fn read(&self, location: &str) -> Result<Package, SourceError> {
        let bytes = self.download(location)?;
        Ok(Package::read(&bytes, &self.app_version)?)
    }

    fn scan(&self) -> Vec<SourceEntry> {
        let mut out = Vec::new();
        for folder in &self.folders {
            let Ok(entries) = std::fs::read_dir(folder) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            paths.sort();
            for p in paths {
                if p.extension().is_none_or(|e| e != EXTENSION) {
                    continue;
                }
                // Invalid packages are skipped: a folder scan never fails on one bad file.
                if let Ok(pkg) = Package::read_file(&p, &self.app_version) {
                    out.push(entry(&pkg.manifest, &p));
                }
            }
        }
        out
    }
}

fn entry(m: &Manifest, path: &Path) -> SourceEntry {
    SourceEntry {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        location: path.display().to_string(),
    }
}

impl PluginRegistrySource for FileSource {
    fn id(&self) -> &str {
        "file"
    }

    fn search(&self, query: &str) -> Result<Vec<SourceEntry>, SourceError> {
        let q = query.to_lowercase();
        Ok(self
            .scan()
            .into_iter()
            .filter(|e| {
                q.is_empty()
                    || e.id.contains(&q)
                    || e.name.to_lowercase().contains(&q)
                    || e.description.to_lowercase().contains(&q)
            })
            .collect())
    }

    fn get_manifest(&self, location: &str) -> Result<Manifest, SourceError> {
        Ok(self.read(location)?.manifest)
    }

    fn download(&self, location: &str) -> Result<Vec<u8>, SourceError> {
        let path = Path::new(location);
        if !path.is_file() {
            return Err(SourceError::NotFound(location.to_owned()));
        }
        std::fs::read(path).map_err(|e| SourceError::Io(format!("{location}: {e}")))
    }

    fn check_updates(
        &self,
        installed: &[(String, String)],
    ) -> Result<Vec<SourceEntry>, SourceError> {
        Ok(self
            .scan()
            .into_iter()
            .filter(|e| {
                installed
                    .iter()
                    .any(|(id, v)| id == &e.id && parse_version(&e.version) > parse_version(v))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::pack;

    fn manifest(version: &str) -> String {
        format!("id = \"com.example.src\"\nname = \"Source demo\"\nversion = \"{version}\"\napi_version = \"1\"\n")
    }

    #[test]
    fn file_source_searches_reads_and_finds_updates() {
        let dir = tempfile::tempdir().unwrap();
        let wasm = b"\0asm\x01\0\0\0";
        std::fs::write(
            dir.path().join("a.fcplugin"),
            pack(&manifest("1.1.0"), wasm, &[]).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.path().join("broken.fcplugin"), b"junk").unwrap();
        std::fs::write(dir.path().join("other.txt"), b"x").unwrap();
        let src = FileSource::new("0.7.0", vec![dir.path().to_path_buf()]);
        let found = src.search("demo").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(src.search("nothing").unwrap().len(), 0);
        let m = src.get_manifest(&found[0].location).unwrap();
        assert_eq!(m.version, "1.1.0");
        assert_eq!(
            src.check_updates(&[("com.example.src".into(), "1.0.0".into())])
                .unwrap()
                .len(),
            1
        );
        assert!(src
            .check_updates(&[("com.example.src".into(), "1.1.0".into())])
            .unwrap()
            .is_empty());
        assert!(matches!(
            src.download("/no/such/file"),
            Err(SourceError::NotFound(_))
        ));
        let broken = dir.path().join("broken.fcplugin").display().to_string();
        assert!(matches!(
            src.get_manifest(&broken),
            Err(SourceError::Package(_))
        ));
    }
}
