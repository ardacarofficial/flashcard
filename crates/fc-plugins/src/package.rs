//! `.fcplugin` packages: a zip with `plugin.toml`, the WASM entry, an optional settings schema
//! (`contributes.settings`, JSON array of setting definitions with plugin-local keys) and
//! optional translations (`contributes.locales/<lang>.json`, flat `key → text`).
//!
//! Reading is bounded (entry and total size caps) and validates everything up front: a
//! package that reads is installable.

use crate::manifest::{Manifest, ManifestError};
use fc_core::settings::{validate_setting_value, SettingDef, SettingScopeKind};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Manifest file name inside a package.
pub const MANIFEST_FILE: &str = "plugin.toml";
/// Package file extension.
pub const EXTENSION: &str = "fcplugin";
/// Largest WASM entry.
pub const MAX_WASM_BYTES: u64 = 32 * 1024 * 1024;
/// Largest other entry (manifest, schema, one locale file).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Most locale files.
pub const MAX_LOCALES: usize = 32;
/// Most settings in a plugin's schema.
pub const MAX_SETTINGS: usize = 100;

/// A read, validated package.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    /// Parsed manifest.
    pub manifest: Manifest,
    /// The WASM module.
    pub wasm: Vec<u8>,
    /// Settings schema (keys local to the plugin, scope `plugin`).
    pub settings: Vec<SettingDef>,
    /// Translations: language → key → text.
    pub locales: BTreeMap<String, BTreeMap<String, String>>,
    /// Lowercase hex SHA-256 of `wasm` (cache key of the compiled module).
    pub sha256: String,
}

/// Why a package was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageError {
    /// Not a readable package (zip error, missing or oversized file, bad JSON…).
    #[error("invalid plugin package: {0}")]
    Invalid(String),
    /// The manifest is invalid.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

fn invalid(msg: impl Into<String>) -> PackageError {
    PackageError::Invalid(msg.into())
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read_entry(
    zip: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, PackageError> {
    let file = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(invalid(format!("{name}: {e}"))),
    };
    if file.size() > limit {
        return Err(invalid(format!("{name} is larger than {limit} bytes")));
    }
    let mut out = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|e| invalid(format!("{name}: {e}")))?;
    if out.len() as u64 > limit {
        return Err(invalid(format!("{name} is larger than {limit} bytes")));
    }
    Ok(Some(out))
}

fn utf8(name: &str, bytes: Vec<u8>) -> Result<String, PackageError> {
    String::from_utf8(bytes).map_err(|_| invalid(format!("{name} is not UTF-8")))
}

/// Validates a settings schema of plugin-local keys; forces scope `plugin`.
pub fn validate_settings(mut defs: Vec<SettingDef>) -> Result<Vec<SettingDef>, PackageError> {
    if defs.len() > MAX_SETTINGS {
        return Err(invalid(format!("settings: at most {MAX_SETTINGS} entries")));
    }
    let mut seen = std::collections::BTreeSet::new();
    for d in &mut defs {
        let ok_key = !d.key.is_empty()
            && d.key.len() <= 64
            && d.key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.');
        if !ok_key {
            return Err(invalid(format!("settings: invalid key {:?}", d.key)));
        }
        if !seen.insert(d.key.clone()) {
            return Err(invalid(format!("settings: {} listed twice", d.key)));
        }
        validate_setting_value(d, &d.default)
            .map_err(|e| invalid(format!("settings: default of {}: {e}", d.key)))?;
        d.scopes = vec![SettingScopeKind::Plugin];
    }
    Ok(defs)
}

impl Package {
    /// Reads and validates a package against `app_version`.
    pub fn read(bytes: &[u8], app_version: &str) -> Result<Self, PackageError> {
        let mut zip =
            ZipArchive::new(Cursor::new(bytes)).map_err(|e| invalid(format!("not a zip: {e}")))?;
        let manifest_bytes = read_entry(&mut zip, MANIFEST_FILE, MAX_FILE_BYTES)?
            .ok_or_else(|| invalid(format!("{MANIFEST_FILE} is missing")))?;
        let manifest = Manifest::parse(&utf8(MANIFEST_FILE, manifest_bytes)?, app_version)?;
        let wasm = read_entry(&mut zip, &manifest.entry, MAX_WASM_BYTES)?
            .ok_or_else(|| invalid(format!("{} is missing", manifest.entry)))?;
        if !wasm.starts_with(b"\0asm") {
            return Err(invalid(format!("{} is not a WASM module", manifest.entry)));
        }
        let settings = match &manifest.contributes.settings {
            Some(path) => {
                let raw = read_entry(&mut zip, path, MAX_FILE_BYTES)?
                    .ok_or_else(|| invalid(format!("{path} is missing")))?;
                let defs: Vec<SettingDef> =
                    serde_json::from_slice(&raw).map_err(|e| invalid(format!("{path}: {e}")))?;
                validate_settings(defs)?
            }
            None => Vec::new(),
        };
        let mut locales = BTreeMap::new();
        if let Some(dir) = &manifest.contributes.locales {
            let prefix = format!("{}/", dir.trim_end_matches('/'));
            let names: Vec<String> = zip
                .file_names()
                .filter_map(Result::ok)
                .filter(|n| n.starts_with(&prefix) && n.ends_with(".json"))
                .map(|n| n.into_owned())
                .collect();
            if names.len() > MAX_LOCALES {
                return Err(invalid("too many locale files"));
            }
            for name in names {
                let lang = name[prefix.len()..name.len() - ".json".len()].to_owned();
                if lang.is_empty()
                    || lang.len() > 16
                    || !lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                {
                    return Err(invalid(format!("{name}: invalid language")));
                }
                let raw = read_entry(&mut zip, &name, MAX_FILE_BYTES)?.unwrap_or_default();
                let map: BTreeMap<String, String> = serde_json::from_slice(&raw).map_err(|e| {
                    invalid(format!("{name}: expected a flat key → text object: {e}"))
                })?;
                locales.insert(lang, map);
            }
        }
        let sha256 = sha256_hex(&wasm);
        Ok(Self {
            manifest,
            wasm,
            settings,
            locales,
            sha256,
        })
    }

    /// Reads a package file.
    pub fn read_file(path: &Path, app_version: &str) -> Result<Self, PackageError> {
        let bytes = std::fs::read(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        Self::read(&bytes, app_version)
    }
}

/// Builds a package from parts (the inverse of [`Package::read`]; used by `fc plugin pack`
/// and tests).
pub fn pack(
    manifest_toml: &str,
    wasm: &[u8],
    extra: &[(&str, &[u8])],
) -> Result<Vec<u8>, PackageError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let mut add = |name: &str, bytes: &[u8]| -> Result<(), PackageError> {
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file(name, options)
            .map_err(|e| invalid(e.to_string()))?;
        zip.write_all(bytes).map_err(|e| invalid(e.to_string()))
    };
    add(MANIFEST_FILE, manifest_toml.as_bytes())?;
    let manifest: Manifest =
        toml::from_str(manifest_toml).map_err(|e| ManifestError::Parse(e.to_string()))?;
    add(&manifest.entry, wasm)?;
    for (name, bytes) in extra {
        add(name, bytes)?;
    }
    zip.finish()
        .map(Cursor::into_inner)
        .map_err(|e| invalid(e.to_string()))
}

/// Packs a plugin folder: `plugin.toml`, its entry (relative to the folder, or `wasm`
/// overriding it), the settings file and the locale folder it names.
pub fn pack_dir(dir: &Path, wasm: Option<&[u8]>) -> Result<Vec<u8>, PackageError> {
    let read = |p: &Path| std::fs::read(p).map_err(|e| invalid(format!("{}: {e}", p.display())));
    let manifest_toml = utf8(MANIFEST_FILE, read(&dir.join(MANIFEST_FILE))?)?;
    let manifest: Manifest =
        toml::from_str(&manifest_toml).map_err(|e| ManifestError::Parse(e.to_string()))?;
    let wasm = match wasm {
        Some(w) => w.to_vec(),
        None => read(&dir.join(&manifest.entry))?,
    };
    let mut extra: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(s) = &manifest.contributes.settings {
        extra.push((s.clone(), read(&dir.join(s))?));
    }
    if let Some(l) = &manifest.contributes.locales {
        let l = l.trim_end_matches('/');
        if let Ok(entries) = std::fs::read_dir(dir.join(l)) {
            let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            files.sort();
            for p in files {
                if p.extension().is_some_and(|e| e == "json") {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        extra.push((format!("{l}/{name}"), read(&p)?));
                    }
                }
            }
        }
    }
    let extra_refs: Vec<(&str, &[u8])> = extra
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    pack(&manifest_toml, &wasm, &extra_refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
id = "com.example.pkg"
name = "Pkg"
version = "0.1.0"
api_version = "1"
[contributes]
settings = "settings.schema.json"
locales = "locales"
"#;

    const SETTINGS: &str = r#"[{"key":"target","kind":{"type":"choice","options":["front","back"]},"default":"back","label_key":"settings.target","description_key":null,"advanced":false,"scopes":["app"]}]"#;

    #[test]
    fn roundtrip() {
        let wasm = b"\0asm\x01\0\0\0";
        let bytes = pack(
            MANIFEST,
            wasm,
            &[
                ("settings.schema.json", SETTINGS.as_bytes()),
                ("locales/tr.json", br#"{"settings.target":"Hedef alan"}"#),
                ("locales/en.json", br#"{"settings.target":"Target field"}"#),
            ],
        )
        .unwrap();
        let p = Package::read(&bytes, "0.7.0").unwrap();
        assert_eq!(p.manifest.id, "com.example.pkg");
        assert_eq!(p.wasm, wasm);
        assert_eq!(p.sha256, sha256_hex(wasm));
        assert_eq!(p.settings.len(), 1);
        assert_eq!(p.settings[0].scopes, vec![SettingScopeKind::Plugin]);
        assert_eq!(p.locales["tr"]["settings.target"], "Hedef alan");
    }

    #[test]
    fn rejects_broken_packages() {
        assert!(matches!(
            Package::read(b"nope", "0.7.0"),
            Err(PackageError::Invalid(_))
        ));
        let no_wasm = pack(
            MANIFEST,
            b"not wasm",
            &[("settings.schema.json", SETTINGS.as_bytes())],
        )
        .unwrap();
        assert!(matches!(
            Package::read(&no_wasm, "0.7.0"),
            Err(PackageError::Invalid(_))
        ));
        let bad_default = SETTINGS.replace("\"default\":\"back\"", "\"default\":\"side\"");
        let bytes = pack(
            MANIFEST,
            b"\0asm\x01\0\0\0",
            &[("settings.schema.json", bad_default.as_bytes())],
        )
        .unwrap();
        assert!(matches!(
            Package::read(&bytes, "0.7.0"),
            Err(PackageError::Invalid(_))
        ));
        let old_api = MANIFEST.replace("api_version = \"1\"", "api_version = \"9\"");
        let bytes = pack(&old_api, b"\0asm\x01\0\0\0", &[]).unwrap();
        assert!(matches!(
            Package::read(&bytes, "0.7.0"),
            Err(PackageError::Manifest(
                ManifestError::UnsupportedApiVersion(_)
            ))
        ));
    }
}
