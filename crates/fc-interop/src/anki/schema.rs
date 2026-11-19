//! Anki collection schema (the parts packages use).

/// Schema 11 (legacy): what [`write_package`](super::write_package) produces and what legacy
/// packages contain. In schema 18 the same tables exist; note types, decks and configs move
/// to their own tables ([`SCHEMA18_TABLES`]).
pub const SCHEMA11: &str = "
CREATE TABLE col (
    id integer primary key, crt integer not null, mod integer not null, scm integer not null,
    ver integer not null, dty integer not null, usn integer not null, ls integer not null,
    conf text not null, models text not null, decks text not null, dconf text not null,
    tags text not null
);
CREATE TABLE notes (
    id integer primary key, guid text not null, mid integer not null, mod integer not null,
    usn integer not null, tags text not null, flds text not null, sfld integer not null,
    csum integer not null, flags integer not null, data text not null
);
CREATE TABLE cards (
    id integer primary key, nid integer not null, did integer not null, ord integer not null,
    mod integer not null, usn integer not null, type integer not null, queue integer not null,
    due integer not null, ivl integer not null, factor integer not null, reps integer not null,
    lapses integer not null, left integer not null, odue integer not null,
    odid integer not null, flags integer not null, data text not null
);
CREATE TABLE revlog (
    id integer primary key, cid integer not null, usn integer not null, ease integer not null,
    ivl integer not null, lastIvl integer not null, factor integer not null,
    time integer not null, type integer not null
);
CREATE TABLE graves (usn integer not null, oid integer not null, type integer not null);
CREATE INDEX ix_notes_usn on notes (usn);
CREATE INDEX ix_cards_usn on cards (usn);
CREATE INDEX ix_revlog_usn on revlog (usn);
CREATE INDEX ix_cards_nid on cards (nid);
CREATE INDEX ix_cards_sched on cards (did, queue, due);
CREATE INDEX ix_revlog_cid on revlog (cid);
CREATE INDEX ix_notes_csum on notes (csum);
";

/// Schema 18 tables that replace the JSON columns of `col`.
pub const SCHEMA18_TABLES: &str = "
CREATE TABLE notetypes (
    id integer not null primary key, name text not null, mtime_secs integer not null,
    usn integer not null, config blob not null
);
CREATE TABLE fields (
    ntid integer not null, ord integer not null, name text not null, config blob not null,
    primary key (ntid, ord)
) without rowid;
CREATE TABLE templates (
    ntid integer not null, ord integer not null, name text not null,
    mtime_secs integer not null, usn integer not null, config blob not null,
    primary key (ntid, ord)
) without rowid;
CREATE TABLE decks (
    id integer primary key not null, name text not null, mtime_secs integer not null,
    usn integer not null, common blob not null, kind blob not null
);
CREATE TABLE deck_config (
    id integer primary key not null, name text not null, mtime_secs integer not null,
    usn integer not null, config blob not null
);
CREATE TABLE config (
    KEY text not null primary key, usn integer not null, mtime_secs integer not null,
    val blob not null
) without rowid;
CREATE TABLE tags (
    tag text not null primary key, usn integer not null, collapsed boolean not null,
    config blob null
) without rowid;
";

/// Separator of deck name components in schema 18 (`::` in schema 11).
pub const DECK_SEPARATOR_18: char = '\x1f';
/// Separator of field values in `notes.flds`.
pub const FIELD_SEPARATOR: char = '\x1f';
