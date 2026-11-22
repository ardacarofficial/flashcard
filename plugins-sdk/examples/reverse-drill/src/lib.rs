//! Example: a study mode with its own flow through the declarative UI. It shows a card's
//! back, the user types the front; the result is recorded with `study.answer` (Good /
//! Again), so the real scheduler and review log are used. Session state travels in the UI
//! `state`.

use fc_plugin_sdk::api::v1::methods::{NotesGet, StudyAnswer, StudyStart};
use fc_plugin_sdk::api::v1::notes::NoteRef;
use fc_plugin_sdk::api::v1::study::{AnswerCard, SessionDto, StartSession, StudyFilter, StudyKind};
use fc_plugin_sdk::core::card::Rating;
use fc_plugin_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Reverse;

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    session_id: Option<String>,
    card_id: Option<String>,
    expected: String,
    correct: u32,
    wrong: u32,
    total: u32,
}

fn parse<T: serde::de::DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_value(json!(s)).map_err(|e| Error::new("STATE", e.to_string()))
}

/// Normalized comparison (case, surrounding space).
pub fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in s.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            c if !tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_owned()
}

/// Shows the current card of `session` (or the summary).
fn step(session: SessionDto, mut st: State, feedback: Option<UiNode>) -> Result<UiReply> {
    let mut children: Vec<UiNode> = feedback.into_iter().collect();
    let Some(card) = session.current else {
        children.push(ui::paragraph(ui::key_with(
            "finished",
            &[
                ("correct", st.correct.to_string()),
                ("wrong", st.wrong.to_string()),
            ],
        )));
        children.push(ui::button("close", ui::key("close"), true));
        st.card_id = None;
        return Ok(ui::show(
            Some(ui::key("title")),
            ui::column(children),
            serde_json::to_value(&st).unwrap_or_default(),
        ));
    };
    let note = host::call::<NotesGet>(&NoteRef { id: card.note_id })?;
    let field = |k: &str| {
        note.fields
            .get(k)
            .map(|v| strip_html(v))
            .unwrap_or_default()
    };
    st.expected = field("front");
    st.card_id = Some(card.card_id.to_string());
    st.session_id = Some(session.id.to_string());
    children.push(ui::progress(
        st.correct + st.wrong,
        st.total.max(st.correct + st.wrong + 1),
    ));
    children.push(ui::paragraph(ui::key("prompt")));
    children.push(ui::heading(ui::text(field("back"))));
    children.push(ui::input("answer", Some(ui::key("answer"))));
    children.push(ui::button("check", ui::key("check"), true));
    Ok(ui::show(
        Some(ui::key("title")),
        ui::column(children),
        serde_json::to_value(&st).unwrap_or_default(),
    ))
}

impl Plugin for Reverse {
    fn ui_open(&self, _view: &str, _session: &str, input: Value) -> Result<UiReply> {
        let deck_id = serde_json::from_value(input["deck_id"].clone()).ok();
        let session = host::call::<StudyStart>(&StartSession {
            kind: StudyKind::Custom,
            mode: None,
            deck_id,
            include_subdecks: None,
            filter: Some(StudyFilter {
                limit: Some(20),
                ..Default::default()
            }),
            from_session: None,
            new_limit: None,
            order: None,
            tz_offset_mins: None,
            modes: None,
            scope: None,
        })?;
        let total = session.counts.new + session.counts.learning + session.counts.review;
        if session.current.is_none() {
            return Ok(ui::show(
                Some(ui::key("title")),
                ui::column(vec![
                    ui::paragraph(ui::key("empty")),
                    ui::button("close", ui::key("close"), true),
                ]),
                Value::Null,
            ));
        }
        step(
            session,
            State {
                total,
                ..State::default()
            },
            None,
        )
    }

    fn ui_act(
        &self,
        _view: &str,
        _session: &str,
        action: &str,
        values: BTreeMap<String, Value>,
        state: Value,
    ) -> Result<UiReply> {
        let mut st: State = serde_json::from_value(state).unwrap_or_default();
        if action != "check" {
            return Ok(ui::done());
        }
        let (Some(session_id), Some(card_id)) = (st.session_id.clone(), st.card_id.clone()) else {
            return Ok(ui::done());
        };
        let given = values
            .get("answer")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right = same(given, &st.expected);
        if right {
            st.correct += 1
        } else {
            st.wrong += 1
        }
        let session = host::call::<StudyAnswer>(&AnswerCard {
            session_id: parse(&session_id)?,
            card_id: parse(&card_id)?,
            rating: if right { Rating::Good } else { Rating::Again },
            duration_ms: 0,
            reveal_ms: None,
        })?;
        let feedback = if right {
            ui::feedback(true, ui::key("right"))
        } else {
            ui::feedback(
                false,
                ui::key_with("wrong", &[("expected", st.expected.clone())]),
            )
        };
        step(session, st, Some(feedback))
    }
}

export_plugin!(Reverse);

#[cfg(test)]
mod tests {
    #[test]
    fn comparison_ignores_case_and_space() {
        assert!(super::same(" Kılıç ", "kılıç"));
        assert!(!super::same("kalkan", "kılıç"));
    }
}
