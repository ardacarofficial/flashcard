//! API v1. Method names are unversioned inside this module (`decks.create`); on the wire they
//! are qualified (`v1.decks.create`).

pub mod attributes;
pub mod common;
pub mod decks;
pub mod events;
pub mod io;
pub mod jobs;
pub mod methods;
pub mod modules;
pub mod notes;
pub mod notetypes;
pub mod oplog;
pub mod settings;
pub mod tags;

pub use methods::catalog;
