//! Example (v1.4): a read-only view over the word index. Paste a subtitle line; the plugin
//! asks `words.lookup` (typed host call, permission `words:read`) which of its words are new
//! to you, and which you are still learning. It writes nothing.
//!
//! The request and answer shapes are pinned in `plugins-sdk/samples/v1/` (a test in the app
//! checks those samples against the API, so a plugin written from them keeps working).

use fc_plugin_sdk::api::v1::methods::WordsLookup;
use fc_plugin_sdk::api::v1::words::LookupWords;
use fc_plugin_sdk::core::reading::WordStatus;
use fc_plugin_sdk::prelude::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct NewWords;

/// The words of a line, each once (case-insensitive), in order. The app's lemmatizer turns
/// each into its dictionary key (`went` → `go`).
pub fn split(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’' || c == '-')) {
        let w = w.trim_matches(|c: char| !c.is_alphanumeric());
        if w.chars().any(char::is_alphabetic) && !out.iter().any(|o| o.to_lowercase() == w.to_lowercase()) {
            out.push(w.to_owned());
        }
    }
    out
}

fn form() -> Vec<UiNode> {
    vec![
        ui::input("text", Some(ui::key("text"))),
        ui::button("check", ui::key("check"), true),
    ]
}

impl Plugin for NewWords {
    fn ui_open(&self, _view: &str, _session: &str, _input: Value) -> Result<UiReply> {
        Ok(ui::show(Some(ui::key("title")), ui::column(form()), Value::Null))
    }

    fn ui_act(
        &self,
        _view: &str,
        _session: &str,
        action: &str,
        values: BTreeMap<String, Value>,
        _state: Value,
    ) -> Result<UiReply> {
        if action != "check" {
            return Ok(ui::done());
        }
        let text = values.get("text").and_then(Value::as_str).unwrap_or_default();
        let forms: Vec<String> = split(text).into_iter().take(2000).collect();
        let mut children = Vec::new();
        if !forms.is_empty() {
            let r = host::call::<WordsLookup>(&LookupWords {
                language: String::new(),
                forms,
            })?;
            let (mut new, mut learning) = (Vec::<String>::new(), Vec::<String>::new());
            for f in r.forms {
                let Some(key) = f.key else { continue };
                let list = match f.status {
                    WordStatus::New => &mut new,
                    WordStatus::Learning => &mut learning,
                    WordStatus::Known | WordStatus::Ignored => continue,
                };
                if !list.contains(&key) {
                    list.push(key);
                }
            }
            children.push(ui::paragraph(if new.is_empty() {
                ui::key("none")
            } else {
                ui::key_with("new", &[("count", new.len().to_string()), ("words", new.join(", "))])
            }));
            if !learning.is_empty() {
                children.push(ui::paragraph(ui::key_with("learning", &[("words", learning.join(", "))])));
            }
        }
        children.extend(form());
        Ok(ui::show(Some(ui::key("title")), ui::column(children), Value::Null))
    }
}

export_plugin!(NewWords);

#[cfg(test)]
mod tests {
    #[test]
    fn a_line_splits_into_its_words_once() {
        assert_eq!(
            super::split("Keys! He went to get the keys, didn't he? 42"),
            ["Keys", "He", "went", "to", "get", "the", "didn't"]
        );
    }
}
