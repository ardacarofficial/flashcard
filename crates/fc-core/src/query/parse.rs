//! Lexer and recursive-descent parser: text → [`Expr`].
//!
//! ```text
//! query   := or
//! or      := and ( "OR" and )*
//! and     := unary ( ["AND"] unary )*          -- juxtaposition = AND
//! unary   := "-" unary | "NOT" unary | "(" or ")" | term
//! term    := "quoted phrase" | word | key OP value
//! OP      := ":" | "=" | "!=" | "<" | "<=" | ">" | ">="
//! value   := word | "quoted"
//! ```
//!
//! * A word runs until whitespace, `(`, `)` or `"`. A key is a run of letters, digits and `_`
//!   followed by an operator: `deck:"Dil::İngilizce"`, `seviye>=B1`, `kelime:app*`.
//! * `OR`/`AND`/`NOT` are keywords in upper or lower case (quote them to search the word).
//! * Inside quotes `\"` and `\\` escape.

use super::ast::{CmpOp, Expr, Span, Term};
use super::error::{QueryError, QueryErrorKind};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    LParen,
    RParen,
    Neg,
    Or,
    And,
    Not,
    Term(Term),
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn ends_word(c: char) -> bool {
    c.is_whitespace() || c == '(' || c == ')' || c == '"'
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Reads a quoted string starting at `self.pos` (on the opening quote).
    fn quoted(&mut self) -> Result<(String, Span), QueryError> {
        let start = self.pos;
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(QueryError::new(
                        QueryErrorKind::UnclosedQuote,
                        Span::new(start, self.chars.len()),
                    ))
                }
                Some('\\') => {
                    self.pos += 1;
                    if let Some(c) = self.peek() {
                        out.push(c);
                        self.pos += 1;
                    }
                }
                Some('"') => {
                    self.pos += 1;
                    return Ok((out, Span::new(start, self.pos)));
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn next(&mut self) -> Result<Option<(Tok, Span)>, QueryError> {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        match c {
            '(' => {
                self.pos += 1;
                return Ok(Some((Tok::LParen, Span::new(start, start + 1))));
            }
            ')' => {
                self.pos += 1;
                return Ok(Some((Tok::RParen, Span::new(start, start + 1))));
            }
            '-' => {
                self.pos += 1;
                if self
                    .chars
                    .get(start + 1)
                    .is_none_or(|n| n.is_whitespace() || *n == ')')
                {
                    return Err(QueryError::new(
                        QueryErrorKind::ExpectedTerm,
                        Span::new(start, start + 1),
                    ));
                }
                return Ok(Some((Tok::Neg, Span::new(start, start + 1))));
            }
            '"' => {
                let (value, span) = self.quoted()?;
                return Ok(Some((
                    Tok::Term(Term {
                        key: None,
                        op: CmpOp::Colon,
                        value,
                        quoted: true,
                        span,
                        key_span: Span::new(start, start),
                        value_span: span,
                    }),
                    span,
                )));
            }
            _ => {}
        }
        while self.peek().is_some_and(|c| !ends_word(c)) {
            self.pos += 1;
        }
        let word: Vec<char> = self.chars[start..self.pos].to_vec();
        let text: String = word.iter().collect();
        let kw = match text.as_str() {
            "OR" | "or" => Some(Tok::Or),
            "AND" | "and" => Some(Tok::And),
            "NOT" | "not" => Some(Tok::Not),
            _ => None,
        };
        if let Some(kw) = kw {
            return Ok(Some((kw, Span::new(start, self.pos))));
        }
        // key<op>value?
        let key_len = word.iter().take_while(|c| is_key_char(**c)).count();
        let op = if key_len > 0 {
            let rest = &word[key_len..];
            match (rest.first(), rest.get(1)) {
                (Some(':'), _) => Some((CmpOp::Colon, 1)),
                (Some('!'), Some('=')) => Some((CmpOp::Ne, 2)),
                (Some('<'), Some('=')) => Some((CmpOp::Le, 2)),
                (Some('>'), Some('=')) => Some((CmpOp::Ge, 2)),
                (Some('='), _) => Some((CmpOp::Eq, 1)),
                (Some('<'), _) => Some((CmpOp::Lt, 1)),
                (Some('>'), _) => Some((CmpOp::Gt, 1)),
                _ => None,
            }
        } else {
            None
        };
        let Some((op, op_len)) = op else {
            let span = Span::new(start, self.pos);
            return Ok(Some((
                Tok::Term(Term {
                    key: None,
                    op: CmpOp::Colon,
                    value: text,
                    quoted: false,
                    span,
                    key_span: Span::new(start, start),
                    value_span: span,
                }),
                span,
            )));
        };
        let key: String = word[..key_len].iter().collect();
        let key_span = Span::new(start, start + key_len);
        let value_start = start + key_len + op_len;
        let (value, value_span, quoted) = if value_start == self.pos {
            if self.peek() == Some('"') {
                let (v, s) = self.quoted()?;
                (v, s, true)
            } else {
                return Err(QueryError::new(
                    QueryErrorKind::EmptyValue { key },
                    Span::new(start, self.pos),
                ));
            }
        } else {
            (
                word[key_len + op_len..].iter().collect(),
                Span::new(value_start, self.pos),
                false,
            )
        };
        let span = Span::new(start, self.pos);
        Ok(Some((
            Tok::Term(Term {
                key: Some(key),
                op,
                value,
                quoted,
                span,
                key_span,
                value_span,
            }),
            span,
        )))
    }
}

/// Deepest nesting of parentheses and negations (`-`, `NOT`) a query may have. Parsing,
/// resolving, evaluating and compiling to SQL recurse once per level: without a bound a pasted
/// run of `(` overflows the stack (v1.4.1: 1000 levels did on a 2 MB thread) and SQLite refuses
/// expressions deeper than 1000.
pub const MAX_DEPTH: usize = 50;

struct Parser {
    toks: Vec<(Tok, Span)>,
    pos: usize,
    len: usize,
    /// Current nesting ([`MAX_DEPTH`]).
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn span_here(&self) -> Span {
        self.toks
            .get(self.pos)
            .map_or(Span::new(self.len, self.len), |(_, s)| *s)
    }

    fn or(&mut self) -> Result<Expr, QueryError> {
        let mut items = vec![self.and()?];
        while self.peek() == Some(&Tok::Or) {
            self.pos += 1;
            items.push(self.and()?);
        }
        Ok(if items.len() == 1 {
            items.remove(0)
        } else {
            Expr::Or(items)
        })
    }

    fn starts_unary(t: Option<&Tok>) -> bool {
        matches!(
            t,
            Some(Tok::Neg | Tok::Not | Tok::LParen | Tok::Term(_) | Tok::And)
        )
    }

    fn and(&mut self) -> Result<Expr, QueryError> {
        let mut items = vec![self.unary()?];
        while Self::starts_unary(self.peek()) {
            if self.peek() == Some(&Tok::And) {
                self.pos += 1;
            }
            items.push(self.unary()?);
        }
        Ok(if items.len() == 1 {
            items.remove(0)
        } else {
            Expr::And(items)
        })
    }

    fn unary(&mut self) -> Result<Expr, QueryError> {
        let here = self.span_here();
        let Some((tok, span)) = self.toks.get(self.pos).cloned() else {
            return Err(QueryError::new(QueryErrorKind::ExpectedTerm, here));
        };
        if matches!(tok, Tok::Neg | Tok::Not | Tok::LParen) {
            if self.depth >= MAX_DEPTH {
                return Err(QueryError::new(QueryErrorKind::TooDeep, span));
            }
            self.depth += 1;
        }
        match tok {
            Tok::Neg | Tok::Not => {
                self.pos += 1;
                let inner = self.unary()?;
                self.depth -= 1;
                Ok(Expr::Not(Box::new(inner)))
            }
            Tok::LParen => {
                self.pos += 1;
                let inner = self.or()?;
                self.depth -= 1;
                match self.peek() {
                    Some(Tok::RParen) => {
                        self.pos += 1;
                        Ok(inner)
                    }
                    _ => Err(QueryError::new(QueryErrorKind::UnclosedParen, span)),
                }
            }
            Tok::Term(t) => {
                self.pos += 1;
                Ok(Expr::Term(t))
            }
            Tok::RParen | Tok::Or | Tok::And => {
                Err(QueryError::new(QueryErrorKind::ExpectedTerm, span))
            }
        }
    }
}

/// Parses a query. Empty (or whitespace-only) text is `Expr::And([])` = everything.
pub fn parse_query(text: &str) -> Result<Expr, QueryError> {
    let mut lexer = Lexer {
        chars: text.chars().collect(),
        pos: 0,
    };
    let mut toks = Vec::new();
    while let Some(t) = lexer.next()? {
        toks.push(t);
    }
    if toks.is_empty() {
        return Ok(Expr::And(Vec::new()));
    }
    let mut p = Parser {
        toks,
        pos: 0,
        len: lexer.chars.len(),
        depth: 0,
    };
    let expr = p.or()?;
    if let Some((tok, span)) = p.toks.get(p.pos) {
        let kind = if *tok == Tok::RParen {
            QueryErrorKind::UnmatchedParen
        } else {
            QueryErrorKind::ExpectedTerm
        };
        return Err(QueryError::new(kind, *span));
    }
    Ok(expr)
}
