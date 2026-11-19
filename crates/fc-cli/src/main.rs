//! `fc` — Flashcard CLI. Every subcommand maps to one API method and goes through
//! `fc_app::App::invoke` exactly like the desktop app (actor = `Cli`).
//!
//! v0.1 contract skeleton: the command layout is final; wiring to `App` is done by L3
//! (subcommands currently report "not implemented").
#![allow(clippy::print_stdout)]

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

/// Flashcard command-line interface.
#[derive(Debug, Parser)]
#[command(name = "fc", version, about = "Flashcard CLI")]
struct Cli {
    /// Collection database file.
    #[arg(long, global = true, env = "FC_DB", default_value = "collection.db")]
    db: PathBuf,
    /// Print raw JSON responses.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Deck operations.
    #[command(subcommand)]
    Deck(DeckCommand),
    /// Note operations.
    #[command(subcommand)]
    Note(NoteCommand),
    /// Undo the last operation (`oplog.undo`).
    Undo,
    /// Redo the last undone operation (`oplog.redo`).
    Redo,
}

#[derive(Debug, Subcommand)]
enum DeckCommand {
    /// Create a deck, e.g. `fc deck create "Dil::İngilizce"` (`decks.create`).
    Create {
        /// Deck path `A::B::C`.
        path: String,
        /// Description.
        #[arg(long)]
        description: Option<String>,
    },
    /// List decks as a tree (`decks.tree`).
    List,
}

#[derive(Debug, Subcommand)]
enum NoteCommand {
    /// Add a Classic (front/back) note (`notes.create`).
    Add(NoteAdd),
    /// List notes (`notes.list`).
    List {
        /// Deck path (default: all decks).
        #[arg(long)]
        deck: Option<String>,
        /// Maximum rows.
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
}

#[derive(Debug, Args)]
struct NoteAdd {
    /// Deck path.
    #[arg(long)]
    deck: String,
    /// Front side.
    #[arg(long)]
    front: String,
    /// Back side.
    #[arg(long, default_value = "")]
    back: String,
    /// Tags, space or comma separated.
    #[arg(long, default_value = "")]
    tags: String,
}

impl Command {
    /// API method this subcommand maps to.
    fn method(&self) -> &'static str {
        use fc_api::v1::methods as m;
        use fc_api::ApiMethod;
        match self {
            Command::Deck(DeckCommand::Create { .. }) => m::DecksCreate::NAME,
            Command::Deck(DeckCommand::List) => m::DecksTree::NAME,
            Command::Note(NoteCommand::Add(_)) => m::NotesCreate::NAME,
            Command::Note(NoteCommand::List { .. }) => m::NotesList::NAME,
            Command::Undo => m::OplogUndo::NAME,
            Command::Redo => m::OplogRedo::NAME,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    eprintln!(
        "fc: `{}` is not implemented yet (db: {}, json: {})",
        cli.command.method(),
        cli.db.display(),
        cli.json
    );
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_v01_commands() {
        let cases: [(&[&str], &str); 6] = [
            (&["fc", "deck", "create", "Dil::İngilizce"], "decks.create"),
            (&["fc", "deck", "list"], "decks.tree"),
            (
                &[
                    "fc", "note", "add", "--deck", "A", "--front", "dog", "--back", "köpek",
                ],
                "notes.create",
            ),
            (
                &["fc", "--db", "x.db", "note", "list", "--deck", "A"],
                "notes.list",
            ),
            (&["fc", "undo"], "oplog.undo"),
            (&["fc", "redo"], "oplog.redo"),
        ];
        for (argv, method) in cases {
            let cli = Cli::try_parse_from(argv).unwrap();
            assert_eq!(cli.command.method(), method);
        }
        assert!(Cli::try_parse_from(["fc", "note", "add", "--front", "x"]).is_err());
    }
}
