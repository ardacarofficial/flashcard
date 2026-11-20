//! Example: a `before:notes.create` hook filling an empty field from a glossary.
//! Shows a hook changing params, a typed host query (`notetypes.get`), plugin settings and
//! plugin storage.

use fc_plugin_sdk::api::v1::methods::NotetypesGet;
use fc_plugin_sdk::api::v1::notetypes::NoteTypeRef;
use fc_plugin_sdk::prelude::*;

const GLOSSARY: &[(&str, &str)] = &[
    ("sword", "kılıç"),
    ("shield", "kalkan"),
    ("book", "kitap"),
    ("water", "su"),
];

#[derive(Default)]
pub struct AutoFill;

fn translate(word: &str) -> Option<&'static str> {
    let w = word.trim().to_lowercase();
    GLOSSARY.iter().find(|(en, _)| *en == w).map(|(_, tr)| *tr)
}

impl Plugin for AutoFill {
    fn before_hook(&self, _hook: &str, _method: &str, mut params: Value) -> Result<BeforeHook> {
        let settings = host::settings()?;
        let get = |k: &str, d: &str| {
            settings
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or(d)
                .to_owned()
        };
        let (source, target) = (get("source_field", "front"), get("target_field", "back"));
        let word = params["fields"][&source]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let empty = params["fields"][&target]
            .as_str()
            .is_none_or(|v| v.trim().is_empty());
        let Some(meaning) = translate(&word).filter(|_| empty) else {
            return Ok(BeforeHook::Continue(None));
        };
        // Only for note types that have the target field (typed host query).
        let notetype_id = serde_json::from_value(params["notetype_id"].clone())
            .map_err(|e| Error::new("INVALID_PARAMS", e.to_string()))?;
        let nt = host::call::<NotetypesGet>(&NoteTypeRef { id: notetype_id })?;
        if !nt.fields.iter().any(|f| f.key.as_str() == target) {
            return Ok(BeforeHook::Continue(None));
        }
        params["fields"][&target] = json!(meaning);
        let n: u64 = host::kv::get("filled")?.unwrap_or(0);
        host::kv::set("filled", &(n + 1))?;
        host::log("info", &format!("filled {target} of {word:?}"));
        Ok(BeforeHook::Continue(Some(params)))
    }

    fn command(&self, command: &str, _params: Value) -> Result<Value> {
        match command {
            "filled-count" => Ok(json!(host::kv::get::<u64>("filled")?.unwrap_or(0))),
            other => Err(Error::unsupported(other)),
        }
    }
}

export_plugin!(AutoFill);

#[cfg(test)]
mod tests {
    use super::*;
    use fc_plugin_sdk::testing::with_host;

    #[test]
    fn words_outside_the_glossary_are_left_alone() {
        let out = with_host(
            |f, _| match f {
                "settings.get" => Ok(json!({})),
                _ => Err(Error::new("PERMISSION_DENIED", f)),
            },
            || {
                AutoFill.before_hook(
                    "before:notes.create",
                    "notes.create",
                    json!({"fields": {"front": "zebra", "back": ""}}),
                )
            },
        );
        assert_eq!(out.unwrap(), BeforeHook::Continue(None));
    }
}
