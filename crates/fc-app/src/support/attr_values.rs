//! Classification references and values as people (and tools) write them: a definition by key
//! or name, a value by option label. Shared by the MCP tools and the inbox, which both take
//! free text; case and accents never matter.

use fc_api::v1::attributes::AttributeDefDto;
use fc_api::{ApiError, ErrorCode};
use fc_core::attribute::{AttributeKind, AttributeValue};
use fc_core::query::text::fold;
use serde_json::Value;

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::ValidationInvalidParams, message)
}

fn same(a: &str, b: &str) -> bool {
    fold(a.trim()) == fold(b.trim())
}

/// Parses a tool value for `def` (`None` = clear). Labels match case/accent-insensitively.
pub(crate) fn parse_value(
    def: &AttributeDefDto,
    raw: &Value,
) -> Result<Option<AttributeValue>, ApiError> {
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
pub(crate) fn match_def<'d>(
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
