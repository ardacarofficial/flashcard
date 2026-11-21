//! API v1. Method names are unversioned inside this module (`decks.create`); on the wire they
//! are qualified (`v1.decks.create`).

pub mod ai;
pub mod attributes;
pub mod automation;
pub mod browse;
pub mod bulk;
pub mod cardtypes;
pub mod common;
pub mod decks;
pub mod events;
pub mod inbox;
pub mod io;
pub mod jobs;
pub mod local_api;
pub mod maintenance;
pub mod mcp;
pub mod methods;
pub mod modes;
pub mod modules;
pub mod notes;
pub mod notetypes;
pub mod onboarding;
pub mod oplog;
pub mod planning;
pub mod plugins;
pub mod presets;
pub mod reading;
pub mod reminders;
pub mod search;
pub mod session;
pub mod settings;
pub mod stats;
pub mod study;
pub mod subtitle;
pub mod tags;
pub mod themes;
pub mod tts;

pub use methods::catalog;
