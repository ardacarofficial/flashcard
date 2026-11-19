//! Resolving human references (deck paths, attribute keys/names, option labels) to ids, via
//! public API queries only.

use fc_api::v1::attributes::{
    AttributeDefDto, EffectiveAttributeDto, EffectiveAttributes, ListAttributes,
};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{DeckDto, DeckRef};
use fc_api::v1::methods as m;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_app::App;
use fc_core::attribute::{AttributeKind, AttributeOption, AttributeTarget, AttributeValue};
use fc_core::deck::DeckPath;
use fc_core::{AttributeOptionId, DeckId};

/// Calls `M` as the CLI actor.
pub fn call<M: ApiMethod>(app: &App, req: &M::Request) -> Result<M::Response, ApiError> {
    app.call::<M>(req, &Actor::Cli)
}

fn invalid(message: String) -> ApiError {
    ApiError::new(ErrorCode::ValidationInvalidParams, message)
}

/// Case-insensitive comparison used for every human reference.
fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// A live deck by path (`A::B`, case-insensitive) or by id.
pub fn deck(app: &App, reference: &str) -> Result<DeckDto, ApiError> {
    if let Ok(id) = reference.trim().parse::<DeckId>() {
        return call::<m::DecksGet>(app, &DeckRef { id });
    }
    let wanted = DeckPath::parse(reference)
        .map_err(fc_app::error::core_to_api)?
        .to_string();
    call::<m::DecksList>(app, &Empty {})?
        .decks
        .into_iter()
        .find(|d| same(&d.path, &wanted))
        .ok_or_else(|| {
            ApiError::new(ErrorCode::NotFoundDeck, format!("no deck {reference:?}"))
                .with_detail("path", reference)
        })
}

/// Optional deck reference → id.
pub fn deck_id(app: &App, reference: Option<&str>) -> Result<Option<DeckId>, ApiError> {
    reference.map(|r| deck(app, r).map(|d| d.id)).transpose()
}

/// Definitions applying to notes in `deck` (inheritance resolved).
pub fn effective(app: &App, deck: DeckId) -> Result<Vec<EffectiveAttributeDto>, ApiError> {
    Ok(call::<m::AttributesEffective>(
        app,
        &EffectiveAttributes {
            deck_id: deck,
            target: AttributeTarget::Note,
        },
    )?
    .attributes)
}

/// The definition whose key **or** (localised) name equals `reference`, case-insensitively.
/// A key match wins over a name match; two different definitions matching equally is an
/// error (pass `--deck` to narrow).
pub fn match_attr<'a>(
    defs: impl IntoIterator<Item = &'a AttributeDefDto>,
    reference: &str,
) -> Result<&'a AttributeDefDto, ApiError> {
    let defs: Vec<&AttributeDefDto> = defs.into_iter().collect();
    let by_key: Vec<_> = defs.iter().filter(|d| same(&d.key, reference)).collect();
    let hits = if by_key.is_empty() {
        defs.iter().filter(|d| same(&d.name, reference)).collect()
    } else {
        by_key
    };
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(ApiError::new(
            ErrorCode::NotFoundAttribute,
            format!("no classification {reference:?} applies here"),
        )
        .with_detail("key", reference)),
        _ => Err(invalid(format!(
            "{reference:?} matches several classifications; pass --deck to choose"
        ))
        .with_detail("key", reference)),
    }
}

/// A definition by key or name: among those applying to `deck` (note and card level), or
/// among all live definitions.
pub fn attr(app: &App, reference: &str, deck: Option<&str>) -> Result<AttributeDefDto, ApiError> {
    let defs: Vec<AttributeDefDto> = match deck {
        Some(path) => {
            let id = self::deck(app, path)?.id;
            let mut v: Vec<_> = effective(app, id)?.into_iter().map(|e| e.def).collect();
            v.extend(
                call::<m::AttributesEffective>(
                    app,
                    &EffectiveAttributes {
                        deck_id: id,
                        target: AttributeTarget::Card,
                    },
                )?
                .attributes
                .into_iter()
                .map(|e| e.def),
            );
            v
        }
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
    match_attr(&defs, reference).cloned()
}

/// An option of `def` by label (case-insensitive).
pub fn option<'a>(def: &'a AttributeDefDto, label: &str) -> Result<&'a AttributeOption, ApiError> {
    def.options
        .iter()
        .find(|o| same(&o.label, label))
        .ok_or_else(|| {
            ApiError::new(
                ErrorCode::NotFoundAttributeOption,
                format!("{} has no option {label:?}", def.key),
            )
            .with_detail("key", def.key.clone())
            .with_detail("label", label)
        })
}

/// Option id by label.
pub fn option_id(def: &AttributeDefDto, label: &str) -> Result<AttributeOptionId, ApiError> {
    option(def, label).map(|o| o.id)
}

/// Parses a CLI value for `def` (option labels resolved case-insensitively).
pub fn value(def: &AttributeDefDto, raw: &str) -> Result<AttributeValue, ApiError> {
    let bad = |why: &str| {
        ApiError::new(
            ErrorCode::ValidationAttributeValue,
            format!("{}: {why}", def.key),
        )
        .with_detail("key", def.key.clone())
    };
    Ok(match def.kind {
        AttributeKind::OrderedEnum | AttributeKind::Enum => {
            AttributeValue::Choice(option_id(def, raw)?)
        }
        AttributeKind::MultiEnum => AttributeValue::Choices(
            raw.split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|l| option_id(def, l))
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

/// Splits `KEY=VALUE`.
pub fn pair(raw: &str) -> Result<(&str, &str), ApiError> {
    raw.split_once('=')
        .map(|(k, v)| (k.trim(), v))
        .ok_or_else(|| invalid(format!("--attr {raw:?}: expected KEY=VALUE")))
}

/// Human label of a value of `def`.
pub fn label(def: &AttributeDefDto, v: &AttributeValue) -> String {
    let name = |id: &AttributeOptionId| {
        def.options
            .iter()
            .find(|o| o.id == *id)
            .map_or_else(|| id.to_string(), |o| o.label.clone())
    };
    match v {
        AttributeValue::Choice(id) => name(id),
        AttributeValue::Choices(ids) => ids.iter().map(name).collect::<Vec<_>>().join(", "),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}
