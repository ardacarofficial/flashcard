//! `fc` — Flashcard CLI. Every subcommand maps to public API methods and goes through
//! `fc_app::App` exactly like the desktop app (actor = `Cli`), so internal methods are refused
//! here too. Output is a short human summary, or the raw JSON response with `--json`.
//!
//! * Ergonomic subcommands (`deck`, `note`, `attribute`, `undo`, `redo`) resolve paths, keys,
//!   names and labels to ids for you.
//! * `fc call <method> [<json>|-]` invokes **any** public method with raw JSON params (`-` =
//!   read them from stdin), so everything the app can do is reachable from the shell.
#![allow(clippy::print_stdout)]

mod commands;
mod i18n;
mod lookup;

use clap::{Args, Parser, Subcommand, ValueEnum};
use fc_api::v1::methods as m;
use fc_api::{ApiError, ApiMethod};
use fc_app::App;
use fc_core::attribute::AttributeKind;
use fc_core::i18n::Locale;
use fc_core::NoteId;
use std::path::PathBuf;
use std::process::ExitCode;

/// Flashcard command-line interface.
#[derive(Debug, Parser)]
#[command(name = "fc", version, about = "Flashcard CLI")]
pub struct Cli {
    /// Collection database file.
    #[arg(long, global = true, env = "FC_DB", default_value = "collection.db")]
    db: PathBuf,
    /// Print raw JSON responses.
    #[arg(long, global = true)]
    json: bool,
    /// Output language (default: the `app.locale` setting).
    #[arg(long, global = true, value_parser = parse_locale)]
    lang: Option<Locale>,
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
    /// Classification (attribute) operations.
    #[command(subcommand)]
    Attribute(AttributeCommand),
    /// Undo the last operation (`oplog.undo`).
    Undo,
    /// Redo the last undone operation (`oplog.redo`).
    Redo,
    /// Invoke any public API method with JSON params, e.g.
    /// `fc call decks.rename '{"id":"…","name":"X"}'`. Prints the JSON response.
    Call {
        /// Method name (`decks.create` or qualified `v1.decks.create`).
        method: String,
        /// JSON params (default `{}`); `-` reads them from stdin.
        params: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum DeckCommand {
    /// Create a deck, e.g. `fc deck create "Dil::İngilizce"` (`decks.create`).
    Create {
        /// Deck path `A::B::C` (relative to `--parent` if given).
        path: String,
        /// Parent deck (path or id).
        #[arg(long)]
        parent: Option<String>,
        /// Description.
        #[arg(long)]
        description: Option<String>,
    },
    /// List decks as a tree (`decks.tree`).
    List,
    /// Rename a deck (leaf name) (`decks.rename`).
    Rename {
        /// Deck (path or id).
        deck: String,
        /// New name.
        name: String,
    },
    /// Move a deck under another one, to the top level, or to a position (`decks.move`).
    Move {
        /// Deck (path or id).
        deck: String,
        /// New parent (path or id).
        #[arg(long, conflicts_with = "top")]
        parent: Option<String>,
        /// Move to the top level.
        #[arg(long)]
        top: bool,
        /// Position among the new siblings (default: last).
        #[arg(long)]
        position: Option<i64>,
    },
    /// Move a deck, its sub-decks, notes and cards to the trash (`decks.delete`).
    Delete {
        /// Deck (path or id).
        deck: String,
    },
    /// Restore a deck from the trash (`decks.restore`). Trashed decks have no path any more:
    /// use the id printed by `fc deck delete`.
    Restore {
        /// Deck id.
        id: fc_core::DeckId,
    },
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
    /// Edit a note: only the given parts change (`notes.update`).
    Edit(NoteEdit),
    /// Move notes (and their cards) to the trash (`notes.delete`).
    Delete {
        /// Note ids.
        #[arg(required = true)]
        ids: Vec<NoteId>,
    },
    /// Restore notes from the trash (`notes.restore`).
    Restore {
        /// Note ids.
        #[arg(required = true)]
        ids: Vec<NoteId>,
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
    /// Classification value `KEY=VALUE`; KEY is the key or the name (case-insensitive),
    /// VALUE an option label for choices (repeatable).
    #[arg(long = "attr", value_name = "KEY=VALUE")]
    attrs: Vec<String>,
}

#[derive(Debug, Args)]
struct NoteEdit {
    /// Note id.
    id: NoteId,
    /// New front side.
    #[arg(long)]
    front: Option<String>,
    /// New back side.
    #[arg(long)]
    back: Option<String>,
    /// Replace all tags (space or comma separated; `""` clears).
    #[arg(long)]
    tags: Option<String>,
    /// Set a classification `KEY=VALUE` (empty VALUE clears; repeatable).
    #[arg(long = "attr", value_name = "KEY=VALUE")]
    attrs: Vec<String>,
    /// New home deck (path or id).
    #[arg(long)]
    deck: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum KindArg {
    OrderedEnum,
    Enum,
    MultiEnum,
    Number,
    Text,
    Date,
    Bool,
}

impl From<KindArg> for AttributeKind {
    fn from(k: KindArg) -> Self {
        match k {
            KindArg::OrderedEnum => Self::OrderedEnum,
            KindArg::Enum => Self::Enum,
            KindArg::MultiEnum => Self::MultiEnum,
            KindArg::Number => Self::Number,
            KindArg::Text => Self::Text,
            KindArg::Date => Self::Date,
            KindArg::Bool => Self::Bool,
        }
    }
}

#[derive(Debug, Subcommand)]
enum AttributeCommand {
    /// Define a classification (`attributes.define`), or copy a built-in template with
    /// `--template cefr|yks|…` (`attributes.define_from_template`).
    Define(AttributeDefine),
    /// Apply a built-in template, e.g. `fc attribute apply-template cefr --deck Dil`
    /// (`attributes.define_from_template`).
    ApplyTemplate {
        /// Template id (`cefr`, `yks`, …).
        template: String,
        /// Deck the classification is defined on (default: whole collection).
        #[arg(long)]
        deck: Option<String>,
        /// Language of the template's names/labels (default: app language).
        #[arg(long, value_parser = parse_locale)]
        locale: Option<Locale>,
    },
    /// Set (or with no value, clear) a classification on a note (`attributes.values.set`).
    Set {
        /// Note id.
        note: NoteId,
        /// Classification key or name.
        key: String,
        /// Value (option label for choices, comma-separated for multi-choice).
        value: Option<String>,
    },
    /// List classifications: all, or those applying to `--deck` (`attributes.effective`).
    List {
        /// Deck path.
        #[arg(long)]
        deck: Option<String>,
    },
    /// Option operations of a choice classification.
    #[command(subcommand)]
    Option(OptionCommand),
}

#[derive(Debug, Args)]
struct AttrRef {
    /// Classification key or name (case-insensitive).
    attr: String,
    /// Look the classification up among those applying to this deck.
    #[arg(long)]
    deck: Option<String>,
}

#[derive(Debug, Subcommand)]
enum OptionCommand {
    /// Add an option (`attributes.option.add`).
    Add {
        #[command(flatten)]
        attr: AttrRef,
        /// Label.
        label: String,
        /// Insert after this option (default: last).
        #[arg(long)]
        after: Option<String>,
    },
    /// Rename an option; values keep pointing at it (`attributes.option.rename`).
    Rename {
        #[command(flatten)]
        attr: AttrRef,
        /// Current label.
        label: String,
        /// New label.
        new_label: String,
    },
    /// Reorder options: list every label in the new order (`attributes.option.reorder`).
    Reorder {
        #[command(flatten)]
        attr: AttrRef,
        /// All labels, in order.
        #[arg(required = true)]
        labels: Vec<String>,
    },
    /// Delete an option; values move to `--replace-with` or are removed
    /// (`attributes.option.delete`).
    Delete {
        #[command(flatten)]
        attr: AttrRef,
        /// Label.
        label: String,
        /// Replacement option label.
        #[arg(long)]
        replace_with: Option<String>,
    },
}

#[derive(Debug, Args)]
struct AttributeDefine {
    /// Display name (required unless `--template`).
    #[arg(long, required_unless_present = "template")]
    name: Option<String>,
    /// Value type.
    #[arg(long, value_enum, required_unless_present = "template")]
    kind: Option<KindArg>,
    /// Option label (repeatable, in order).
    #[arg(long = "option")]
    options: Vec<String>,
    /// Query key (default: derived from the name).
    #[arg(long)]
    key: Option<String>,
    /// Deck path the classification is defined on (default: whole collection).
    #[arg(long)]
    deck: Option<String>,
    /// Required on notes.
    #[arg(long)]
    required: bool,
    /// Built-in template id.
    #[arg(long, conflicts_with_all = ["name", "kind", "options", "key", "required"])]
    template: Option<String>,
    /// Language for template names/labels.
    #[arg(long, requires = "template", value_parser = parse_locale)]
    locale: Option<Locale>,
}

fn parse_locale(s: &str) -> Result<Locale, String> {
    match s {
        "tr" => Ok(Locale::Tr),
        "en" => Ok(Locale::En),
        _ => Err("expected tr or en".into()),
    }
}

impl Command {
    /// Main API method this subcommand maps to.
    fn method(&self) -> &str {
        match self {
            Command::Deck(c) => match c {
                DeckCommand::Create { .. } => m::DecksCreate::NAME,
                DeckCommand::List => m::DecksTree::NAME,
                DeckCommand::Rename { .. } => m::DecksRename::NAME,
                DeckCommand::Move { .. } => m::DecksMove::NAME,
                DeckCommand::Delete { .. } => m::DecksDelete::NAME,
                DeckCommand::Restore { .. } => m::DecksRestore::NAME,
            },
            Command::Note(c) => match c {
                NoteCommand::Add(_) => m::NotesCreate::NAME,
                NoteCommand::List { .. } => m::NotesList::NAME,
                NoteCommand::Edit(_) => m::NotesUpdate::NAME,
                NoteCommand::Delete { .. } => m::NotesDelete::NAME,
                NoteCommand::Restore { .. } => m::NotesRestore::NAME,
            },
            Command::Attribute(c) => match c {
                AttributeCommand::Define(d) if d.template.is_some() => {
                    m::AttributesDefineFromTemplate::NAME
                }
                AttributeCommand::Define(_) => m::AttributesDefine::NAME,
                AttributeCommand::ApplyTemplate { .. } => m::AttributesDefineFromTemplate::NAME,
                AttributeCommand::Set { .. } => m::AttributesValuesSet::NAME,
                AttributeCommand::List { deck: Some(_) } => m::AttributesEffective::NAME,
                AttributeCommand::List { deck: None } => m::AttributesList::NAME,
                AttributeCommand::Option(o) => match o {
                    OptionCommand::Add { .. } => m::AttributesOptionAdd::NAME,
                    OptionCommand::Rename { .. } => m::AttributesOptionRename::NAME,
                    OptionCommand::Reorder { .. } => m::AttributesOptionReorder::NAME,
                    OptionCommand::Delete { .. } => m::AttributesOptionDelete::NAME,
                },
            },
            Command::Undo => m::OplogUndo::NAME,
            Command::Redo => m::OplogRedo::NAME,
            Command::Call { method, .. } => method,
        }
    }
}

fn report(method: &str, e: &ApiError, json: bool) {
    if json {
        let body = serde_json::to_string_pretty(e).unwrap_or_else(|_| e.message.clone());
        eprintln!("{body}");
    } else {
        eprintln!("fc: {method}: {}: {}", e.code, e.message);
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let app = match App::open_sqlite(&cli.db) {
        Ok(app) => app,
        Err(e) => {
            report(cli.command.method(), &e.into_api(), cli.json);
            return ExitCode::from(2);
        }
    };
    // AI provider keys: the same OS keychain entries as the desktop app (v0.8).
    app.set_secret_store(std::sync::Arc::new(fc_ai::secrets::OsKeychain::new(
        fc_ai::secrets::SERVICE,
    )));
    let ctx = commands::Ctx {
        app: &app,
        json: cli.json,
        text: i18n::Catalog::new(cli.lang.unwrap_or_else(|| app.locale())),
    };
    match commands::run(&ctx, &cli.command, &mut std::io::stdin().lock()) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            report(cli.command.method(), &e, cli.json);
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
