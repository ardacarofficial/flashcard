//! The MCP server's tool catalog (docs/06 §2.1) and its execution.
//!
//! A tool is **not** a new capability: it resolves human references (deck paths, note type
//! and field names, classification keys/names, option labels) through public queries and then
//! calls public v1 methods as [`Actor::Mcp`]. The transport (`fc-mcp`) only maps
//! [`catalog`] to MCP tool definitions and forwards `tools/call` to [`call_tool`].
//!
//! Safety semantics:
//! * Nothing deletes. Write tools are hidden and refused when writes are off
//!   ([`super::ALLOW_WRITES`] or the transport's read-only flag).
//! * `add_notes` is one `batch.execute` = one transaction = one undo step.
//! * `set_classification` (bulk, overwrites values) only previews until called again with
//!   `confirm: true`.

use super::{ALLOW_WRITES, ID};
use crate::app::App;
use crate::error::AppResult;
use crate::handler::QueryCtx;
use fc_api::envelope::InvokeRequest;
use fc_api::v1::attributes::{
    AttributeAssignment, AttributeDefDto, EffectiveAttributes, ListAttributes,
};
use fc_api::v1::bulk::SetAttributeBulk;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto, DeckRef, DeckTreeNode};
use fc_api::v1::mcp::{McpToolAccess, McpToolInfo, McpToolList};
use fc_api::v1::methods as m;
use fc_api::v1::notes::{CreateNote, NoteDto, NoteRef, UpdateNote};
use fc_api::v1::notetypes::NoteTypeDto;
use fc_api::v1::oplog::{Actor, ExecuteBatch};
use fc_api::v1::search::{QueryText, SearchCards};
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::attribute::{AttributeKind, AttributeTarget, AttributeValue};
use fc_core::deck::DeckPath;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::query::text::fold;
use fc_core::settings::SettingScope;
use fc_core::{CardId, DeckId, NoteId};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Most notes one `add_notes` call may create.
pub const MAX_NOTES_PER_CALL: usize = 100;
/// Most rows one `search_cards` call returns.
pub const MAX_SEARCH_ROWS: u32 = 100;

/// One tool of the catalog.
#[derive(Debug, Clone, Copy)]
pub struct ToolSpec {
    /// MCP tool name.
    pub name: &'static str,
    /// Human title (English, for MCP clients; the app's UI uses `mcp.tools.<name>`).
    pub title: &'static str,
    /// Description for the model (English).
    pub description: &'static str,
    /// Read or write.
    pub access: McpToolAccess,
    /// Overwrites existing content (`destructiveHint`).
    pub destructive: bool,
    /// Calling twice with the same arguments has no further effect (`idempotentHint`).
    pub idempotent: bool,
    /// Public v1 methods the tool calls.
    pub methods: &'static [&'static str],
    /// JSON Schema of the arguments.
    pub input_schema: fn() -> Value,
}

impl ToolSpec {
    /// API view.
    pub fn info(&self) -> McpToolInfo {
        McpToolInfo {
            name: self.name.to_owned(),
            access: self.access,
            destructive: self.destructive,
            methods: self.methods.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
}

/// Short guidance sent as the server's `instructions`.
pub const INSTRUCTIONS: &str = "Flashcard collection tools. Start with list_decks (decks are \
addressed by path like 'English::Verbs' or by id), list_note_types (default note type: \
Classic with fields front/back) and list_classifications (user-defined dimensions such as \
Seviye/Level with options A1..C2). search_cards takes the app's query language, e.g. \
'deck:English seviye>=B1 tag:verbs is:due'. Writes are normal undoable operations in the \
app; nothing here deletes.";

fn schema_empty() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn classifications_schema() -> Value {
    json!({
        "type": "object",
        "description": "Classification key or name → value. Choice kinds take the option \
label (e.g. {\"Seviye\": \"B1\"}); multi-choice takes a list of labels; number/date/text/bool \
take a plain value. null clears (update only).",
        "additionalProperties": {
            "type": ["string", "number", "boolean", "array", "null"],
            "items": { "type": "string" }
        }
    })
}

fn fields_schema() -> Value {
    json!({
        "type": "object",
        "description": "Field key or name → value (HTML or plain text), e.g. \
{\"front\": \"to go\", \"back\": \"gitmek\"}.",
        "additionalProperties": { "type": "string" }
    })
}

fn schema_list_classifications() -> Value {
    json!({
        "type": "object",
        "properties": {
            "deck": { "type": "string", "description": "Deck path or id." }
        },
        "required": ["deck"],
        "additionalProperties": false
    })
}

fn schema_search_cards() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "Query, e.g. 'deck:English \
seviye>=B1 -is:suspended'. Empty = all cards." },
            "limit": { "type": "integer", "minimum": 1, "maximum": MAX_SEARCH_ROWS,
                       "description": "Rows to return (default 20)." },
            "offset": { "type": "integer", "minimum": 0 },
            "notes_only": { "type": "boolean", "description": "One row per note." }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

fn schema_get_note() -> Value {
    json!({
        "type": "object",
        "properties": { "note_id": { "type": "string" } },
        "required": ["note_id"],
        "additionalProperties": false
    })
}

fn schema_create_deck() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Full path, e.g. 'English::Verbs'; \
missing parents are created. Returns the existing deck if it already exists." },
            "description": { "type": "string" }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

fn schema_add_notes() -> Value {
    json!({
        "type": "object",
        "properties": {
            "deck": { "type": "string", "description": "Deck path or id." },
            "note_type": { "type": "string", "description": "Note type name or id \
(default: Classic)." },
            "notes": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_NOTES_PER_CALL,
                "items": {
                    "type": "object",
                    "properties": {
                        "fields": fields_schema(),
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "classifications": classifications_schema()
                    },
                    "required": ["fields"],
                    "additionalProperties": false
                }
            },
            "allow_duplicates": { "type": "boolean", "description": "Default false: a \
duplicate of an existing note fails the whole call (nothing is added)." }
        },
        "required": ["deck", "notes"],
        "additionalProperties": false
    })
}

fn schema_update_note() -> Value {
    json!({
        "type": "object",
        "properties": {
            "note_id": { "type": "string" },
            "fields": fields_schema(),
            "tags": { "type": "array", "items": { "type": "string" },
                      "description": "Replaces all tags." },
            "classifications": classifications_schema(),
            "deck": { "type": "string", "description": "New home deck (path or id)." }
        },
        "required": ["note_id"],
        "additionalProperties": false
    })
}

fn schema_set_classification() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "Which cards/notes, in the query \
language (e.g. 'deck:English tag:verbs')." },
            "classification": { "type": "string", "description": "Classification key or \
name." },
            "value": { "type": ["string", "number", "boolean", "array", "null"],
                       "items": { "type": "string" },
                       "description": "Option label / value; null clears." },
            "confirm": { "type": "boolean", "description": "false/omitted = preview only \
(how many notes would change, plus a confirm_token). To apply, call again with the same \
arguments, confirm: true and that confirm_token." },
            "confirm_token": { "type": "string", "description": "From the preview of these \
exact arguments; refused when the matching cards changed since." }
        },
        "required": ["query", "classification", "value"],
        "additionalProperties": false
    })
}

/// The catalog, in listing order.
pub fn catalog() -> &'static [ToolSpec] {
    use McpToolAccess::{Read, Write};
    const CATALOG: &[ToolSpec] = &[
        ToolSpec {
            name: "list_decks",
            title: "List decks",
            description: "All decks with their full path, id and card counts (new, \
learning, due today, total; sub-decks included).",
            access: Read,
            destructive: false,
            idempotent: true,
            methods: &[m::DecksTree::NAME],
            input_schema: schema_empty,
        },
        ToolSpec {
            name: "list_note_types",
            title: "List note types",
            description: "Note types with their fields (key, name, required). Use field \
keys or names in add_notes.",
            access: Read,
            destructive: false,
            idempotent: true,
            methods: &[m::NotetypesList::NAME],
            input_schema: schema_empty,
        },
        ToolSpec {
            name: "list_classifications",
            title: "List classifications",
            description: "Classifications (user-defined dimensions like Seviye = A1…C2, \
inherited from parent decks) that apply to notes and cards in a deck, with their options \
in order. Their keys work in search_cards (e.g. seviye>=B1).",
            access: Read,
            destructive: false,
            idempotent: true,
            methods: &[m::DecksList::NAME, m::AttributesEffective::NAME],
            input_schema: schema_list_classifications,
        },
        ToolSpec {
            name: "search_cards",
            title: "Search cards",
            description: "Searches cards with the app's query language: words, \
\"phrases\", deck:Name, tag:x, is:due/new/suspended, flag:red, classification comparisons \
(seviye:B1, seviye>=B1), field:value, -negation, or. Returns a page of matches.",
            access: Read,
            destructive: false,
            idempotent: true,
            methods: &[m::SearchCardsMethod::NAME],
            input_schema: schema_search_cards,
        },
        ToolSpec {
            name: "get_note",
            title: "Get note",
            description: "One note: fields, tags, classifications (as labels), home deck \
and its cards.",
            access: Read,
            destructive: false,
            idempotent: true,
            methods: &[
                m::NotesGet::NAME,
                m::DecksGet::NAME,
                m::AttributesList::NAME,
            ],
            input_schema: schema_get_note,
        },
        ToolSpec {
            name: "create_deck",
            title: "Create deck",
            description: "Creates a deck from a path (parents too), or returns it if it \
already exists.",
            access: Write,
            destructive: false,
            idempotent: true,
            methods: &[m::DecksList::NAME, m::DecksCreate::NAME],
            input_schema: schema_create_deck,
        },
        ToolSpec {
            name: "add_notes",
            title: "Add notes",
            description: "Adds notes (and their cards) to a deck in one undoable step, with \
optional tags and classification values (e.g. {\"Seviye\": \"B1\"}). All or nothing.",
            access: Write,
            destructive: false,
            idempotent: false,
            methods: &[
                m::DecksList::NAME,
                m::NotetypesList::NAME,
                m::AttributesEffective::NAME,
                m::BatchExecute::NAME,
                m::NotesCreate::NAME,
            ],
            input_schema: schema_add_notes,
        },
        ToolSpec {
            name: "update_note",
            title: "Update note",
            description: "Changes the given fields, tags, classifications or home deck of \
one note; everything not given stays. Undoable in the app.",
            access: Write,
            destructive: true,
            idempotent: true,
            methods: &[
                m::NotesGet::NAME,
                m::NotetypesList::NAME,
                m::DecksList::NAME,
                m::AttributesEffective::NAME,
                m::NotesUpdate::NAME,
            ],
            input_schema: schema_update_note,
        },
        ToolSpec {
            name: "set_classification",
            title: "Set classification",
            description: "Sets (or clears) one classification on every note/card matching \
a query. Without confirm: true it only previews (how many would change, plus a \
confirm_token); to apply, repeat the call with confirm: true and that token.",
            access: Write,
            destructive: true,
            idempotent: true,
            methods: &[
                m::SearchIdsMethod::NAME,
                m::AttributesList::NAME,
                m::BulkSetAttribute::NAME,
            ],
            input_schema: schema_set_classification,
        },
    ];
    CATALOG
}

/// Whether an MCP client may call public method `name` at all: only those some tool of the
/// [`catalog`] calls (the registry applies this to every call of the `mcp` actor, including
/// each inside `batch.execute`).
pub fn method_reachable(name: &str) -> bool {
    catalog().iter().any(|t| t.methods.contains(&name))
}

/// Whether MCP write tools may run: module setting [`ALLOW_WRITES`] and not `read_only`.
pub fn writes_allowed(app: &App, read_only: bool) -> Result<bool, ApiError> {
    if read_only {
        return Ok(false);
    }
    let v = app.call::<m::SettingsGet>(
        &fc_api::v1::settings::GetSettings {
            scope: SettingScope::App,
            keys: Some(vec![ALLOW_WRITES.to_owned()]),
        },
        &Actor::System,
    )?;
    Ok(v.values
        .iter()
        .find(|s| s.key == ALLOW_WRITES)
        .and_then(|s| s.value.as_bool())
        .unwrap_or(true))
}

/// Picks up changes another process (the desktop app) made to module switches, then checks
/// the `mcp` module is on.
fn ensure_enabled(app: &App) -> Result<(), ApiError> {
    app.reload_cached_state()
        .map_err(crate::error::AppError::into_api)?;
    if app.is_module_enabled(ID) {
        Ok(())
    } else {
        Err(
            ApiError::new(ErrorCode::ConflictModuleDisabled, "module mcp is disabled")
                .with_detail("module", ID),
        )
    }
}

/// Tools an MCP client may see now: none when the module is off, read tools only when writes
/// are off.
pub fn list_tools(app: &App, read_only: bool) -> Result<Vec<&'static ToolSpec>, ApiError> {
    ensure_enabled(app)?;
    let writes = writes_allowed(app, read_only)?;
    Ok(catalog()
        .iter()
        .filter(|t| writes || t.access == McpToolAccess::Read)
        .collect())
}

/// Runs tool `name` for MCP client `client` (actor `mcp:<client>`). Errors are API errors
/// (`code`, `details`) the transport hands back as a tool error.
pub fn call_tool(
    app: &App,
    client: &str,
    read_only: bool,
    name: &str,
    args: Value,
) -> Result<Value, ApiError> {
    ensure_enabled(app)?;
    let spec = catalog().iter().find(|t| t.name == name).ok_or_else(|| {
        ApiError::new(ErrorCode::NotFoundMcpTool, format!("unknown tool {name:?}"))
            .with_detail("tool", name)
    })?;
    if spec.access == McpToolAccess::Write && !writes_allowed(app, read_only)? {
        return Err(ApiError::new(
            ErrorCode::PermissionMcpReadOnly,
            format!("{name} writes, and MCP writes are turned off in the app"),
        )
        .with_detail("tool", name));
    }
    let client = client.trim();
    let cx = Cx {
        app,
        actor: Actor::Mcp(if client.is_empty() { "mcp" } else { client }.to_owned()),
    };
    let args = if args.is_null() {
        Value::Object(Map::new())
    } else {
        args
    };
    match name {
        "list_decks" => parse::<NoArgs>(args).and_then(|_| cx.list_decks()),
        "list_note_types" => parse::<NoArgs>(args).and_then(|_| cx.list_note_types()),
        "list_classifications" => cx.list_classifications(parse(args)?),
        "search_cards" => cx.search_cards(parse(args)?),
        "get_note" => cx.get_note(parse(args)?),
        "create_deck" => cx.create_deck(parse(args)?),
        "add_notes" => cx.add_notes(parse(args)?),
        "update_note" => cx.update_note(parse(args)?),
        "set_classification" => cx.set_classification(parse(args)?),
        _ => Err(ApiError::new(ErrorCode::Internal, format!("tool {name} has no runner"))),
    }
}

/// `mcp.tools.list`.
pub(super) fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<McpToolList> {
    let writes_allowed = ctx
        .tx
        .get_setting(&SettingScope::App, ALLOW_WRITES)?
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    Ok(McpToolList {
        tools: catalog().iter().map(ToolSpec::info).collect(),
        writes_allowed,
    })
}

// ------------------------------------------------------------------ arguments

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::ValidationInvalidParams, message)
}

fn parse<T: DeserializeOwned>(args: Value) -> Result<T, ApiError> {
    serde_json::from_value(args).map_err(|e| invalid(format!("invalid arguments: {e}")))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckArg {
    deck: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    limit: Option<u32>,
    offset: Option<u32>,
    notes_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteArg {
    note_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateDeckArgs {
    path: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewNote {
    fields: BTreeMap<String, String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    classifications: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddNotesArgs {
    deck: String,
    note_type: Option<String>,
    notes: Vec<NewNote>,
    allow_duplicates: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateNoteArgs {
    note_id: String,
    fields: Option<BTreeMap<String, String>>,
    tags: Option<Vec<String>>,
    #[serde(default)]
    classifications: BTreeMap<String, Value>,
    deck: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetClassificationArgs {
    query: String,
    classification: String,
    value: Value,
    confirm: Option<bool>,
    confirm_token: Option<String>,
}

/// Most cards one confirmed `set_classification` may change; bigger selections are narrowed
/// by the model (or done in the app).
pub const MAX_BULK_CARDS: usize = 2000;

/// Binds a confirmation to what the preview showed: the classification, the value and the
/// exact set of matched cards (FNV-1a, two seeds). Not a secret — it proves the apply call
/// saw a preview of this very selection, and breaks when the matches change in between.
fn confirm_token(def: &AttributeDefDto, value: &Option<AttributeValue>, cards: &[CardId]) -> String {
    let mut ids: Vec<String> = cards.iter().map(ToString::to_string).collect();
    ids.sort_unstable();
    let canonical = format!(
        "{}|{}|{}",
        def.id,
        serde_json::to_string(value).unwrap_or_default(),
        ids.join(",")
    );
    let fnv = |seed: u64| {
        canonical.bytes().fold(seed, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    };
    format!("{:016x}{:016x}", fnv(0xcbf2_9ce4_8422_2325), fnv(0x8422_2325_cbf2_9ce4))
}

// ------------------------------------------------------------------ execution

struct Cx<'a> {
    app: &'a App,
    actor: Actor,
}

fn same(a: &str, b: &str) -> bool {
    fold(a.trim()) == fold(b.trim())
}

fn kind_str<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Human view of a stored value (option ids → labels).
fn value_view(def: &AttributeDefDto, v: &AttributeValue) -> Value {
    let label = |id: &fc_core::AttributeOptionId| {
        def.options
            .iter()
            .find(|o| o.id == *id)
            .map_or_else(|| id.to_string(), |o| o.label.clone())
    };
    match v {
        AttributeValue::Choice(id) => Value::String(label(id)),
        AttributeValue::Choices(ids) => ids.iter().map(|i| Value::String(label(i))).collect(),
        AttributeValue::Number(n) => json!(n),
        AttributeValue::Text(s) | AttributeValue::Date(s) => Value::String(s.clone()),
        AttributeValue::Bool(b) => Value::Bool(*b),
    }
}

/// Parses a tool value for `def` (`None` = clear). Labels match case/accent-insensitively.
fn parse_value(def: &AttributeDefDto, raw: &Value) -> Result<Option<AttributeValue>, ApiError> {
    let bad = |why: &str| {
        ApiError::new(
            ErrorCode::ValidationAttributeValue,
            format!("{}: {why}", def.name),
        )
        .with_detail("key", def.key.clone())
    };
    let option = |label: &str| {
        def.options
            .iter()
            .find(|o| same(&o.label, label) || o.id.to_string() == label.trim())
            .map(|o| o.id)
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::NotFoundAttributeOption,
                    format!(
                        "{} has no option {label:?} (options: {})",
                        def.name,
                        def.options
                            .iter()
                            .map(|o| o.label.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
                .with_detail("key", def.key.clone())
                .with_detail("label", label)
            })
    };
    if raw.is_null() {
        return Ok(None);
    }
    let text = || match raw {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    };
    Ok(Some(match def.kind {
        AttributeKind::OrderedEnum | AttributeKind::Enum => {
            AttributeValue::Choice(option(&text().ok_or_else(|| bad("expected a label"))?)?)
        }
        AttributeKind::MultiEnum => {
            let labels: Vec<String> = match raw {
                Value::Array(items) => items
                    .iter()
                    .map(|i| i.as_str().map(str::to_owned).ok_or_else(|| bad("labels")))
                    .collect::<Result<_, _>>()?,
                Value::String(s) => s.split(',').map(str::to_owned).collect(),
                _ => return Err(bad("expected a list of labels")),
            };
            AttributeValue::Choices(
                labels
                    .iter()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| option(l))
                    .collect::<Result<_, _>>()?,
            )
        }
        AttributeKind::Number => AttributeValue::Number(match raw {
            Value::Number(n) => n.as_f64().ok_or_else(|| bad("not a number"))?,
            Value::String(s) => s.trim().parse().map_err(|_| bad("not a number"))?,
            _ => return Err(bad("not a number")),
        }),
        AttributeKind::Text => AttributeValue::Text(text().ok_or_else(|| bad("expected text"))?),
        AttributeKind::Date => {
            AttributeValue::Date(text().ok_or_else(|| bad("expected YYYY-MM-DD"))?)
        }
        AttributeKind::Bool => AttributeValue::Bool(match raw {
            Value::Bool(b) => *b,
            Value::String(s) => match fold(s.trim()).as_str() {
                "true" | "yes" | "evet" | "1" => true,
                "false" | "no" | "hayir" | "0" => false,
                _ => return Err(bad("expected true/false")),
            },
            _ => return Err(bad("expected true/false")),
        }),
    }))
}

/// The definition whose key or name is `reference` (key match first).
fn match_def<'d>(
    defs: &'d [AttributeDefDto],
    reference: &str,
    scope: &str,
) -> Result<&'d AttributeDefDto, ApiError> {
    let by_key: Vec<&AttributeDefDto> = defs.iter().filter(|d| same(&d.key, reference)).collect();
    let hits = if by_key.is_empty() {
        defs.iter().filter(|d| same(&d.name, reference)).collect()
    } else {
        by_key
    };
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(ApiError::new(
            ErrorCode::NotFoundAttribute,
            format!(
                "no classification {reference:?} {scope} (available: {})",
                defs.iter()
                    .map(|d| d.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
        .with_detail("key", reference)),
        _ => Err(invalid(format!(
            "{reference:?} matches several classifications; use its key"
        ))
        .with_detail("key", reference)),
    }
}

impl Cx<'_> {
    fn call<M: ApiMethod>(&self, req: &M::Request) -> Result<M::Response, ApiError> {
        self.app.call::<M>(req, &self.actor)
    }

    fn deck(&self, reference: &str) -> Result<DeckDto, ApiError> {
        if let Ok(id) = reference.trim().parse::<DeckId>() {
            return self.call::<m::DecksGet>(&DeckRef { id });
        }
        let wanted = DeckPath::parse(reference)
            .map_err(crate::error::core_to_api)?
            .to_string();
        self.call::<m::DecksList>(&Empty {})?
            .decks
            .into_iter()
            .find(|d| same(&d.path, &wanted))
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::NotFoundDeck,
                    format!("no deck {reference:?}; see list_decks or create_deck"),
                )
                .with_detail("path", reference)
            })
    }

    fn note_type(&self, reference: Option<&str>) -> Result<NoteTypeDto, ApiError> {
        let all = self.call::<m::NotetypesList>(&Empty {})?.notetypes;
        let found = match reference.map(str::trim).filter(|r| !r.is_empty()) {
            None => all.into_iter().find(|t| t.id == CLASSIC_ID),
            Some(r) => all.into_iter().find(|t| {
                t.id.to_string() == r || same(&t.name, r) || t.builtin.as_deref() == Some(r)
            }),
        };
        found.ok_or_else(|| {
            ApiError::new(
                ErrorCode::NotFoundNoteType,
                format!("no note type {reference:?}; see list_note_types"),
            )
            .with_detail("name", reference.unwrap_or_default())
        })
    }

    fn note_defs(&self, deck: DeckId) -> Result<Vec<AttributeDefDto>, ApiError> {
        Ok(self
            .call::<m::AttributesEffective>(&EffectiveAttributes {
                deck_id: deck,
                target: AttributeTarget::Note,
            })?
            .attributes
            .into_iter()
            .map(|e| e.def)
            .collect())
    }

    /// Field values keyed by the note type's field keys.
    fn fields(
        nt: &NoteTypeDto,
        given: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, ApiError> {
        given
            .iter()
            .map(|(k, v)| {
                nt.fields
                    .iter()
                    .find(|f| same(&f.key.0, k))
                    .or_else(|| nt.fields.iter().find(|f| same(&f.name, k)))
                    .map(|f| (f.key.0.clone(), v.clone()))
                    .ok_or_else(|| {
                        invalid(format!(
                            "{} has no field {k:?} (fields: {})",
                            nt.name,
                            nt.fields
                                .iter()
                                .map(|f| f.key.0.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                        .with_detail("field", k.as_str())
                    })
            })
            .collect()
    }

    fn assignments(
        defs: &[AttributeDefDto],
        given: &BTreeMap<String, Value>,
        deck_path: &str,
    ) -> Result<Vec<AttributeAssignment>, ApiError> {
        given
            .iter()
            .map(|(k, v)| {
                let def = match_def(defs, k, &format!("applies to notes in {deck_path}"))?;
                Ok(AttributeAssignment {
                    attr_id: def.id,
                    value: parse_value(def, v)?,
                })
            })
            .collect()
    }

    fn list_decks(&self) -> Result<Value, ApiError> {
        fn walk(nodes: &[DeckTreeNode], out: &mut Vec<Value>) {
            for n in nodes {
                out.push(json!({
                    "id": n.deck.id,
                    "path": n.deck.path,
                    "description": n.deck.description,
                    "cards": {
                        "new": n.counts.new,
                        "learning": n.counts.learning,
                        "due": n.counts.due,
                        "total": n.counts.total,
                    },
                }));
                walk(&n.children, out);
            }
        }
        let tree = self.call::<m::DecksTree>(&Empty {})?;
        let mut decks = Vec::new();
        walk(&tree.roots, &mut decks);
        Ok(json!({ "decks": decks }))
    }

    fn list_note_types(&self) -> Result<Value, ApiError> {
        let types = self.call::<m::NotetypesList>(&Empty {})?.notetypes;
        Ok(json!({
            "note_types": types.iter().map(|t| json!({
                "id": t.id,
                "name": t.name,
                "kind": kind_str(&t.kind),
                "builtin": t.builtin,
                "default": t.id == CLASSIC_ID,
                "fields": t.fields.iter().map(|f| json!({
                    "key": f.key.0,
                    "name": f.name,
                    "required": f.required,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>()
        }))
    }

    fn list_classifications(&self, a: DeckArg) -> Result<Value, ApiError> {
        let deck = self.deck(&a.deck)?;
        let mut out = Vec::new();
        for target in [AttributeTarget::Note, AttributeTarget::Card] {
            let defs = self
                .call::<m::AttributesEffective>(&EffectiveAttributes {
                    deck_id: deck.id,
                    target,
                })?
                .attributes;
            for e in defs {
                let d = &e.def;
                out.push(json!({
                    "key": d.key,
                    "name": d.name,
                    "kind": kind_str(&d.kind),
                    "applies_to": kind_str(&d.target),
                    "required": d.required,
                    "inherited": e.inherited,
                    "options": d.options.iter().map(|o| o.label.clone()).collect::<Vec<_>>(),
                    "default": d.default_value.as_ref().map(|v| value_view(d, v)),
                }));
            }
        }
        Ok(json!({ "deck": deck.path, "classifications": out }))
    }

    fn search_cards(&self, a: SearchArgs) -> Result<Value, ApiError> {
        let page = self.call::<m::SearchCardsMethod>(&SearchCards {
            query: a.query,
            sort: None,
            descending: None,
            notes_only: a.notes_only,
            offset: a.offset,
            limit: Some(a.limit.unwrap_or(20).clamp(1, MAX_SEARCH_ROWS)),
        })?;
        Ok(json!({
            "total": page.total,
            "offset": page.offset,
            "cards": page.items.iter().map(|r| json!({
                "card_id": r.card_id,
                "note_id": r.note_id,
                "deck": r.deck_path,
                "text": r.sort_field,
                "template": r.template_key,
                "state": kind_str(&r.state),
                "due": r.due,
                "suspended": r.suspended,
                "tags": r.tags,
            })).collect::<Vec<_>>()
        }))
    }

    fn note_id(raw: &str) -> Result<NoteId, ApiError> {
        raw.trim()
            .parse()
            .map_err(|_| invalid(format!("{raw:?} is not a note id")).with_detail("field", "note_id"))
    }

    fn get_note(&self, a: NoteArg) -> Result<Value, ApiError> {
        let note = self.call::<m::NotesGet>(&NoteRef {
            id: Self::note_id(&a.note_id)?,
        })?;
        let deck = self.call::<m::DecksGet>(&DeckRef {
            id: note.home_deck_id,
        })?;
        let defs = self
            .call::<m::AttributesList>(&ListAttributes {
                scope_deck_id: None,
                include_deleted: None,
            })?
            .defs;
        Ok(note_view(&note, &deck.path, &defs))
    }

    fn create_deck(&self, a: CreateDeckArgs) -> Result<Value, ApiError> {
        if let Ok(existing) = self.deck(&a.path) {
            return Ok(json!({ "created": false, "id": existing.id, "path": existing.path }));
        }
        let d = self.call::<m::DecksCreate>(&CreateDeck {
            path: a.path,
            parent_id: None,
            description: a.description,
            icon: None,
            color: None,
        })?;
        Ok(json!({ "created": true, "id": d.id, "path": d.path }))
    }

    fn add_notes(&self, a: AddNotesArgs) -> Result<Value, ApiError> {
        if a.notes.is_empty() || a.notes.len() > MAX_NOTES_PER_CALL {
            return Err(invalid(format!(
                "notes: give 1–{MAX_NOTES_PER_CALL} notes per call"
            )));
        }
        let deck = self.deck(&a.deck)?;
        let nt = self.note_type(a.note_type.as_deref())?;
        let defs = self.note_defs(deck.id)?;
        let mut calls = Vec::with_capacity(a.notes.len());
        for (i, n) in a.notes.iter().enumerate() {
            let at = |e: ApiError| e.with_detail("note_index", i);
            let req = CreateNote {
                notetype_id: nt.id,
                deck_id: deck.id,
                fields: Self::fields(&nt, &n.fields).map_err(at)?,
                tags: n.tags.clone(),
                attributes: Self::assignments(&defs, &n.classifications, &deck.path)
                    .map_err(at)?,
                allow_duplicate: a.allow_duplicates,
            };
            calls.push(InvokeRequest {
                method: format!("{}.{}", fc_api::API_VERSION, m::NotesCreate::NAME),
                params: serde_json::to_value(&req)
                    .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?,
            });
        }
        let r = self.call::<m::BatchExecute>(&ExecuteBatch { calls })?;
        let notes: Vec<Value> = r
            .results
            .into_iter()
            .map(|v| {
                let n: NoteDto = serde_json::from_value(v)
                    .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?;
                Ok(json!({ "note_id": n.id, "cards": n.cards.len(), "text": n.sort_field }))
            })
            .collect::<Result<_, ApiError>>()?;
        Ok(json!({
            "added": notes.len(),
            "deck": deck.path,
            "note_type": nt.name,
            "undo_batch_id": r.batch_id,
            "notes": notes,
        }))
    }

    fn update_note(&self, a: UpdateNoteArgs) -> Result<Value, ApiError> {
        let old = self.call::<m::NotesGet>(&NoteRef {
            id: Self::note_id(&a.note_id)?,
        })?;
        let nt = self
            .call::<m::NotetypesList>(&Empty {})?
            .notetypes
            .into_iter()
            .find(|t| t.id == old.notetype_id)
            .ok_or_else(|| ApiError::new(ErrorCode::NotFoundNoteType, "note type missing"))?;
        let mut fields = old.fields.clone();
        if let Some(given) = &a.fields {
            fields.extend(Self::fields(&nt, given)?);
        }
        let new_deck = a.deck.as_deref().map(|d| self.deck(d)).transpose()?;
        let home = new_deck.as_ref().map_or(old.home_deck_id, |d| d.id);
        let home_path = match &new_deck {
            Some(d) => d.path.clone(),
            None => self.call::<m::DecksGet>(&DeckRef { id: home })?.path,
        };
        let defs = self.note_defs(home)?;
        let attributes = Self::assignments(&defs, &a.classifications, &home_path)?;
        let tags = a
            .tags
            .unwrap_or_else(|| old.tags.iter().map(ToString::to_string).collect());
        let note = self.call::<m::NotesUpdate>(&UpdateNote {
            id: old.id,
            deck_id: new_deck.map(|d| d.id),
            fields,
            tags,
            attributes,
        })?;
        let defs = self
            .call::<m::AttributesList>(&ListAttributes {
                scope_deck_id: None,
                include_deleted: None,
            })?
            .defs;
        Ok(note_view(&note, &home_path, &defs))
    }

    fn set_classification(&self, a: SetClassificationArgs) -> Result<Value, ApiError> {
        let defs = self
            .call::<m::AttributesList>(&ListAttributes {
                scope_deck_id: None,
                include_deleted: None,
            })?
            .defs;
        let def = match_def(&defs, &a.classification, "exists")?;
        let value = parse_value(def, &a.value)?;
        let ids = self.call::<m::SearchIdsMethod>(&QueryText {
            query: a.query.clone(),
        })?;
        let preview = json!({
            "classification": def.name,
            "value": value.as_ref().map(|v| value_view(def, v)),
            "matched_cards": ids.card_ids.len(),
            "matched_notes": ids.note_ids.len(),
        });
        let token = confirm_token(def, &value, &ids.card_ids);
        let too_many = ids.card_ids.len() > MAX_BULK_CARDS;
        if a.confirm != Some(true) || ids.card_ids.is_empty() {
            let mut p = preview;
            p["applied"] = Value::Bool(false);
            if too_many {
                p["limit"] = json!(MAX_BULK_CARDS);
                p["next"] = Value::String(format!(
                    "Too many cards for one step (limit {MAX_BULK_CARDS}): narrow the query."
                ));
            } else if !ids.card_ids.is_empty() {
                p["confirm_token"] = Value::String(token);
                p["next"] = Value::String(
                    "Nothing changed yet. Call again with the same arguments, confirm: true \
and this confirm_token to apply."
                        .into(),
                );
            }
            return Ok(p);
        }
        if too_many {
            return Err(invalid(format!(
                "{} cards match; at most {MAX_BULK_CARDS} may change in one step: narrow the query",
                ids.card_ids.len()
            )));
        }
        if a.confirm_token.as_deref() != Some(token.as_str()) {
            return Err(invalid(
                "confirm needs the confirm_token of a preview of these exact arguments \
(missing, or the matching cards changed since the preview): preview again",
            ));
        }
        let r = self.call::<m::BulkSetAttribute>(&SetAttributeBulk {
            card_ids: ids.card_ids,
            attr_id: def.id,
            value,
        })?;
        let mut p = preview;
        p["applied"] = Value::Bool(true);
        p["changed"] = json!(r.changed);
        p["skipped"] = json!(r.skipped);
        Ok(p)
    }
}

fn note_view(note: &NoteDto, deck_path: &str, defs: &[AttributeDefDto]) -> Value {
    let classifications: Map<String, Value> = note
        .attributes
        .iter()
        .filter_map(|a| {
            let def = defs.iter().find(|d| d.id == a.attr_id)?;
            Some((
                def.name.clone(),
                a.value
                    .as_ref()
                    .map_or(Value::Null, |v| value_view(def, v)),
            ))
        })
        .collect();
    json!({
        "id": note.id,
        "note_type_id": note.notetype_id,
        "deck": deck_path,
        "fields": note.fields,
        "tags": note.tags,
        "classifications": classifications,
        "cards": note.cards.iter().map(|c| json!({
            "card_id": c.id,
            "deck_id": c.deck_id,
            "template": c.template_key,
            "state": kind_str(&c.state),
            "due": c.due,
            "suspended": c.suspended,
        })).collect::<Vec<_>>(),
        "updated_at": note.updated_at,
    })
}
