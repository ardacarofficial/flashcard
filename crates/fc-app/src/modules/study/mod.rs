//! `study` module (v0.2): the study core — session engine, FSRS/SM-2 scheduling, typed
//! answers, hints, trial mode, mistakes sessions, suspend/bury/postpone, card history
//! (docs/02 §5–7, docs/03 §9).
//!
//! Building blocks (CLAUDE.md rule 3): the pure parts live in `fc-study` (schedulers, queue,
//! checkers, hints); this module adds persistence and the command contract. Every card
//! change is a command whose inverse is one internal call, `study.restore`, which writes
//! back exactly the study-owned columns, the session state and the review log rows it
//! touched — so undo of an answer returns the card **and** the session to where they were.
//!
//! * `engine`: settings, scheduler parameters, session load/save, rendering, the session DTO.
//! * `session`: `study.start/session/active/answer/hint/check_answer/end/summary`.
//! * `cards`: `study.suspend/bury/postpone/restore/card_history`.
//! * `state_diff`: how a session's queue is stored and undone (v1.0): row edits and diffs
//!   instead of whole-queue copies.

mod cards;
mod engine;
mod session;
mod state_diff;
#[cfg(test)]
mod tests;

/// Extension points of the session engine for the v0.10 study modes (`modules::modes`):
/// sessions, rendering, the shared answer path and its hooks. Modes add session kinds and
/// options through these instead of forking the engine (docs/10 §24).
pub(crate) mod ext {
    pub(crate) use super::engine::{
        not_available, render_for_study, require_live_card, scheduler,
        scheduler_error, session_dto, setting_bool, setting_str, Session,
        SessionConfig, View,
    };
    pub(crate) use super::session::{answer_core, AnswerHooks};
    pub(crate) use super::{ANSWER_CHECKER, FOLD_TURKISH};
}

use crate::module::{bool_setting, choice_setting, main_menu, ModuleDescriptor};
use crate::registry::Registry;
use fc_api::v1::methods::*;
use fc_core::settings::{SettingDef, SettingKind, SettingScopeKind};

/// Module id.
pub const ID: &str = "study";

/// Scheduler (`fsrs`, `sm2`).
pub const SCHEDULER: &str = "study.scheduler";
/// New cards per normal session.
pub const NEW_PER_SESSION: &str = "study.new_per_session";
/// Local hour at which a study day starts.
pub const DAY_START_HOUR: &str = "study.day_start_hour";
/// Show an input to type the answer.
pub const TYPE_ANSWER: &str = "study.type_answer";
/// Answer checker (`exact`, `normalized`, `tolerant`, `multi`).
pub const ANSWER_CHECKER: &str = "study.answer_checker";
/// Treat Turkish letter variants as equal when checking.
pub const FOLD_TURKISH: &str = "study.fold_turkish";
/// Let the checker's suggestion rate the card.
pub const AUTO_GRADE: &str = "study.auto_grade";
/// Hint strategy (`first_letters`, `word_starts`, `partial`).
pub const HINT_STRATEGY: &str = "study.hint_strategy";
/// v1.1 display preference: answer buttons of the study screen — `two` ("Still learning" =
/// again, "I know it" = good) or `four` (again / hard / good / easy with next intervals).
/// Presentation only: the ratings and the scheduler are the same either way.
pub const ANSWER_BUTTONS: &str = "study.answer_buttons";

fn int_setting(key: &str, default: i64, min: i64, max: i64, advanced: bool) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: SettingKind::Integer {
            min: Some(min),
            max: Some(max),
        },
        default: serde_json::Value::from(default),
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced,
        scopes: vec![SettingScopeKind::App],
    }
}

fn advanced(mut def: SettingDef) -> SettingDef {
    def.advanced = true;
    def
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    let deck_scopes = [
        SettingScopeKind::App,
        SettingScopeKind::Preset,
        SettingScopeKind::Deck,
    ];
    ModuleDescriptor {
        id: ID,
        name_key: "modules.study.name",
        description_key: "modules.study.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            int_setting(NEW_PER_SESSION, 20, 0, 9_999, false),
            bool_setting(TYPE_ANSWER, false, &deck_scopes),
            choice_setting(HINT_STRATEGY, &["first_letters", "word_starts", "partial"], "first_letters"),
            choice_setting(ANSWER_BUTTONS, &["two", "four"], "two"),
            advanced(choice_setting(SCHEDULER, &fc_study::scheduler::BUILTIN_IDS, "fsrs")),
            advanced(choice_setting(
                ANSWER_CHECKER,
                &["exact", "normalized", "tolerant", "multi"],
                "multi",
            )),
            advanced(bool_setting(FOLD_TURKISH, true, &[SettingScopeKind::App])),
            advanced(bool_setting(AUTO_GRADE, false, &[SettingScopeKind::App])),
            int_setting(DAY_START_HOUR, 4, 0, 23, true),
        ],
        ui: vec![
            main_menu("study.session", "nav.study", "study", "/study", 5),
            // v1.1: Settings → "Reading and modes" (display options of the study modes,
            // learn/test/timed/focus/reading settings; the page shows the enabled ones).
            main_menu(
                "study.mode_settings",
                "nav.mode_settings",
                "sliders",
                "/mode-settings",
                90,
            ),
        ],
    }
}

/// Drops the in-memory session queue cache (v1.0: profile switch). The cache is keyed by
/// session id and a per-process revision, so it could not serve another collection's
/// queue anyway; clearing it keeps nothing of the old profile in memory.
pub(crate) fn invalidate_queue_cache(reg: &Registry) {
    if let Some(c) = reg.extension::<state_diff::QueueCache>() {
        c.clear();
    }
}

/// Whether the session queue cache holds a queue (tests).
#[cfg(test)]
pub(crate) fn queue_cache_is_empty(reg: &Registry) -> bool {
    reg.extension::<state_diff::QueueCache>()
        .is_none_or(|c| c.is_empty())
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.provide(std::sync::Arc::new(state_diff::QueueCache::default()));
    reg.command::<StudyStart>(session::start);
    reg.query::<StudySession>(session::get);
    reg.query::<StudyActive>(session::active);
    reg.command::<StudyAnswer>(session::answer);
    reg.command::<StudyHint>(session::hint);
    reg.query::<StudyCheckAnswer>(session::check_answer);
    reg.command::<StudyEnd>(session::end);
    reg.query::<StudySummary>(session::summary_query);
    reg.command::<StudySuspend>(cards::suspend);
    reg.command::<StudyBury>(cards::bury);
    reg.command::<StudyPostpone>(cards::postpone);
    reg.query::<StudyCardHistory>(cards::history);
    reg.command::<StudyRestore>(cards::restore);
}
