//! `fc` — Flashcard CLI. Every subcommand maps to API methods and goes through
//! `fc_app::App::call` exactly like the desktop app (actor = `Cli`). Output is a short human
//! summary, or the raw JSON response with `--json`.
#![allow(clippy::print_stdout)]

use clap::{Args, Parser, Subcommand, ValueEnum};
use fc_api::v1::attributes::*;
use fc_api::v1::common::{Empty, EntityRef};
use fc_api::v1::decks::{CreateDeck, DeckDto, DeckTreeNode};
use fc_api::v1::methods as m;
use fc_api::v1::notes::{CreateNote, ListNotes, NoteRef};
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_app::App;
use fc_core::attribute::{AttributeKind, AttributeTarget, AttributeValue};
use fc_core::deck::DeckPath;
use fc_core::i18n::Locale;
use fc_core::notetype::builtin;
use fc_core::paging::PageRequest;
use fc_core::{DeckId, NoteId};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
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
    /// Classification (attribute) operations.
    #[command(subcommand)]
    Attribute(AttributeCommand),
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
    /// Classification value `key=value` (option label for choices; repeatable).
    #[arg(long = "attr", value_name = "KEY=VALUE")]
    attrs: Vec<String>,
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
    /// Set (or with no value, clear) a classification on a note (`attributes.values.set`).
    Set {
        /// Note id.
        note: NoteId,
        /// Attribute key.
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
    fn method(&self) -> &'static str {
        match self {
            Command::Deck(DeckCommand::Create { .. }) => m::DecksCreate::NAME,
            Command::Deck(DeckCommand::List) => m::DecksTree::NAME,
            Command::Note(NoteCommand::Add(_)) => m::NotesCreate::NAME,
            Command::Note(NoteCommand::List { .. }) => m::NotesList::NAME,
            Command::Attribute(AttributeCommand::Define(d)) if d.template.is_some() => {
                m::AttributesDefineFromTemplate::NAME
            }
            Command::Attribute(AttributeCommand::Define(_)) => m::AttributesDefine::NAME,
            Command::Attribute(AttributeCommand::Set { .. }) => m::AttributesValuesSet::NAME,
            Command::Attribute(AttributeCommand::List { deck: Some(_) }) => {
                m::AttributesEffective::NAME
            }
            Command::Attribute(AttributeCommand::List { deck: None }) => m::AttributesList::NAME,
            Command::Undo => m::OplogUndo::NAME,
            Command::Redo => m::OplogRedo::NAME,
        }
    }
}

// ------------------------------------------------------------------ plumbing

fn call<M: ApiMethod>(app: &App, req: &M::Request) -> Result<M::Response, ApiError> {
    app.call::<M>(req, &Actor::Cli)
}

/// Output of one command: the JSON response, or a human summary.
fn out<T: Serialize>(
    json: bool,
    value: &T,
    human: impl FnOnce(&T) -> String,
) -> Result<String, ApiError> {
    if json {
        serde_json::to_string_pretty(value)
            .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))
    } else {
        Ok(human(value))
    }
}

fn find_deck(app: &App, path: &str) -> Result<DeckDto, ApiError> {
    let wanted = DeckPath::parse(path)
        .map_err(fc_app::error::core_to_api)?
        .to_string()
        .to_lowercase();
    call::<m::DecksList>(app, &Empty {})?
        .decks
        .into_iter()
        .find(|d| d.path.to_lowercase() == wanted)
        .ok_or_else(|| {
            ApiError::new(ErrorCode::NotFoundDeck, format!("no deck {path:?}"))
                .with_detail("path", path)
        })
}

fn effective(app: &App, deck: DeckId) -> Result<Vec<EffectiveAttributeDto>, ApiError> {
    Ok(call::<m::AttributesEffective>(
        app,
        &EffectiveAttributes {
            deck_id: deck,
            target: AttributeTarget::Note,
        },
    )?
    .attributes)
}

/// Parses a CLI value for `def` (option labels resolved case-insensitively).
fn parse_value(def: &AttributeDefDto, raw: &str) -> Result<AttributeValue, ApiError> {
    let bad = |why: &str| {
        ApiError::new(
            ErrorCode::ValidationAttributeValue,
            format!("{}: {why}", def.key),
        )
        .with_detail("key", def.key.clone())
    };
    let option = |label: &str| {
        let wanted = label.trim().to_lowercase();
        def.options
            .iter()
            .find(|o| o.label.to_lowercase() == wanted)
            .map(|o| o.id)
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::NotFoundAttributeOption,
                    format!("{} has no option {label:?}", def.key),
                )
                .with_detail("key", def.key.clone())
            })
    };
    Ok(match def.kind {
        AttributeKind::OrderedEnum | AttributeKind::Enum => AttributeValue::Choice(option(raw)?),
        AttributeKind::MultiEnum => AttributeValue::Choices(
            raw.split(',')
                .filter(|s| !s.trim().is_empty())
                .map(option)
                .collect::<Result<_, _>>()?,
        ),
        AttributeKind::Number => {
            AttributeValue::Number(raw.trim().parse().map_err(|_| bad("not a number"))?)
        }
        AttributeKind::Text => AttributeValue::Text(raw.to_owned()),
        AttributeKind::Date => AttributeValue::Date(raw.trim().to_owned()),
        AttributeKind::Bool => AttributeValue::Bool(match raw.trim().to_lowercase().as_str() {
            "true" | "yes" | "evet" | "1" => true,
            "false" | "no" | "hayır" | "0" => false,
            _ => return Err(bad("expected true/false")),
        }),
    })
}

fn find_attr<'a>(
    defs: &'a [EffectiveAttributeDto],
    key: &str,
) -> Result<&'a AttributeDefDto, ApiError> {
    defs.iter()
        .map(|e| &e.def)
        .find(|d| d.key == key)
        .ok_or_else(|| {
            ApiError::new(
                ErrorCode::NotFoundAttribute,
                format!("no classification {key:?} applies here"),
            )
            .with_detail("key", key)
        })
}

fn tree_lines(node: &DeckTreeNode, depth: usize, buf: &mut String) {
    let c = node.counts;
    let _ = writeln!(
        buf,
        "{}{}  [new {} · learn {} · due {} · total {}]",
        "  ".repeat(depth),
        node.deck.name,
        c.new,
        c.learning,
        c.due,
        c.total
    );
    for child in &node.children {
        tree_lines(child, depth + 1, buf);
    }
}

// ------------------------------------------------------------------ commands

/// Runs one parsed command against `app`.
fn run(app: &App, command: &Command, json: bool) -> Result<String, ApiError> {
    match command {
        Command::Deck(DeckCommand::Create { path, description }) => {
            let d = call::<m::DecksCreate>(
                app,
                &CreateDeck {
                    path: path.clone(),
                    description: description.clone(),
                    icon: None,
                    color: None,
                },
            )?;
            out(json, &d, |d| format!("created deck {} ({})", d.path, d.id))
        }
        Command::Deck(DeckCommand::List) => {
            let tree = call::<m::DecksTree>(app, &Empty {})?;
            out(json, &tree, |t| {
                let mut buf = String::new();
                for root in &t.roots {
                    tree_lines(root, 0, &mut buf);
                }
                if buf.is_empty() {
                    buf.push_str("no decks");
                }
                buf.trim_end().to_owned()
            })
        }
        Command::Note(NoteCommand::Add(add)) => {
            let deck = find_deck(app, &add.deck)?;
            let mut attributes = Vec::new();
            if !add.attrs.is_empty() {
                let defs = effective(app, deck.id)?;
                for pair in &add.attrs {
                    let (key, raw) = pair.split_once('=').ok_or_else(|| {
                        ApiError::new(
                            ErrorCode::ValidationInvalidParams,
                            format!("--attr {pair:?}: expected KEY=VALUE"),
                        )
                    })?;
                    let def = find_attr(&defs, key.trim())?;
                    attributes.push(AttributeAssignment {
                        attr_id: def.id,
                        value: Some(parse_value(def, raw)?),
                    });
                }
            }
            let tags = add
                .tags
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            let note = call::<m::NotesCreate>(
                app,
                &CreateNote {
                    notetype_id: builtin::CLASSIC_ID,
                    deck_id: deck.id,
                    fields: BTreeMap::from([
                        (builtin::FRONT.to_owned(), add.front.clone()),
                        (builtin::BACK.to_owned(), add.back.clone()),
                    ]),
                    tags,
                    attributes,
                    allow_duplicate: None,
                },
            )?;
            out(json, &note, |n| {
                format!("added note {} to {} ({})", n.sort_field, deck.path, n.id)
            })
        }
        Command::Note(NoteCommand::List { deck, limit }) => {
            let deck_id = match deck {
                Some(p) => Some(find_deck(app, p)?.id),
                None => None,
            };
            let page = call::<m::NotesList>(
                app,
                &ListNotes {
                    deck_id,
                    include_subdecks: None,
                    page: PageRequest {
                        cursor: None,
                        limit: Some(*limit),
                    },
                },
            )?;
            out(json, &page, |p| {
                if p.items.is_empty() {
                    return "no notes".into();
                }
                p.items
                    .iter()
                    .map(|n| {
                        let tags: Vec<&str> = n.tags.iter().map(|t| t.as_str()).collect();
                        format!("{}  {}  [{}]", n.id, n.sort_field, tags.join(" "))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
        Command::Attribute(AttributeCommand::Define(def)) => {
            let scope_deck_id = match &def.deck {
                Some(p) => Some(find_deck(app, p)?.id),
                None => None,
            };
            let dto = match &def.template {
                Some(template_id) => call::<m::AttributesDefineFromTemplate>(
                    app,
                    &DefineAttributeFromTemplate {
                        template_id: template_id.clone(),
                        scope_deck_id,
                        locale: def.locale,
                    },
                )?,
                None => call::<m::AttributesDefine>(
                    app,
                    &DefineAttribute {
                        scope_deck_id,
                        key: def.key.clone(),
                        name: def.name.clone().unwrap_or_default(),
                        kind: def.kind.map_or(AttributeKind::Text, Into::into),
                        target: None,
                        required: Some(def.required),
                        options: def
                            .options
                            .iter()
                            .map(|label| NewAttributeOption {
                                label: label.clone(),
                                color: None,
                            })
                            .collect(),
                    },
                )?,
            };
            out(json, &dto, |d| {
                let labels: Vec<&str> = d.options.iter().map(|o| o.label.as_str()).collect();
                format!("defined {} ({}) [{}]", d.name, d.key, labels.join(", "))
            })
        }
        Command::Attribute(AttributeCommand::Set { note, key, value }) => {
            let n = call::<m::NotesGet>(app, &NoteRef { id: *note })?;
            let deck = n
                .cards
                .first()
                .map(|c| c.deck_id)
                .ok_or_else(|| ApiError::new(ErrorCode::NotFoundCard, "note has no cards"))?;
            let defs = effective(app, deck)?;
            let def = find_attr(&defs, key)?;
            let value = value.as_deref().map(|v| parse_value(def, v)).transpose()?;
            let res = call::<m::AttributesValuesSet>(
                app,
                &SetAttributeValues {
                    entity: EntityRef::Note(*note),
                    values: vec![AttributeAssignment {
                        attr_id: def.id,
                        value,
                    }],
                },
            )?;
            let label = |v: &AttributeValue| -> String {
                match v {
                    AttributeValue::Choice(id) => def
                        .options
                        .iter()
                        .find(|o| o.id == *id)
                        .map_or_else(|| id.to_string(), |o| o.label.clone()),
                    other => serde_json::to_string(other).unwrap_or_default(),
                }
            };
            out(json, &res, |r| {
                match r
                    .values
                    .iter()
                    .find(|a| a.attr_id == def.id)
                    .and_then(|a| a.value.as_ref())
                {
                    Some(v) => format!("{} = {}", def.key, label(v)),
                    None => format!("{} cleared", def.key),
                }
            })
        }
        Command::Attribute(AttributeCommand::List { deck }) => {
            let defs: Vec<AttributeDefDto> = match deck {
                Some(p) => effective(app, find_deck(app, p)?.id)?
                    .into_iter()
                    .map(|e| e.def)
                    .collect(),
                None => {
                    call::<m::AttributesList>(
                        app,
                        &ListAttributes {
                            scope_deck_id: None,
                            include_deleted: None,
                        },
                    )?
                    .defs
                }
            };
            out(json, &defs, |defs| {
                if defs.is_empty() {
                    return "no classifications".into();
                }
                defs.iter()
                    .map(|d| {
                        let labels: Vec<&str> =
                            d.options.iter().map(|o| o.label.as_str()).collect();
                        format!(
                            "{}  {} ({})  [{}]",
                            d.key,
                            d.name,
                            d.kind.as_str(),
                            labels.join(", ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
        Command::Undo => {
            let r = call::<m::OplogUndo>(app, &Empty {})?;
            out(json, &r, |r| format!("undone: {}", r.summary.key))
        }
        Command::Redo => {
            let r = call::<m::OplogRedo>(app, &Empty {})?;
            out(json, &r, |r| format!("redone: {}", r.summary.key))
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
    match run(&app, &cli.command, cli.json) {
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
mod tests {
    use super::*;
    use clap::CommandFactory;
    use fc_app::testing::TestApp;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_v01_commands() {
        let cases: [(&[&str], &str); 10] = [
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
            (
                &[
                    "fc",
                    "attribute",
                    "define",
                    "--name",
                    "Seviye",
                    "--kind",
                    "ordered-enum",
                ],
                "attributes.define",
            ),
            (
                &[
                    "fc",
                    "attribute",
                    "define",
                    "--template",
                    "cefr",
                    "--locale",
                    "en",
                ],
                "attributes.define_from_template",
            ),
            (
                &[
                    "fc",
                    "attribute",
                    "set",
                    "01900000-0000-7000-8000-000000000001",
                    "level",
                    "B1",
                ],
                "attributes.values.set",
            ),
            (&["fc", "attribute", "list"], "attributes.list"),
            (&["fc", "undo"], "oplog.undo"),
            (&["fc", "redo"], "oplog.redo"),
        ];
        for (argv, method) in cases {
            let cli = Cli::try_parse_from(argv).unwrap();
            assert_eq!(cli.command.method(), method);
        }
        assert!(Cli::try_parse_from(["fc", "note", "add", "--front", "x"]).is_err());
        assert!(Cli::try_parse_from(["fc", "attribute", "define", "--name", "x"]).is_err());
        assert!(Cli::try_parse_from([
            "fc",
            "attribute",
            "define",
            "--template",
            "cefr",
            "--name",
            "x"
        ])
        .is_err());
    }

    fn exec(t: &TestApp, argv: &[&str]) -> Result<String, ApiError> {
        let mut full = vec!["fc"];
        full.extend_from_slice(argv);
        let cli = Cli::try_parse_from(full).unwrap();
        run(&t.app, &cli.command, cli.json)
    }

    #[test]
    fn end_to_end_against_the_app() {
        let t = TestApp::mem();
        assert!(exec(&t, &["deck", "create", "Dil::İngilizce"])
            .unwrap()
            .starts_with("created deck Dil::İngilizce"));
        exec(&t, &["deck", "create", "Dil::İngilizce::Fiiller"]).unwrap();
        exec(
            &t,
            &[
                "attribute",
                "define",
                "--template",
                "cefr",
                "--deck",
                "dil::İngilizce",
            ],
        )
        .unwrap();
        let added = exec(
            &t,
            &[
                "--json",
                "note",
                "add",
                "--deck",
                "Dil::İngilizce::Fiiller",
                "--front",
                "run",
                "--back",
                "koşmak",
                "--tags",
                "fiil düzensiz",
                "--attr",
                "level=b1",
            ],
        )
        .unwrap();
        let note: serde_json::Value = serde_json::from_str(&added).unwrap();
        let id = note["id"].as_str().unwrap().to_owned();
        assert_eq!(note["tags"], serde_json::json!(["fiil", "düzensiz"]));
        assert_eq!(note["attributes"][0]["value"]["type"], "choice");

        let tree = exec(&t, &["deck", "list"]).unwrap();
        assert!(tree.contains("    Fiiller  [new 1"), "{tree}");
        assert!(
            tree.contains("Dil  [new 1 · learn 0 · due 0 · total 1]"),
            "{tree}"
        );
        let list = exec(&t, &["note", "list", "--deck", "Dil"]).unwrap();
        assert!(list.contains("run  [fiil düzensiz]"), "{list}");

        assert_eq!(
            exec(&t, &["attribute", "set", &id, "level", "C1"]).unwrap(),
            "level = C1"
        );
        assert_eq!(
            exec(&t, &["attribute", "set", &id, "level"]).unwrap(),
            "level cleared"
        );
        let attrs = exec(
            &t,
            &["attribute", "list", "--deck", "Dil::İngilizce::Fiiller"],
        )
        .unwrap();
        assert!(attrs.starts_with("level  "), "{attrs}");
        let e = exec(&t, &["attribute", "set", &id, "level", "Z9"]).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotFoundAttributeOption);

        assert_eq!(
            exec(&t, &["undo"]).unwrap(),
            "undone: oplog.summary.attributes.values.set"
        );
        exec(&t, &["redo"]).unwrap();
        exec(&t, &["undo"]).unwrap();
        exec(&t, &["undo"]).unwrap();
        exec(&t, &["undo"]).unwrap();
        assert_eq!(exec(&t, &["note", "list"]).unwrap(), "no notes");
        let e = exec(&t, &["note", "add", "--deck", "Yok", "--front", "x"]).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotFoundDeck);
        exec(
            &t,
            &["attribute", "define", "--name", "Sayfa", "--kind", "number"],
        )
        .unwrap();
        assert!(exec(&t, &["attribute", "list"])
            .unwrap()
            .contains("sayfa  Sayfa (number)"));
    }
}
