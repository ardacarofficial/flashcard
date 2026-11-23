//! Positioned query errors. Every error carries the [`Span`] of the offending text so the UI can
//! underline it; `kind.code()` selects the i18n message (`errors.query.<code>`).

use super::ast::Span;
use std::fmt;

/// What is wrong with a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryErrorKind {
    /// `"` without its closing quote.
    UnclosedQuote,
    /// `(` without its closing `)`.
    UnclosedParen,
    /// `)` without an opening `(`.
    UnmatchedParen,
    /// A term was expected (after `-`, `NOT`, `OR`, `(`…).
    ExpectedTerm,
    /// `key:` with nothing after it.
    EmptyValue {
        /// The key.
        key: String,
    },
    /// Not a built-in key, classification or field.
    UnknownKey {
        /// The key.
        key: String,
    },
    /// The key does not support this operator (`deck>x`, `seviye>=B1` on an unordered list).
    UnsupportedOperator {
        /// The key.
        key: String,
        /// The operator.
        op: String,
    },
    /// Not a number.
    InvalidNumber {
        /// The value.
        value: String,
    },
    /// Not a `YYYY-MM-DD` date.
    InvalidDate {
        /// The value.
        value: String,
    },
    /// Not yes/no.
    InvalidBool {
        /// The value.
        value: String,
    },
    /// No option of the classification has this label.
    UnknownOption {
        /// The key.
        key: String,
        /// The value.
        value: String,
    },
    /// Unknown `is:` value.
    UnknownState {
        /// The value.
        value: String,
    },
    /// Unknown `flag:` value.
    InvalidFlag {
        /// The value.
        value: String,
    },
    /// Invalid id in `nid:`/`cid:`.
    InvalidId {
        /// The value.
        value: String,
    },
    /// The regex does not compile.
    InvalidRegex {
        /// Compiler message.
        message: String,
    },
    /// v1.4.1: parentheses/negations nested deeper than [`super::parse::MAX_DEPTH`] (a pasted
    /// `((((…` used to overflow the stack).
    TooDeep,
}

impl QueryErrorKind {
    /// Stable code (`unclosed_quote`, …): i18n key suffix and `details.kind` on the wire.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnclosedQuote => "unclosed_quote",
            Self::UnclosedParen => "unclosed_paren",
            Self::UnmatchedParen => "unmatched_paren",
            Self::ExpectedTerm => "expected_term",
            Self::EmptyValue { .. } => "empty_value",
            Self::UnknownKey { .. } => "unknown_key",
            Self::UnsupportedOperator { .. } => "unsupported_operator",
            Self::InvalidNumber { .. } => "invalid_number",
            Self::InvalidDate { .. } => "invalid_date",
            Self::InvalidBool { .. } => "invalid_bool",
            Self::UnknownOption { .. } => "unknown_option",
            Self::UnknownState { .. } => "unknown_state",
            Self::InvalidFlag { .. } => "invalid_flag",
            Self::InvalidId { .. } => "invalid_id",
            Self::InvalidRegex { .. } => "invalid_regex",
            Self::TooDeep => "too_deep",
        }
    }

    /// Interpolation parameters (`key`, `value`, `op`, `message`).
    pub fn params(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::EmptyValue { key } | Self::UnknownKey { key } => vec![("key", key.clone())],
            Self::UnsupportedOperator { key, op } => {
                vec![("key", key.clone()), ("op", op.clone())]
            }
            Self::InvalidNumber { value }
            | Self::InvalidDate { value }
            | Self::InvalidBool { value }
            | Self::UnknownState { value }
            | Self::InvalidFlag { value }
            | Self::InvalidId { value } => vec![("value", value.clone())],
            Self::UnknownOption { key, value } => {
                vec![("key", key.clone()), ("value", value.clone())]
            }
            Self::InvalidRegex { message } => vec![("message", message.clone())],
            _ => Vec::new(),
        }
    }
}

/// A query error at a position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    /// What is wrong.
    pub kind: QueryErrorKind,
    /// Where (characters).
    pub span: Span,
}

impl QueryError {
    /// Error at `span`.
    pub fn new(kind: QueryErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "query error `{}` at {}..{}",
            self.kind.code(),
            self.span.start,
            self.span.end
        )?;
        for (k, v) in self.kind.params() {
            write!(f, " {k}={v:?}")?;
        }
        Ok(())
    }
}

impl std::error::Error for QueryError {}
