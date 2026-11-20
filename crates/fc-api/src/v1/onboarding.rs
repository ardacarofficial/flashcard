//! First-run guide and starter decks (v1.0, docs/02 §32): "what do you want to learn?" sets
//! up a deck with the matching classification template, note type and preset, and offers
//! small built-in starter decks (TR/EN, with classification values). Skippable; starter
//! decks are ordinary decks the user can change or delete.

use fc_core::i18n::Locale;
use fc_core::ux::onboarding::{StudyGoal, StudySubject};
use fc_core::{DeckId, NoteTypeId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Response of `onboarding.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OnboardingState {
    /// Show the guide now: never finished or skipped, and the collection holds no notes.
    pub show: bool,
    /// The guide was finished or skipped in this collection.
    pub completed: bool,
}

/// `onboarding.options` / `starter.list`: labels in `locale` (default: the app locale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct LocaleRequest {
    /// Language of names and descriptions.
    #[serde(default)]
    pub locale: Option<Locale>,
}

/// What the guide would set up for one subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OnboardingSubjectDto {
    /// Subject.
    pub subject: StudySubject,
    /// Its goal (screen grouping).
    pub goal: StudyGoal,
    /// Suggested deck name.
    pub deck_name: String,
    /// Classification template defined on the deck (`cefr`, `yks`…).
    pub template_id: Option<String>,
    /// Note type suggested for the deck.
    pub notetype_id: NoteTypeId,
    /// Starter decks offered, in order (ids from `starters`).
    pub starters: Vec<String>,
}

/// A built-in starter deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StarterDeckDto {
    /// Stable id.
    pub id: String,
    /// Deck name.
    pub name: String,
    /// One line about it.
    pub description: String,
    /// Number of notes.
    pub note_count: u32,
    /// Note type of its notes.
    pub notetype_id: NoteTypeId,
    /// Classification template it defines (`null` = none).
    pub template_id: Option<String>,
}

/// Response of `onboarding.options`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OnboardingChoices {
    /// Every subject, in display order.
    pub subjects: Vec<OnboardingSubjectDto>,
    /// Every starter deck.
    pub starters: Vec<StarterDeckDto>,
}

/// Response of `starter.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StarterDeckList {
    /// Every starter deck, in display order.
    pub starters: Vec<StarterDeckDto>,
}

/// `onboarding.apply`: one undo step that creates the deck (with its classification template
/// and preset) and installs the chosen starter decks, then marks the guide finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ApplyOnboarding {
    /// What the user studies.
    pub subject: StudySubject,
    /// Deck name (`null`/blank = the suggestion).
    #[serde(default)]
    pub deck_name: Option<String>,
    /// Starter decks to install (ids from `onboarding.options`).
    #[serde(default)]
    pub starters: Vec<String>,
    /// Language of names and card texts (default: the app locale).
    #[serde(default)]
    pub locale: Option<Locale>,
}

/// Response of `onboarding.apply`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OnboardingResult {
    /// The subject's deck.
    pub deck_id: DeckId,
    /// Installed starter decks, in request order.
    pub starter_deck_ids: Vec<DeckId>,
    /// Notes created by the starter decks.
    pub notes_created: u32,
}

/// `starter.install`: adds one starter deck (one undo step).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct InstallStarter {
    /// Starter deck id.
    pub id: String,
    /// Parent deck (`null` = top level).
    #[serde(default)]
    pub parent_id: Option<DeckId>,
    /// Language of names and card texts (default: the app locale).
    #[serde(default)]
    pub locale: Option<Locale>,
}

/// Response of `starter.install`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StarterInstalled {
    /// The new deck.
    pub deck_id: DeckId,
    /// Notes created.
    pub notes_created: u32,
}
