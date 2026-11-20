//! API v1. Method names are unversioned inside this module (`decks.create`); on the wire they
//! are qualified (`v1.decks.create`).

pub mod ai;
pub mod attributes;
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
pub mod mcp;
pub mod methods;
pub mod modules;
pub mod notes;
pub mod notetypes;
pub mod oplog;
pub mod planning;
pub mod plugins;
pub mod presets;
pub mod search;
pub mod settings;
pub mod stats;
pub mod study;
pub mod tags;
pub mod tts;

pub use methods::catalog;
