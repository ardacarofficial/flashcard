//! Syntax tree of a query, exactly as typed (no knowledge of decks, fields or attributes).
//! [`super::resolve`] turns it into a [`super::Filter`].

use serde::{Deserialize, Serialize};
use std::fmt;

/// A range of the query text in **characters** (Unicode scalar values), `start..end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Span {
    /// First character.
    pub start: u32,
    /// One past the last character.
    pub end: u32,
}

impl Span {
    /// `start..end`.
    pub fn new(start: usize, end: usize) -> Self {
        let c = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Self {
            start: c(start),
            end: c(end),
        }
    }

    /// Smallest span covering both.
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Comparison operator of a `key<op>value` term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CmpOp {
    /// `:` (match / equals).
    Colon,
    /// `=`.
    Eq,
    /// `!=`.
    Ne,
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
}

impl CmpOp {
    /// Operator text.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Colon => ":",
            Self::Eq => "=",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }

    /// `:` or `=`.
    pub const fn is_equality(self) -> bool {
        matches!(self, Self::Colon | Self::Eq)
    }

    /// `<`, `<=`, `>`, `>=`.
    pub const fn is_ordering(self) -> bool {
        matches!(self, Self::Lt | Self::Le | Self::Gt | Self::Ge)
    }

    /// Compares `a <op> b` (`:`/`=` = equality).
    pub fn holds<T: PartialOrd>(self, a: T, b: T) -> bool {
        match self {
            Self::Colon | Self::Eq => a == b,
            Self::Ne => a != b,
            Self::Lt => a < b,
            Self::Le => a <= b,
            Self::Gt => a > b,
            Self::Ge => a >= b,
        }
    }

    /// The operator with sides swapped (`a < b` ⇔ `b > a`).
    pub const fn flipped(self) -> Self {
        match self {
            Self::Lt => Self::Gt,
            Self::Le => Self::Ge,
            Self::Gt => Self::Lt,
            Self::Ge => Self::Le,
            other => other,
        }
    }
}

/// One search term: a bare word / quoted phrase (`key = None`) or `key<op>value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// Key (`deck`, `seviye`, …) as typed; `None` for free text.
    pub key: Option<String>,
    /// Operator (`Colon` for free text).
    pub op: CmpOp,
    /// Value as typed (unescaped).
    pub value: String,
    /// The value was written in double quotes.
    pub quoted: bool,
    /// Whole term.
    pub span: Span,
    /// Key (empty span for free text).
    pub key_span: Span,
    /// Value.
    pub value_span: Span,
}

/// A query expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// All must match (juxtaposition or `AND`). Empty = match everything.
    And(Vec<Expr>),
    /// Any must match (`OR`).
    Or(Vec<Expr>),
    /// Negation (`-x`, `NOT x`).
    Not(Box<Expr>),
    /// A term.
    Term(Term),
}

impl Expr {
    /// The same tree with every span zeroed (to compare trees parsed from different text).
    pub fn without_spans(&self) -> Expr {
        match self {
            Self::And(v) => Self::And(v.iter().map(Expr::without_spans).collect()),
            Self::Or(v) => Self::Or(v.iter().map(Expr::without_spans).collect()),
            Self::Not(e) => Self::Not(Box::new(e.without_spans())),
            Self::Term(t) => Self::Term(Term {
                span: Span::default(),
                key_span: Span::default(),
                value_span: Span::default(),
                ..t.clone()
            }),
        }
    }
}

/// Writes `s` in double quotes with `"` and `\` escaped.
fn write_quoted(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    f.write_str("\"")?;
    for c in s.chars() {
        if c == '"' || c == '\\' {
            f.write_str("\\")?;
        }
        write!(f, "{c}")?;
    }
    f.write_str("\"")
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(k) = &self.key {
            write!(f, "{k}{}", self.op.as_str())?;
        }
        if self.quoted {
            write_quoted(f, &self.value)
        } else {
            f.write_str(&self.value)
        }
    }
}

/// Canonical text: parsing it yields the same tree (modulo spans). Compound children are
/// parenthesized.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let child = |f: &mut fmt::Formatter<'_>, e: &Expr| -> fmt::Result {
            match e {
                Expr::And(_) | Expr::Or(_) => write!(f, "({e})"),
                _ => write!(f, "{e}"),
            }
        };
        match self {
            Self::And(v) | Self::Or(v) => {
                let sep = if matches!(self, Self::And(_)) {
                    " "
                } else {
                    " OR "
                };
                for (i, e) in v.iter().enumerate() {
                    if i > 0 {
                        f.write_str(sep)?;
                    }
                    child(f, e)?;
                }
                Ok(())
            }
            Self::Not(e) => {
                f.write_str("-")?;
                child(f, e)
            }
            Self::Term(t) => write!(f, "{t}"),
        }
    }
}
