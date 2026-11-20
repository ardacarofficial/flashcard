//! Execution of parsed subcommands. Every write goes through a public API method; nothing
//! here touches storage or business rules.

use crate::i18n::Catalog;
use crate::lookup::{self, call};
use crate::{
    AttrRef, AttributeCommand, Command, DeckCommand, NoteCommand, OptionCommand, PluginCommand,
};
use fc_api::v1::attributes::*;
use fc_api::v1::common::{AffectedCount, Empty, EntityRef};
use fc_api::v1::decks::{CreateDeck, DeckRef, DeckTreeNode, MoveDeck, RenameDeck};
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods as m;
use fc_api::v1::notes::{CreateNote, ListNotes, NoteIds, NoteRef, UpdateNote};
use fc_api::v1::oplog::{Actor, UndoResult};
use fc_api::v1::plugins::{
    InspectPackage, InstallPlugin, PermissionDto, PluginDto, PluginRef, RunPluginCommand,
    SetPluginEnabled, SetPluginPermissions, UninstallPlugin,
};
use fc_api::{ApiError, ErrorCode, API_VERSION};
use fc_app::App;
use fc_core::attribute::AttributeKind;
use fc_core::notetype::builtin;
use fc_core::paging::PageRequest;
use fc_core::DeckId;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read;

/// What a command runs against.
pub struct Ctx<'a> {
    /// The opened collection.
    pub app: &'a App,
    /// Print raw JSON instead of a human summary.
    pub json: bool,
    /// Message catalog for human output.
    pub text: Catalog,
}

impl Ctx<'_> {
    /// The JSON response, or a human summary.
    fn out<T: Serialize>(
        &self,
        value: &T,
        human: impl FnOnce(&T) -> String,
    ) -> Result<String, ApiError> {
        if self.json {
            pretty(value)
        } else {
            Ok(human(value))
        }
    }
}

fn pretty<T: Serialize>(value: &T) -> Result<String, ApiError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::ValidationInvalidParams, message)
}

fn split_tags(raw: &str) -> Vec<String> {
    raw.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn affected(text: &Catalog, key: &str, what: &str, a: &AffectedCount) -> String {
    text.text(
        key,
        &params(&[
            ("what", what.to_owned()),
            ("decks", a.decks.to_string()),
            ("notes", a.notes.to_string()),
            ("cards", a.cards.to_string()),
        ]),
    )
}

fn params(pairs: &[(&str, String)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn tree_lines(text: &Catalog, node: &DeckTreeNode, depth: usize, buf: &mut String) {
    let c = node.counts;
    let counts = text.text(
        "cli.deck.counts",
        &params(&[
            ("new", c.new.to_string()),
            ("learning", c.learning.to_string()),
            ("due", c.due.to_string()),
            ("total", c.total.to_string()),
        ]),
    );
    let _ = writeln!(buf, "{}{}  {counts}", "  ".repeat(depth), node.deck.name);
    for child in &node.children {
        tree_lines(text, child, depth + 1, buf);
    }
}

fn def_line(d: &AttributeDefDto) -> String {
    let labels: Vec<&str> = d.options.iter().map(|o| o.label.as_str()).collect();
    format!(
        "{}  {} ({})  [{}]",
        d.key,
        d.name,
        d.kind.as_str(),
        labels.join(", ")
    )
}

/// `--attr KEY=VALUE` pairs → assignments, resolved against the definitions applying to
/// `deck`. An empty VALUE clears (when `allow_clear`).
fn assignments(
    app: &App,
    deck: DeckId,
    pairs: &[String],
    allow_clear: bool,
) -> Result<Vec<AttributeAssignment>, ApiError> {
    if pairs.is_empty() {
        return Ok(Vec::new());
    }
    let defs = lookup::effective(app, deck)?;
    pairs
        .iter()
        .map(|p| {
            let (key, raw) = lookup::pair(p)?;
            let def = lookup::match_attr(defs.iter().map(|e| &e.def), key)?;
            let value = if allow_clear && raw.trim().is_empty() {
                None
            } else {
                Some(lookup::value(def, raw)?)
            };
            Ok(AttributeAssignment {
                attr_id: def.id,
                value,
            })
        })
        .collect()
}

/// Runs one parsed command. `stdin` feeds `fc call <method> -`.
pub fn run(ctx: &Ctx<'_>, command: &Command, stdin: &mut dyn Read) -> Result<String, ApiError> {
    match command {
        Command::Deck(c) => deck(ctx, c),
        Command::Note(c) => note(ctx, c),
        Command::Attribute(c) => attribute(ctx, c),
        Command::Plugin(c) => plugin(ctx, c),
        Command::Undo => {
            let r = call::<m::OplogUndo>(ctx.app, &Empty {})?;
            ctx.out(&r, |r| undo_line(&ctx.text, "cli.undone", r))
        }
        Command::Redo => {
            let r = call::<m::OplogRedo>(ctx.app, &Empty {})?;
            ctx.out(&r, |r| undo_line(&ctx.text, "cli.redone", r))
        }
        Command::Mcp(crate::mcp::McpCommand::Tools { server, command }) => {
            crate::mcp::tools(ctx, server.as_deref(), command)
        }
        Command::Mcp(crate::mcp::McpCommand::Serve { .. }) => {
            Err(invalid("fc mcp serve runs before the collection is opened"))
        }
        Command::Call { method, params } => {
            let raw = match params.as_deref() {
                Some("-") => {
                    let mut s = String::new();
                    stdin
                        .read_to_string(&mut s)
                        .map_err(|e| ApiError::new(ErrorCode::IoFile, e.to_string()))?;
                    s
                }
                Some(s) => s.to_owned(),
                None => String::new(),
            };
            let params: Value = if raw.trim().is_empty() {
                Value::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(&raw).map_err(|e| invalid(format!("params: {e}")))?
            };
            let prefix = format!("{API_VERSION}.");
            let qualified = if method.starts_with(&prefix) {
                method.clone()
            } else {
                format!("{prefix}{method}")
            };
            let response = ctx.app.invoke(&qualified, params, &Actor::Cli)?;
            // No background worker here: a job the call queued runs now, and the finished
            // job (with its result) is printed instead of the queued one.
            if let Some(job) = queued_job(&response) {
                ctx.app.run_pending_jobs().map_err(ApiError::from)?;
                let done = call::<m::JobsGet>(ctx.app, &JobRef { id: job.id })?;
                return pretty(
                    &serde_json::to_value(done)
                        .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?,
                );
            }
            pretty(&response)
        }
    }
}

/// The job a response started: a bare `JobDto`, or one wrapped as `{"job": …}` (the shape of
/// `optimizer.run`'s `OptimizerStarted`). Only a still-queued job counts.
fn queued_job(response: &Value) -> Option<JobDto> {
    let inner = match response.get("job") {
        Some(j) if response.as_object().is_some_and(|o| o.len() == 1) => j,
        _ => response,
    };
    serde_json::from_value::<JobDto>(inner.clone())
        .ok()
        .filter(|j| j.status == JobStatus::Queued)
}

fn undo_line(text: &Catalog, key: &str, r: &UndoResult) -> String {
    text.with(key, "summary", text.message(&r.summary))
}

fn deck(ctx: &Ctx<'_>, c: &DeckCommand) -> Result<String, ApiError> {
    let app = ctx.app;
    match c {
        DeckCommand::Create {
            path,
            parent,
            description,
        } => {
            let d = call::<m::DecksCreate>(
                app,
                &CreateDeck {
                    path: path.clone(),
                    parent_id: lookup::deck_id(app, parent.as_deref())?,
                    description: description.clone(),
                    icon: None,
                    color: None,
                },
            )?;
            ctx.out(&d, |d| {
                ctx.text.text(
                    "cli.deck.created",
                    &params(&[("path", d.path.clone()), ("id", d.id.to_string())]),
                )
            })
        }
        DeckCommand::List => {
            let tree = call::<m::DecksTree>(app, &Empty {})?;
            ctx.out(&tree, |t| {
                let mut buf = String::new();
                for root in &t.roots {
                    tree_lines(&ctx.text, root, 0, &mut buf);
                }
                if buf.is_empty() {
                    buf.push_str(&ctx.text.text("cli.deck.none", &BTreeMap::new()));
                }
                buf.trim_end().to_owned()
            })
        }
        DeckCommand::Rename { deck, name } => {
            let id = lookup::deck(app, deck)?.id;
            let d = call::<m::DecksRename>(
                app,
                &RenameDeck {
                    id,
                    name: name.clone(),
                },
            )?;
            ctx.out(&d, |d| {
                ctx.text.with("cli.deck.renamed", "path", d.path.clone())
            })
        }
        DeckCommand::Move {
            deck,
            parent,
            top,
            position,
        } => {
            let current = lookup::deck(app, deck)?;
            let parent_id = match (parent, top) {
                (Some(p), _) => Some(lookup::deck(app, p)?.id),
                (None, true) => None,
                (None, false) => current.parent_id,
            };
            let d = call::<m::DecksMove>(
                app,
                &MoveDeck {
                    id: current.id,
                    parent_id,
                    position: *position,
                },
            )?;
            ctx.out(&d, |d| {
                ctx.text.with("cli.deck.moved", "path", d.path.clone())
            })
        }
        DeckCommand::Delete { deck } => {
            let d = lookup::deck(app, deck)?;
            let a = call::<m::DecksDelete>(app, &DeckRef { id: d.id })?;
            ctx.out(&a, |a| {
                ctx.text.text(
                    "cli.deck.deleted",
                    &params(&[
                        (
                            "summary",
                            affected(&ctx.text, "cli.affected.deleted", &d.path, a),
                        ),
                        ("id", d.id.to_string()),
                    ]),
                )
            })
        }
        DeckCommand::Restore { id } => {
            let a = call::<m::DecksRestore>(app, &DeckRef { id: *id })?;
            let path = call::<m::DecksGet>(app, &DeckRef { id: *id })?.path;
            ctx.out(&a, |a| {
                affected(&ctx.text, "cli.affected.restored", &path, a)
            })
        }
    }
}

fn note(ctx: &Ctx<'_>, c: &NoteCommand) -> Result<String, ApiError> {
    let app = ctx.app;
    match c {
        NoteCommand::Add(add) => {
            let deck = lookup::deck(app, &add.deck)?;
            let note = call::<m::NotesCreate>(
                app,
                &CreateNote {
                    notetype_id: builtin::CLASSIC_ID,
                    deck_id: deck.id,
                    fields: BTreeMap::from([
                        (builtin::FRONT.to_owned(), add.front.clone()),
                        (builtin::BACK.to_owned(), add.back.clone()),
                    ]),
                    tags: split_tags(&add.tags),
                    attributes: assignments(app, deck.id, &add.attrs, false)?,
                    allow_duplicate: None,
                },
            )?;
            ctx.out(&note, |n| {
                ctx.text.text(
                    "cli.note.added",
                    &params(&[
                        ("front", n.sort_field.clone()),
                        ("deck", deck.path.clone()),
                        ("id", n.id.to_string()),
                    ]),
                )
            })
        }
        NoteCommand::List { deck, limit } => {
            let page = call::<m::NotesList>(
                app,
                &ListNotes {
                    deck_id: lookup::deck_id(app, deck.as_deref())?,
                    include_subdecks: None,
                    page: PageRequest {
                        cursor: None,
                        limit: Some(*limit),
                    },
                },
            )?;
            ctx.out(&page, |p| {
                if p.items.is_empty() {
                    return ctx.text.text("cli.note.none", &BTreeMap::new());
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
        NoteCommand::Edit(e) => {
            let current = call::<m::NotesGet>(app, &NoteRef { id: e.id })?;
            let deck_id = lookup::deck_id(app, e.deck.as_deref())?;
            let mut fields = current.fields.clone();
            if let Some(front) = &e.front {
                fields.insert(builtin::FRONT.to_owned(), front.clone());
            }
            if let Some(back) = &e.back {
                fields.insert(builtin::BACK.to_owned(), back.clone());
            }
            let tags = match &e.tags {
                Some(raw) => split_tags(raw),
                None => current.tags.iter().map(|t| t.as_str().to_owned()).collect(),
            };
            let attributes =
                assignments(app, deck_id.unwrap_or(current.home_deck_id), &e.attrs, true)?;
            let note = call::<m::NotesUpdate>(
                app,
                &UpdateNote {
                    id: e.id,
                    deck_id,
                    fields,
                    tags,
                    attributes,
                },
            )?;
            ctx.out(&note, |n| {
                ctx.text.text(
                    "cli.note.edited",
                    &params(&[("front", n.sort_field.clone()), ("id", n.id.to_string())]),
                )
            })
        }
        NoteCommand::Delete { ids } => {
            let a = call::<m::NotesDelete>(app, &NoteIds { ids: ids.clone() })?;
            ctx.out(&a, |a| {
                ctx.text
                    .with("cli.note.deleted", "count", a.notes.to_string())
            })
        }
        NoteCommand::Restore { ids } => {
            let a = call::<m::NotesRestore>(app, &NoteIds { ids: ids.clone() })?;
            ctx.out(&a, |a| {
                ctx.text
                    .with("cli.note.restored", "count", a.notes.to_string())
            })
        }
    }
}

fn attribute(ctx: &Ctx<'_>, c: &AttributeCommand) -> Result<String, ApiError> {
    let app = ctx.app;
    let defined = |d: &AttributeDefDto| {
        let labels: Vec<&str> = d.options.iter().map(|o| o.label.as_str()).collect();
        ctx.text.text(
            "cli.attr.defined",
            &params(&[
                ("name", d.name.clone()),
                ("key", d.key.clone()),
                ("labels", labels.join(", ")),
            ]),
        )
    };
    match c {
        AttributeCommand::Define(def) => {
            let scope_deck_id = lookup::deck_id(app, def.deck.as_deref())?;
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
            ctx.out(&dto, defined)
        }
        AttributeCommand::ApplyTemplate {
            template,
            deck,
            locale,
        } => {
            let dto = call::<m::AttributesDefineFromTemplate>(
                app,
                &DefineAttributeFromTemplate {
                    template_id: template.clone(),
                    scope_deck_id: lookup::deck_id(app, deck.as_deref())?,
                    locale: *locale,
                },
            )?;
            ctx.out(&dto, defined)
        }
        AttributeCommand::Set { note, key, value } => {
            let n = call::<m::NotesGet>(app, &NoteRef { id: *note })?;
            let defs = lookup::effective(app, n.home_deck_id)?;
            let def = lookup::match_attr(defs.iter().map(|e| &e.def), key)?;
            let value = value
                .as_deref()
                .map(|v| lookup::value(def, v))
                .transpose()?;
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
            ctx.out(&res, |r| {
                match r
                    .values
                    .iter()
                    .find(|a| a.attr_id == def.id)
                    .and_then(|a| a.value.as_ref())
                {
                    Some(v) => ctx.text.text(
                        "cli.attr.set",
                        &params(&[("key", def.key.clone()), ("value", lookup::label(def, v))]),
                    ),
                    None => ctx.text.with("cli.attr.cleared", "key", def.key.clone()),
                }
            })
        }
        AttributeCommand::List { deck } => {
            let defs: Vec<AttributeDefDto> = match deck {
                Some(p) => lookup::effective(app, lookup::deck(app, p)?.id)?
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
            ctx.out(&defs, |defs| {
                if defs.is_empty() {
                    return ctx.text.text("cli.attr.none", &BTreeMap::new());
                }
                defs.iter().map(def_line).collect::<Vec<_>>().join("\n")
            })
        }
        AttributeCommand::Option(o) => option(ctx, o),
    }
}

fn resolve(app: &App, r: &AttrRef) -> Result<AttributeDefDto, ApiError> {
    lookup::attr(app, &r.attr, r.deck.as_deref())
}

fn option(ctx: &Ctx<'_>, c: &OptionCommand) -> Result<String, ApiError> {
    let app = ctx.app;
    match c {
        OptionCommand::Add { attr, label, after } => {
            let def = resolve(app, attr)?;
            let after = after
                .as_deref()
                .map(|l| lookup::option_id(&def, l))
                .transpose()?;
            let d = call::<m::AttributesOptionAdd>(
                app,
                &AddAttributeOption {
                    attr_id: def.id,
                    label: label.clone(),
                    color: None,
                    after,
                    id: None,
                },
            )?;
            ctx.out(&d, def_line)
        }
        OptionCommand::Rename {
            attr,
            label,
            new_label,
        } => {
            let def = resolve(app, attr)?;
            let d = call::<m::AttributesOptionRename>(
                app,
                &RenameAttributeOption {
                    attr_id: def.id,
                    option_id: lookup::option_id(&def, label)?,
                    label: new_label.clone(),
                },
            )?;
            ctx.out(&d, def_line)
        }
        OptionCommand::Reorder { attr, labels } => {
            let def = resolve(app, attr)?;
            let order = labels
                .iter()
                .map(|l| lookup::option_id(&def, l))
                .collect::<Result<Vec<_>, _>>()?;
            let d = call::<m::AttributesOptionReorder>(
                app,
                &ReorderAttributeOptions {
                    attr_id: def.id,
                    order,
                },
            )?;
            ctx.out(&d, def_line)
        }
        OptionCommand::Delete {
            attr,
            label,
            replace_with,
        } => {
            let def = resolve(app, attr)?;
            let replace_with = replace_with
                .as_deref()
                .map(|l| lookup::option_id(&def, l))
                .transpose()?;
            let r = call::<m::AttributesOptionDelete>(
                app,
                &DeleteAttributeOption {
                    attr_id: def.id,
                    option_id: lookup::option_id(&def, label)?,
                    replace_with,
                },
            )?;
            ctx.out(&r, |r| {
                ctx.text.text(
                    "cli.attr.option_deleted",
                    &params(&[
                        ("label", label.clone()),
                        ("key", def.key.clone()),
                        ("count", r.values_affected.to_string()),
                    ]),
                )
            })
        }
    }
}

fn path_str(p: &std::path::Path) -> String {
    p.display().to_string()
}

fn wire_name<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn permission_list(text: &Catalog, perms: &[PermissionDto]) -> String {
    let mut buf = String::new();
    for p in perms {
        let key = match (p.granted, p.sensitive) {
            (true, _) => "cli.plugin.perm_granted",
            (false, true) => "cli.plugin.perm_sensitive",
            (false, false) => "cli.plugin.perm_off",
        };
        let _ = writeln!(
            buf,
            "  {}",
            text.with(key, "permission", p.permission.clone())
        );
    }
    buf.trim_end().to_owned()
}

fn plugin_line(text: &Catalog, p: &PluginDto) -> String {
    text.text(
        "cli.plugin.line",
        &params(&[
            ("name", p.name.clone()),
            ("version", p.version.clone()),
            ("id", p.id.clone()),
            ("status", wire_name(&p.status)),
        ]),
    )
}

fn plugin(ctx: &Ctx<'_>, c: &PluginCommand) -> Result<String, ApiError> {
    let app = ctx.app;
    let set_enabled = |id: &str, enabled: bool| -> Result<String, ApiError> {
        let p = call::<m::PluginsSetEnabled>(
            app,
            &SetPluginEnabled {
                id: id.to_owned(),
                enabled,
            },
        )?;
        ctx.out(&p, |p| plugin_line(&ctx.text, p))
    };
    match c {
        PluginCommand::List => {
            let list = call::<m::PluginsList>(app, &Empty {})?;
            ctx.out(&list, |l| {
                if l.plugins.is_empty() {
                    return ctx.text.text("cli.plugin.none", &BTreeMap::new());
                }
                l.plugins
                    .iter()
                    .map(|p| plugin_line(&ctx.text, p))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
        PluginCommand::Inspect { path } => {
            let info = call::<m::PluginsInspect>(
                app,
                &InspectPackage {
                    path: path_str(path),
                },
            )?;
            ctx.out(&info, |i| {
                let head = ctx.text.text(
                    "cli.plugin.package",
                    &params(&[
                        ("name", i.name.clone()),
                        ("version", i.version.clone()),
                        ("id", i.id.clone()),
                    ]),
                );
                format!("{head}\n{}", permission_list(&ctx.text, &i.permissions))
                    .trim_end()
                    .to_owned()
            })
        }
        PluginCommand::Install {
            path,
            grant,
            grant_all,
        } => {
            let path = path_str(path);
            let grant = if *grant_all {
                call::<m::PluginsInspect>(app, &InspectPackage { path: path.clone() })?
                    .permissions
                    .into_iter()
                    .map(|p| p.permission)
                    .collect()
            } else {
                grant.clone()
            };
            let p = call::<m::PluginsInstall>(app, &InstallPlugin { path, grant })?;
            ctx.out(&p, |p| {
                let head = ctx.text.text(
                    "cli.plugin.installed",
                    &params(&[("name", p.name.clone()), ("version", p.version.clone())]),
                );
                format!("{head}\n{}", permission_list(&ctx.text, &p.permissions))
                    .trim_end()
                    .to_owned()
            })
        }
        PluginCommand::Uninstall { id, delete_data } => {
            let p = call::<m::PluginsUninstall>(
                app,
                &UninstallPlugin {
                    id: id.clone(),
                    delete_data: *delete_data,
                },
            )?;
            ctx.out(&p, |p| {
                ctx.text
                    .with("cli.plugin.uninstalled", "name", p.name.clone())
            })
        }
        PluginCommand::Enable { id } => set_enabled(id, true),
        PluginCommand::Disable { id } => set_enabled(id, false),
        PluginCommand::Grant { id, permissions } => {
            let p = call::<m::PluginsSetPermissions>(
                app,
                &SetPluginPermissions {
                    id: id.clone(),
                    granted: permissions.clone(),
                },
            )?;
            ctx.out(&p, |p| {
                format!(
                    "{}\n{}",
                    plugin_line(&ctx.text, p),
                    permission_list(&ctx.text, &p.permissions)
                )
                .trim_end()
                .to_owned()
            })
        }
        PluginCommand::Logs { id } => {
            let logs = call::<m::PluginsLogs>(app, &PluginRef { id: id.clone() })?;
            ctx.out(&logs, |l| {
                if l.entries.is_empty() {
                    return ctx.text.text("cli.plugin.no_logs", &BTreeMap::new());
                }
                l.entries
                    .iter()
                    .map(|e| format!("{} {} {}", e.at.0, wire_name(&e.level), e.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
        PluginCommand::Run {
            id,
            command,
            params: raw,
        } => {
            let params: Value = match raw.as_deref().map(str::trim) {
                None | Some("") => Value::Null,
                Some(s) => serde_json::from_str(s).map_err(|e| invalid(format!("params: {e}")))?,
            };
            let r = call::<m::PluginsRun>(
                app,
                &RunPluginCommand {
                    plugin_id: id.clone(),
                    command: command.clone(),
                    params,
                },
            )?;
            pretty(&r.result)
        }
        PluginCommand::Pack { dir, wasm, out } => pack(ctx, dir, wasm.as_deref(), out.as_deref()),
    }
}

/// `fc plugin pack`: a developer tool on local files, no collection involved.
fn pack(
    ctx: &Ctx<'_>,
    dir: &std::path::Path,
    wasm: Option<&std::path::Path>,
    out: Option<&std::path::Path>,
) -> Result<String, ApiError> {
    let io = |e: std::io::Error| ApiError::new(ErrorCode::IoFile, e.to_string());
    let wasm = wasm.map(std::fs::read).transpose().map_err(io)?;
    let bytes = fc_plugins::package::pack_dir(dir, wasm.as_deref())
        .map_err(|e| ApiError::new(ErrorCode::ValidationPluginPackage, e.to_string()))?;
    // Same checks as an install (manifest, permissions, API version).
    let pkg = fc_plugins::Package::read(&bytes, fc_plugins::APP_VERSION)
        .map_err(|e| ApiError::new(ErrorCode::ValidationPluginPackage, e.to_string()))?;
    let m = &pkg.manifest;
    let target = out.map_or_else(
        || dir.join(format!("{}-{}.fcplugin", m.id, m.version)),
        std::path::Path::to_path_buf,
    );
    std::fs::write(&target, &bytes).map_err(io)?;
    let result = serde_json::json!({
        "path": path_str(&target),
        "id": m.id,
        "version": m.version,
        "sha256": pkg.sha256,
    });
    ctx.out(&result, |_| {
        ctx.text.text(
            "cli.plugin.packed",
            &params(&[
                ("path", path_str(&target)),
                ("id", m.id.clone()),
                ("version", m.version.clone()),
            ]),
        )
    })
}
