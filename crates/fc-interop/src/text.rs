//! Flexible delimited text (docs/02 §15.1): `term, definition`, `term: definition`, CSV, TSV,
//! Quizlet exports, custom separators (`>>`, ` - `), quoted values (`"Arda, Ali": porsuk`),
//! and separator auto-detection.
//!
//! Separators are plain strings. Two record separators are special: [`NEWLINE`] (one record
//! per line, empty lines skipped) and [`BLANK_LINE`] (records separated by one or more blank
//! lines; single newlines stay inside the value, so definitions can span lines).

use crate::error::{InteropError, InteropResult};
use serde::{Deserialize, Serialize};

/// One record per line.
pub const NEWLINE: &str = "\n";
/// Records separated by blank lines.
pub const BLANK_LINE: &str = "\n\n";
/// Field separator for columns aligned with blanks (`kedi      cat`, `cat\t\tkedi`): a run of
/// two or more spaces, or any run of spaces and tabs that holds a tab. Indentation at the
/// start of a line and blanks at its end are not separators.
pub const ALIGNED: &str = "  ";

/// How to split text into records and fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextOptions {
    /// Between fields (`,`, `:`, `\t`, ` - `, `>>` …). Never contains a newline.
    pub field_separator: String,
    /// Between records: [`NEWLINE`], [`BLANK_LINE`] or a custom string (`;`).
    pub record_separator: String,
    /// Honour `"…"` quoting (`""` inside a quoted value is a literal quote).
    pub quotes: bool,
    /// Split at most into this many fields; the rest of the record stays in the last field
    /// (Quizlet: 2, so definitions may contain the separator). `None` = unlimited.
    pub max_fields: Option<usize>,
}

impl TextOptions {
    /// Options with quoting on and no field limit.
    pub fn new(field_separator: &str, record_separator: &str) -> Self {
        Self {
            field_separator: field_separator.to_owned(),
            record_separator: record_separator.to_owned(),
            quotes: true,
            max_fields: None,
        }
    }

    /// Quizlet's default export: `term<TAB>definition`, one card per line, two columns.
    pub fn quizlet() -> Self {
        Self {
            field_separator: "\t".into(),
            record_separator: NEWLINE.into(),
            quotes: false,
            max_fields: Some(2),
        }
    }

    /// Checks the separators make sense together.
    pub fn validate(&self) -> InteropResult<()> {
        let f = &self.field_separator;
        let r = &self.record_separator;
        if f.is_empty() || r.is_empty() {
            return Err(InteropError::Options("separators must not be empty".into()));
        }
        if f.contains('\n') {
            return Err(InteropError::Options(
                "field separator cannot contain a newline".into(),
            ));
        }
        if f == r
            || (r != NEWLINE
                && r != BLANK_LINE
                && (f.contains(r.as_str()) || r.contains(f.as_str())))
        {
            return Err(InteropError::Options(
                "field and record separators overlap".into(),
            ));
        }
        if self.quotes && (f.contains('"') || r.contains('"')) {
            return Err(InteropError::Options(
                "separators cannot contain a quote while quoting is on".into(),
            ));
        }
        if self.max_fields == Some(0) {
            return Err(InteropError::Options(
                "max_fields must be at least 1".into(),
            ));
        }
        Ok(())
    }
}

/// Text decoded from a file's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The text (no BOM).
    pub text: String,
    /// Encoding used: `UTF-8`, `UTF-16LE`, `UTF-16BE` or `windows-1254`.
    pub encoding: &'static str,
}

/// Decodes an import file whatever Windows wrote it as (Turkish Excel saves CSV as
/// Windows-1254, "Unicode text" as UTF-16):
///
/// 1. a byte-order mark wins (UTF-8, UTF-16 LE/BE; the mark is removed);
/// 2. BOM-less UTF-16 is recognised by its zero bytes (mostly at odd positions = LE, even =
///    BE — true for Latin-script text);
/// 3. valid UTF-8 is UTF-8;
/// 4. anything else is Windows-1254 (a superset of Latin-1 for the printable range, so
///    Western European files decode too, except the six Turkish-specific code points).
///
/// Never fails: malformed sequences become U+FFFD.
pub fn decode(bytes: &[u8]) -> Decoded {
    use encoding_rs::{Encoding, UTF_8, WINDOWS_1254};
    if let Some((enc, bom)) = Encoding::for_bom(bytes) {
        let (text, _) = enc.decode_without_bom_handling(&bytes[bom..]);
        return Decoded {
            text: text.into_owned(),
            encoding: enc.name(),
        };
    }
    // Before the UTF-8 check: ASCII-only UTF-16 is also valid UTF-8 (full of NULs).
    let enc = match utf16_without_bom(bytes) {
        Some(e) => e,
        None => match std::str::from_utf8(bytes) {
            Ok(s) => {
                return Decoded {
                    text: s.to_owned(),
                    encoding: UTF_8.name(),
                }
            }
            Err(_) => WINDOWS_1254,
        },
    };
    let (text, _) = enc.decode_without_bom_handling(bytes);
    Decoded {
        text: text.into_owned(),
        encoding: enc.name(),
    }
}

/// UTF-16 without a BOM: an even length and at least 40% of the code units having a zero
/// high byte at the same side, with almost none on the other.
fn utf16_without_bom(bytes: &[u8]) -> Option<&'static encoding_rs::Encoding> {
    if bytes.len() < 4 || bytes.len() % 2 != 0 {
        return None;
    }
    let sample = &bytes[..bytes.len().min(64 * 1024)];
    let units = sample.len() / 2;
    let (mut even, mut odd) = (0usize, 0usize);
    for pair in sample.chunks_exact(2) {
        even += usize::from(pair[0] == 0);
        odd += usize::from(pair[1] == 0);
    }
    let many = |n: usize| n * 10 >= units * 4;
    let few = |n: usize| n * 20 <= units;
    if many(odd) && few(even) {
        Some(encoding_rs::UTF_16LE)
    } else if many(even) && few(odd) {
        Some(encoding_rs::UTF_16BE)
    } else {
        None
    }
}

/// Strips a UTF-8 BOM and turns `\r\n` / `\r` into `\n`.
pub fn normalize_newlines(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Splits `text` into records of fields. Unquoted values are trimmed; quoted values are kept
/// verbatim. Records whose fields are all empty are dropped.
pub fn parse_delimited(text: &str, opts: &TextOptions) -> InteropResult<Vec<Vec<String>>> {
    opts.validate()?;
    let text = normalize_newlines(text);
    let (_, body) = split_anki_header(&text);
    Ok(Parser::new(body, opts).run())
}

/// Header lines Anki (2.1.55+) writes at the top of a plain-text export (`#separator:tab`,
/// `#html:true`, `#tags column:3` …). Only these known keys count, so a first record that
/// happens to start with `#` stays data.
const ANKI_HEADER_KEYS: &[&str] = &[
    "separator",
    "html",
    "tags",
    "tags column",
    "deck",
    "deck column",
    "notetype",
    "notetype column",
    "guid column",
    "columns",
    "if matches",
];

/// Splits Anki's `#key:value` header lines off the top of `text`: `(header, rest)`.
fn split_anki_header(text: &str) -> (Vec<(&str, &str)>, &str) {
    let mut header = Vec::new();
    let mut rest = text;
    while let Some(line) = rest.strip_prefix('#') {
        let end = line.find('\n').unwrap_or(line.len());
        let Some((key, value)) = line[..end].split_once(':') else {
            break;
        };
        if !ANKI_HEADER_KEYS.contains(&key.trim()) {
            break;
        }
        header.push((key.trim(), value.trim()));
        rest = line.get(end + 1..).unwrap_or("");
    }
    (header, rest)
}

/// The field separator an Anki header declares (`#separator:Tab` …).
fn anki_separator(header: &[(&str, &str)]) -> Option<String> {
    let (_, v) = header.iter().find(|(k, _)| *k == "separator")?;
    let sep = match v.to_ascii_lowercase().as_str() {
        "tab" => "\t",
        "comma" => ",",
        "semicolon" => ";",
        "space" => " ",
        "pipe" => "|",
        "colon" => ":",
        _ => v,
    };
    (!sep.is_empty()).then(|| sep.to_owned())
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
    opts: &'a TextOptions,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str, opts: &'a TextOptions) -> Self {
        Self { s, i: 0, opts }
    }

    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }

    /// Length of the record separator at the cursor, if any.
    fn record_sep_len(&self) -> Option<usize> {
        let rest = self.rest();
        let sep = self.opts.record_separator.as_str();
        if sep == BLANK_LINE {
            if !rest.starts_with('\n') {
                return None;
            }
            let after = rest[1..].trim_start_matches([' ', '\t']);
            if !after.starts_with('\n') {
                return None;
            }
            let tail = rest.trim_start_matches([' ', '\t', '\n']);
            return Some(rest.len() - tail.len());
        }
        rest.starts_with(sep).then_some(sep.len())
    }

    fn field_sep_len(&self, fields_so_far: usize) -> Option<usize> {
        if let Some(max) = self.opts.max_fields {
            if fields_so_far + 1 >= max {
                return None;
            }
        }
        let sep = self.opts.field_separator.as_str();
        if sep == ALIGNED {
            return self.aligned_sep_len();
        }
        self.rest().starts_with(sep).then_some(sep.len())
    }

    /// Length of an [`ALIGNED`] separator at the cursor.
    fn aligned_sep_len(&self) -> Option<usize> {
        let rest = self.rest();
        let run = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let after = &rest[run..];
        let line_start = self.i == 0 || self.s[..self.i].ends_with('\n');
        let is_sep = (run >= 2 || rest[..run].contains('\t'))
            && !line_start
            && !after.is_empty()
            && !after.starts_with('\n');
        is_sep.then_some(run)
    }

    fn is_blank(&self, c: char) -> bool {
        c == ' ' || (c == '\t' && self.opts.field_separator != "\t")
    }

    /// Tries to read a quoted value at the cursor (after leading blanks). Returns it and moves
    /// past the closing quote; `None` (cursor unchanged) if there is no well-formed quote.
    fn quoted(&mut self) -> Option<String> {
        let start = self.i;
        let lead = self.rest().len() - self.rest().trim_start_matches(|c| self.is_blank(c)).len();
        let body = &self.s[start + lead..];
        let inner = body.strip_prefix('"')?;
        let mut out = String::new();
        let mut chars = inner.char_indices().peekable();
        while let Some((idx, c)) = chars.next() {
            if c == '"' {
                if matches!(chars.peek(), Some((_, '"'))) {
                    out.push('"');
                    chars.next();
                    continue;
                }
                self.i = start + lead + 1 + idx + 1;
                return Some(out);
            }
            out.push(c);
        }
        None
    }

    fn run(mut self) -> Vec<Vec<String>> {
        let mut records = Vec::new();
        let mut fields: Vec<String> = Vec::new();
        loop {
            // One field.
            let quoted = if self.opts.quotes {
                self.quoted()
            } else {
                None
            };
            let mut raw = String::new();
            let mut ended_record = false;
            let mut ended_text = false;
            loop {
                if self.i >= self.s.len() {
                    ended_text = true;
                    break;
                }
                if let Some(n) = self.record_sep_len() {
                    self.i += n;
                    ended_record = true;
                    break;
                }
                if let Some(n) = self.field_sep_len(fields.len()) {
                    self.i += n;
                    break;
                }
                let c = self.rest().chars().next().unwrap_or_default();
                raw.push(c);
                self.i += c.len_utf8().max(1);
            }
            let value = match quoted {
                Some(q) => {
                    let tail = raw.trim();
                    if tail.is_empty() {
                        q
                    } else {
                        format!("{q}{tail}")
                    }
                }
                None => raw.trim().to_owned(),
            };
            fields.push(value);
            if ended_record || ended_text {
                if fields.iter().any(|f| !f.is_empty()) {
                    records.push(std::mem::take(&mut fields));
                } else {
                    fields.clear();
                }
            }
            if ended_text {
                break;
            }
        }
        records
    }
}

/// Writes records with `opts` separators, quoting values that would not read back verbatim.
/// Ends with a newline.
pub fn write_delimited(rows: &[Vec<String>], opts: &TextOptions) -> String {
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push_str(&opts.record_separator);
            if opts.record_separator != NEWLINE && opts.record_separator != BLANK_LINE {
                out.push('\n');
            }
        }
        for (j, value) in row.iter().enumerate() {
            if j > 0 {
                out.push_str(&opts.field_separator);
            }
            out.push_str(&quote_if_needed(value, opts));
        }
    }
    out.push('\n');
    out
}

fn quote_if_needed(value: &str, opts: &TextOptions) -> String {
    let record_hit = if opts.record_separator == BLANK_LINE {
        value.contains("\n\n")
    } else {
        value.contains(opts.record_separator.as_str())
    };
    let needs = value.contains(opts.field_separator.as_str())
        || record_hit
        || value.starts_with('"')
        || value.contains('\n') && opts.record_separator == NEWLINE
        || value.trim() != value;
    if needs {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

// ------------------------------------------------------------------ detection

/// Result of [`detect`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    /// Suggested options.
    pub options: TextOptions,
    /// Most common number of fields per record.
    pub columns: usize,
    /// Records found with the suggestion.
    pub records: usize,
    /// 0.0–1.0; low values mean "please check the separators".
    pub confidence: f32,
}

/// Field separator candidates and their prior weights (more specific first). Separators are
/// trimmed around values, so ` → ` is covered by `→`.
const FIELD_CANDIDATES: &[(&str, f64)] = &[
    ("\t", 1.0),
    ("::", 0.97),
    (" - ", 0.97),
    (" – ", 0.97),
    (" — ", 0.96),
    (" = ", 0.96),
    (" => ", 0.96),
    ("→", 0.96),
    ("=>", 0.95),
    ("==", 0.95),
    ("->", 0.95),
    (">>", 0.95),
    (":", 0.93),
    ("~", 0.92),
    ("=", 0.92),
    ("|", 0.91),
    (",", 0.9),
    (";", 0.88),
    (" / ", 0.88),
    ("-", 0.75),
    (ALIGNED, 0.7),
];

/// Two-column separators: a record with more fields is unlikely to be meant this way.
const PAIR_SEPARATORS: &[&str] = &[
    "::", " - ", " – ", " — ", " = ", " => ", "→", "=>", "->", ">>", "==", ":", "~", "=", "-",
];

/// Score factor of a separator when every line also holds a stronger one.
const OUTRANKED: f64 = 0.9;

/// Sentence punctuation ending a line of prose.
const SENTENCE_END: &[char] = &['.', '!', '?', '…'];

/// A record share above which a pattern counts as "every line".
const MOST_RECORDS: f64 = 0.75;

const RECORD_CANDIDATES: &[(&str, f64)] = &[(NEWLINE, 1.0), (BLANK_LINE, 0.97), (";", 0.9)];

/// How much text detection looks at.
const SAMPLE_BYTES: usize = 64 * 1024;

/// A candidate whose splits cut through protected tokens (`3,14`, `10:30`, `https://…`) is
/// worth this much of its consistency score.
const SPLITS_TOKENS: f64 = 0.5;

/// A separator missing from more than this share of the records is not the separator (a list
/// of sentences, some with a comma, is one column).
const MAX_RECORDS_WITHOUT_SEPARATOR: f64 = 0.25;

/// Suggests separators for `text` (docs/02 §15.1 "otomatik algılama").
///
/// Every (record, field) separator pair is parsed and scored by how consistently it yields
/// the same number (≥ 2) of fields per record, weighted by how specific the separator is.
/// Values that merely *contain* separator characters are recognised first: decimal numbers
/// (`3,14`), times (`10:30`), number ranges (`1919-1922`) and URLs are masked, so a candidate
/// is judged by the columns it gives outside those tokens. A candidate that would cut through
/// them scores half; when the clean split is reachable by limiting the field count
/// (`Toplantı: 10:30` → `max_fields: 2`), that limit is suggested.
pub fn detect(text: &str) -> Detection {
    let text = normalize_newlines(text);
    let (header, body) = split_anki_header(&text);
    let sample = sample(body);
    if let Some(fs) = anki_separator(&header) {
        // Anki said which separator it used (fields with newlines are quoted).
        let options = TextOptions::new(&fs, NEWLINE);
        if options.validate().is_ok() {
            let records = Parser::new(sample, &options).run();
            let columns = score(&records, &fs).map_or(1, |(_, c)| c);
            return Detection {
                options,
                columns,
                records: records.len(),
                confidence: 1.0,
            };
        }
    }
    let masked = mask_tokens(sample);
    let mut best: Option<(f64, TextOptions, usize, usize)> = None;
    for (rs, rw) in RECORD_CANDIDATES {
        for (fs, fw) in FIELD_CANDIDATES {
            let opts = TextOptions::new(fs, rs);
            if opts.validate().is_err() {
                continue;
            }
            let Some((score, opts, columns, records)) = candidate(sample, &masked, opts) else {
                continue;
            };
            let score = score * fw * rw;
            if best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((score, opts, columns, records));
            }
        }
    }
    match best {
        Some((score, options, columns, records)) => Detection {
            options,
            columns,
            records,
            confidence: score.clamp(0.0, 1.0) as f32,
        },
        None => {
            // One column: a paragraph per record when the text is prose split by blank lines.
            let rs = if paragraphs(sample) {
                BLANK_LINE
            } else {
                NEWLINE
            };
            let options = TextOptions::new("\t", rs);
            let records = Parser::new(sample, &options).run().len();
            Detection {
                options,
                columns: 1,
                records,
                confidence: 0.0,
            }
        }
    }
}

/// Scores one separator pair: `(score, options, columns, records)`.
fn candidate(
    sample: &str,
    masked: &str,
    opts: TextOptions,
) -> Option<(f64, TextOptions, usize, usize)> {
    let fs = opts.field_separator.clone();
    let rs = opts.record_separator.clone();
    let real = Parser::new(sample, &opts).run();
    if rs != NEWLINE && real.len() < 2 {
        // A record separator that does not separate anything (no blank line, no `;`).
        return None;
    }
    if fs != ALIGNED && !without_cloze(sample).contains(fs.as_str()) {
        // Only inside `{{c1::…}}`: a cloze list is one column, even for want of anything else.
        return None;
    }
    if rs == BLANK_LINE && ends_lines(sample, &fs) {
        // `Roses are red,⏎violets are blue.`: punctuation at a line end, not a separator.
        return None;
    }
    let clean = unmask_records(Parser::new(masked, &opts).run());
    let adjust = |records: &[Vec<String>], s: f64| {
        if rs == NEWLINE && trailing_custom_separator(records) {
            // `a,b;` lines: the `;` is a record separator followed by a line break.
            s * 0.5
        } else {
            s
        }
    };
    if real == clean {
        let (s, columns) = score(&real, &fs)?;
        if columns == 2 && repeats_in_definition(&fs) {
            // `Sympathy - feeling sorry - not empathy`: a term/definition separator repeated
            // inside the definition. Keep the rest of the record in the second field.
            let limited = TextOptions {
                max_fields: Some(2),
                ..opts.clone()
            };
            let two = Parser::new(sample, &limited).run();
            if two != real {
                let (s, _) = score(&two, &fs)?;
                return Some((adjust(&two, s), limited, 2, two.len()));
            }
        }
        return Some((adjust(&real, s), opts, columns, real.len()));
    }
    // The candidate cuts through protected tokens. Best: the clean split, if limiting the
    // field count reproduces it (protected tokens only in the last column).
    if let Some((s, columns)) = score(&clean, &fs) {
        let limited = TextOptions {
            max_fields: Some(columns),
            ..opts.clone()
        };
        let split = Parser::new(sample, &limited).run();
        if split == clean {
            return Some((adjust(&clean, s), limited, columns, clean.len()));
        }
        if columns == 2 && repeats_in_definition(&fs) {
            // Both at once: `Saat: 13:15` and `Not: Kitap getir: sayfa 42`.
            let clean = unmask_records(Parser::new(masked, &limited).run());
            if split == clean {
                let (s, _) = score(&clean, &fs)?;
                return Some((adjust(&clean, s), limited, 2, clean.len()));
            }
        }
    }
    // Otherwise the plain split, for want of anything better ("1,5\n2,5" is still a CSV).
    let (s, columns) = score(&real, &fs)?;
    Some((adjust(&real, s) * SPLITS_TOKENS, opts, columns, real.len()))
}

fn sample(text: &str) -> &str {
    if text.len() <= SAMPLE_BYTES {
        return text;
    }
    let mut end = SAMPLE_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let cut = &text[..end];
    match cut.rfind('\n') {
        Some(n) if n > 0 => &cut[..n],
        _ => cut,
    }
}

/// Stand-ins for separator characters inside protected tokens (control characters never
/// found in text; one byte each, so offsets do not move).
const MASKS: &[(char, char)] = &[
    (',', '\u{1}'),
    (':', '\u{2}'),
    ('-', '\u{3}'),
    ('=', '\u{4}'),
    (';', '\u{5}'),
];

fn mask_char(c: char) -> char {
    MASKS.iter().find(|(a, _)| *a == c).map_or(c, |(_, m)| *m)
}

fn unmask_char(c: char) -> char {
    MASKS.iter().find(|(_, m)| *m == c).map_or(c, |(a, _)| *a)
}

fn unmask_records(records: Vec<Vec<String>>) -> Vec<Vec<String>> {
    records
        .into_iter()
        .map(|r| {
            r.into_iter()
                .map(|f| {
                    if f.contains(|c| MASKS.iter().any(|(_, m)| *m == c)) {
                        f.chars().map(unmask_char).collect()
                    } else {
                        f
                    }
                })
                .collect()
        })
        .collect()
}

/// `text` with the separator characters inside protected tokens replaced by [`MASKS`]:
///
/// * numbers: `,` `:` `-` between digits (`3,14`, `1.000,50`, `10:30`, `1919-1922`) — not
///   when the `,` is followed by a space (`1, 2` lists separate);
/// * URLs: `scheme://…` up to whitespace, a quote or a `,` `;` `|` `\t` boundary (their `:`,
///   `-`, `=` stay inside);
/// * cloze deletions: `{{c1::Ankara}}`.
fn mask_tokens(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = chars.clone();
    let digit = |i: usize| chars.get(i).is_some_and(|c| c.is_ascii_digit());
    for i in 1..chars.len() {
        if matches!(chars[i], ',' | ':' | '-') && digit(i - 1) && digit(i + 1) {
            out[i] = mask_char(chars[i]);
        }
    }
    // Cloze deletions: `{{c1::Ankara}}` (and `{{c1::Ankara::capital}}`) are one token.
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '{' && chars[i + 1] == '{' {
            if let Some(len) = chars[i..].windows(2).position(|w| w == ['}', '}']) {
                for k in i..i + len {
                    out[k] = mask_char(chars[k]);
                }
                i += len + 2;
                continue;
            }
        }
        i += 1;
    }
    let mut i = 0;
    while i + 3 <= chars.len() {
        if chars[i] == ':' && chars.get(i + 1) == Some(&'/') && chars.get(i + 2) == Some(&'/') {
            let mut start = i;
            while start > 0 && chars[start - 1].is_ascii_alphanumeric() {
                start -= 1;
            }
            if start < i && chars[start].is_ascii_alphabetic() {
                let mut end = i;
                while end < chars.len()
                    && !chars[end].is_whitespace()
                    && !matches!(chars[end], '"' | ',' | ';' | '|')
                {
                    out[end] = mask_char(chars[end]);
                    end += 1;
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// `(score, columns)` of one candidate, `None` if it does not split records at all.
fn score(records: &[Vec<String>], fs: &str) -> Option<(f64, usize)> {
    let n = records.len();
    if n == 0 {
        return None;
    }
    let mut counts: Vec<(usize, usize)> = Vec::new();
    for r in records {
        match counts.iter_mut().find(|(c, _)| *c == r.len()) {
            Some((_, k)) => *k += 1,
            None => counts.push((r.len(), 1)),
        }
    }
    let (mode, hits) = counts
        .iter()
        .copied()
        .max_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)))?;
    if mode < 2 {
        return None;
    }
    let without = records.iter().filter(|r| r.len() == 1).count();
    if without as f64 > MAX_RECORDS_WITHOUT_SEPARATOR * n as f64 {
        return None;
    }
    if fs == "-" && records.iter().any(|r| r.len() > 2) {
        // `up-to-date`, `mother-in-law`: a bare hyphen chained inside words joins them.
        return None;
    }
    if fs == "," && sentence_lines(records) {
        // `Evet, geliyorum.` on every line: prose with a comma, not `word, meaning`.
        return None;
    }
    // Lines that hold a stronger separator too: `das Kind, die Kinder; çocuk` (a `;` beats
    // the commas of a list), `Ohm's law: V = IR` (a label colon beats a formula's `=`).
    let weaker_than = |other: &str| {
        let with = records
            .iter()
            .filter(|r| r.iter().any(|f| f.contains(other)))
            .count();
        with as f64 >= MOST_RECORDS * n as f64
    };
    let outranked = match fs {
        "," => weaker_than(";"),
        " = " | "=" | "==" => weaker_than(":"),
        _ => false,
    };
    let share = hits as f64 / n as f64;
    let consistent: Vec<&Vec<String>> = records.iter().filter(|r| r.len() == mode).collect();
    let fields = (consistent.len() * mode).max(1);
    let empty = consistent
        .iter()
        .flat_map(|r| r.iter())
        .filter(|f| f.is_empty())
        .count() as f64
        / fields as f64;
    let mut s = share * (n as f64 / (n as f64 + 1.0)) * (1.0 - 0.5 * empty);
    if outranked {
        s *= OUTRANKED;
    }
    if PAIR_SEPARATORS.contains(&fs) && mode > 2 {
        s *= 0.6;
    }
    Some((s, mode))
}

/// A term/definition separator the definition may repeat (`Sympathy - feeling sorry - not
/// empathy`). Not a bare `-`, which also joins words.
fn repeats_in_definition(fs: &str) -> bool {
    fs != "-" && PAIR_SEPARATORS.contains(&fs)
}

/// `text` without its cloze deletions (`{{c1::…}}`).
fn without_cloze(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start..].find("}}") else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push(' ');
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

/// Whether most occurrences of `fs` end a line.
fn ends_lines(sample: &str, fs: &str) -> bool {
    let total = sample.matches(fs).count();
    let at_end = sample
        .match_indices(fs)
        .filter(|(i, _)| {
            sample[i + fs.len()..]
                .trim_start_matches([' ', '\t'])
                .starts_with('\n')
        })
        .count();
    total > 0 && at_end * 2 > total
}

/// Whether `sample` is prose in paragraphs: at least two blocks between blank lines, each of
/// several lines, and most line breaks inside a block continue a sentence (the line before
/// ends with `,` `;` or `:`). A word list grouped by blank lines stays one word per record.
fn paragraphs(sample: &str) -> bool {
    let opts = TextOptions::new("\t", BLANK_LINE);
    let blocks: Vec<String> = Parser::new(sample, &opts)
        .run()
        .into_iter()
        .filter_map(|r| r.into_iter().next())
        .collect();
    if blocks.len() < 2 || blocks.iter().any(|b| !b.contains('\n')) {
        return false;
    }
    let (mut breaks, mut continued) = (0usize, 0usize);
    for b in &blocks {
        let lines: Vec<&str> = b.lines().collect();
        for line in &lines[..lines.len() - 1] {
            breaks += 1;
            continued += usize::from(line.trim_end().ends_with([',', ';', ':']));
        }
    }
    continued as f64 >= MOST_RECORDS * breaks as f64
}

/// Whether most records read as one sentence each: a capital letter first, sentence
/// punctuation last, and a lower-case word right after the first separator (`Evet,
/// geliyorum.`). A `word, meaning` pair starts its second value fresh and has no full stop.
fn sentence_lines(records: &[Vec<String>]) -> bool {
    let prose = records
        .iter()
        .filter(|r| {
            let (Some(first), Some(last)) = (r.first(), r.last()) else {
                return false;
            };
            r.len() >= 2
                && first.chars().next().is_some_and(char::is_uppercase)
                && last
                    .trim_end_matches(['"', '\'', ')'])
                    .ends_with(SENTENCE_END)
                && r[1].chars().next().is_some_and(char::is_lowercase)
        })
        .count();
    prose as f64 > MOST_RECORDS * records.len() as f64
}

/// Whether most records (all but the last) end with `;`, i.e. `;` separates records.
fn trailing_custom_separator(records: &[Vec<String>]) -> bool {
    if records.len() < 2 {
        return false;
    }
    let head = &records[..records.len() - 1];
    let hits = head
        .iter()
        .filter(|r| r.last().is_some_and(|f| f.ends_with(';')))
        .count();
    hits * 2 > head.len()
}

#[cfg(test)]
mod encoding_tests;
#[cfg(test)]
mod heldout2_tests;
#[cfg(test)]
mod heldout3_samples;
#[cfg(test)]
mod heldout3_tests;
#[cfg(test)]
mod heldout4_samples;
#[cfg(test)]
mod heldout4_tests;
#[cfg(test)]
mod heldout_tests;
#[cfg(test)]
mod tests;
