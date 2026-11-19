//! Decks and presets.

use super::{
    all, count, exec, exec_one, id, json, json_list, one, opt_id, opt_ms, opt_ts, to_json, ts,
};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{CardCounts, DeckRead, DeckTrashSet, DeckWrite, PresetRead, PresetWrite};
use fc_core::deck::{name_eq, Deck};
use fc_core::preset::Preset;
use fc_core::{CardId, DeckId, NoteId, PresetId, Timestamp};
use rusqlite::{params, Connection, Row};

const T: &str = "decks";
const COLS: &str = "id, parent_id, name, description, icon, color, preset_id, position, \
                    created_at, updated_at, deleted_at";

/// Upper bound on hierarchy depth when walking ancestors (cycle guard).
const MAX_DEPTH: i64 = 10_000;

fn deck_row(r: &Row<'_>) -> rusqlite::Result<Deck> {
    Ok(Deck {
        id: id(r, 0)?,
        parent_id: opt_id(r, 1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        icon: r.get(4)?,
        color: r.get(5)?,
        preset_id: opt_id(r, 6)?,
        position: r.get(7)?,
        created_at: ts(r, 8)?,
        updated_at: ts(r, 9)?,
        deleted_at: opt_ts(r, 10)?,
    })
}

impl<M: Mode> DeckRead for SqliteTx<'_, M> {
    fn get_deck(&self, deck: DeckId) -> StorageResult<Option<Deck>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM decks WHERE id = ?1"),
            [deck.to_string()],
            deck_row,
        )
    }

    fn list_decks(&self, include_deleted: bool) -> StorageResult<Vec<Deck>> {
        all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS} FROM decks WHERE ?1 OR deleted_at IS NULL \
                 ORDER BY position, name COLLATE NOCASE, id"
            ),
            [include_deleted],
            deck_row,
        )
    }

    fn find_child_deck(&self, parent: Option<DeckId>, name: &str) -> StorageResult<Option<Deck>> {
        let children = all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS} FROM decks WHERE parent_id IS ?1 AND deleted_at IS NULL ORDER BY id"
            ),
            [parent.map(|p| p.to_string())],
            deck_row,
        )?;
        Ok(children.into_iter().find(|d| name_eq(&d.name, name)))
    }

    fn card_counts_by_deck(&self, now: Timestamp) -> StorageResult<Vec<(DeckId, CardCounts)>> {
        let sql = "SELECT c.deck_id,
                coalesce(sum(c.state = 'new' AND c.suspended = 0
                      AND (c.buried_until IS NULL OR c.buried_until <= ?1)), 0),
                coalesce(sum(c.state IN ('learning', 'relearning') AND c.due <= ?1
                      AND c.suspended = 0
                      AND (c.buried_until IS NULL OR c.buried_until <= ?1)), 0),
                coalesce(sum(c.state = 'review' AND c.due <= ?1 AND c.suspended = 0
                      AND (c.buried_until IS NULL OR c.buried_until <= ?1)), 0),
                count(*)
            FROM cards c JOIN notes n ON n.id = c.note_id
            WHERE c.deleted_at IS NULL AND n.deleted_at IS NULL
            GROUP BY c.deck_id ORDER BY c.deck_id";
        all(self.conn(), "cards", sql, [now.0], |r| {
            Ok((
                id(r, 0)?,
                CardCounts {
                    new: r.get(1)?,
                    learning: r.get(2)?,
                    review_due: r.get(3)?,
                    total: r.get(4)?,
                },
            ))
        })
    }

    fn deck_ancestry(&self, deck: DeckId) -> StorageResult<Vec<DeckId>> {
        let rows: Vec<(DeckId, i64)> = all(
            self.conn(),
            T,
            "WITH RECURSIVE anc(id, parent_id, depth) AS (
                 SELECT id, parent_id, 0 FROM decks WHERE id = ?1
                 UNION ALL
                 SELECT d.id, d.parent_id, anc.depth + 1
                 FROM decks d JOIN anc ON d.id = anc.parent_id
                 WHERE anc.depth < ?2
             )
             SELECT id, depth FROM anc ORDER BY depth DESC",
            params![deck.to_string(), MAX_DEPTH],
            |r| Ok((id(r, 0)?, r.get(1)?)),
        )?;
        if rows.is_empty() {
            return Err(StorageError::NotFound {
                entity: "deck",
                id: deck.to_string(),
            });
        }
        let chain: Vec<DeckId> = rows.into_iter().map(|(d, _)| d).collect();
        let mut seen = std::collections::HashSet::new();
        if !chain.iter().all(|d| seen.insert(*d)) {
            return Err(StorageError::Corrupt {
                table: T,
                message: format!("parent cycle above deck {deck}"),
            });
        }
        Ok(chain)
    }
}

impl DeckWrite for SqliteTx<'_, WriteMode> {
    fn insert_deck(&mut self, d: &Deck) -> StorageResult<()> {
        exec(
            self.conn(),
            T,
            &format!(
                "INSERT INTO decks ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
            ),
            params![
                d.id.to_string(),
                d.parent_id.map(|p| p.to_string()),
                d.name,
                d.description,
                d.icon,
                d.color,
                d.preset_id.map(|p| p.to_string()),
                d.position,
                d.created_at.0,
                d.updated_at.0,
                opt_ms(d.deleted_at),
            ],
        )?;
        Ok(())
    }

    fn update_deck(&mut self, d: &Deck) -> StorageResult<()> {
        exec_one(
            self.conn(),
            T,
            "deck",
            &d.id,
            "UPDATE decks SET parent_id = ?2, name = ?3, description = ?4, icon = ?5, color = ?6,
                 preset_id = ?7, position = ?8, created_at = ?9, updated_at = ?10,
                 deleted_at = ?11
             WHERE id = ?1",
            params![
                d.id.to_string(),
                d.parent_id.map(|p| p.to_string()),
                d.name,
                d.description,
                d.icon,
                d.color,
                d.preset_id.map(|p| p.to_string()),
                d.position,
                d.created_at.0,
                d.updated_at.0,
                opt_ms(d.deleted_at),
            ],
        )
    }

    fn set_decks_deleted(
        &mut self,
        ids: &[DeckId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        set_deleted(self.conn(), "decks", ids, deleted_at)
    }

    fn trash_deck_subtree(&mut self, deck: DeckId, at: Timestamp) -> StorageResult<DeckTrashSet> {
        let conn = self.conn();
        if deck_deleted_at(conn, deck)?.is_some() {
            return Ok(DeckTrashSet::default());
        }
        // Live sub-decks, walking through live decks only.
        let decks: Vec<DeckId> = all(
            conn,
            T,
            "WITH RECURSIVE sub(id, depth) AS (
                 SELECT id, 0 FROM decks WHERE id = ?1
                 UNION
                 SELECT d.id, sub.depth + 1 FROM decks d JOIN sub ON d.parent_id = sub.id
                 WHERE d.deleted_at IS NULL AND sub.depth < ?2
             )
             SELECT id FROM sub ORDER BY id",
            params![deck.to_string(), MAX_DEPTH],
            |r| id(r, 0),
        )?;
        let deck_list = json_list(&decks);
        let cards: Vec<CardId> = all(
            conn,
            "cards",
            "SELECT id FROM cards
             WHERE deleted_at IS NULL AND deck_id IN (SELECT value FROM json_each(?1))
             ORDER BY id",
            [&deck_list],
            |r| id(r, 0),
        )?;
        let card_list = json_list(&cards);
        // Live notes that lose their last live card, plus live card-less notes homed here.
        let notes: Vec<NoteId> = all(
            conn,
            "notes",
            "SELECT n.id FROM notes n
             WHERE n.deleted_at IS NULL
               AND (n.id IN (SELECT c.note_id FROM cards c
                             WHERE c.id IN (SELECT value FROM json_each(?2)))
                    OR (n.home_deck_id IN (SELECT value FROM json_each(?1))
                        AND NOT EXISTS (SELECT 1 FROM cards c WHERE c.note_id = n.id
                                          AND c.deleted_at IS NULL)))
               AND NOT EXISTS (SELECT 1 FROM cards c
                               WHERE c.note_id = n.id AND c.deleted_at IS NULL
                                 AND c.id NOT IN (SELECT value FROM json_each(?2)))
             ORDER BY n.id",
            params![deck_list, card_list],
            |r| id(r, 0),
        )?;
        let set = DeckTrashSet {
            decks,
            cards,
            notes,
        };
        apply_trash_set(conn, &set, Some(at))?;
        Ok(set)
    }

    fn restore_deck_subtree(&mut self, deck: DeckId) -> StorageResult<DeckTrashSet> {
        let conn = self.conn();
        let Some(at) = deck_deleted_at(conn, deck)? else {
            return Ok(DeckTrashSet::default());
        };
        // Sub-decks trashed together with `deck` (same deleted_at), walking through them only.
        let decks: Vec<DeckId> = all(
            conn,
            T,
            "WITH RECURSIVE sub(id, depth) AS (
                 SELECT id, 0 FROM decks WHERE id = ?1
                 UNION
                 SELECT d.id, sub.depth + 1 FROM decks d JOIN sub ON d.parent_id = sub.id
                 WHERE d.deleted_at = ?2 AND sub.depth < ?3
             )
             SELECT id FROM sub ORDER BY id",
            params![deck.to_string(), at.0, MAX_DEPTH],
            |r| id(r, 0),
        )?;
        let deck_list = json_list(&decks);
        let cards: Vec<CardId> = all(
            conn,
            "cards",
            "SELECT id FROM cards
             WHERE deleted_at = ?2 AND deck_id IN (SELECT value FROM json_each(?1))
             ORDER BY id",
            params![deck_list, at.0],
            |r| id(r, 0),
        )?;
        let notes: Vec<NoteId> = all(
            conn,
            "notes",
            "SELECT n.id FROM notes n
             WHERE n.deleted_at = ?3
               AND (n.id IN (SELECT c.note_id FROM cards c
                             WHERE c.id IN (SELECT value FROM json_each(?2)))
                    OR n.home_deck_id IN (SELECT value FROM json_each(?1)))
             ORDER BY n.id",
            params![deck_list, json_list(&cards), at.0],
            |r| id(r, 0),
        )?;
        let set = DeckTrashSet {
            decks,
            cards,
            notes,
        };
        apply_trash_set(conn, &set, None)?;
        Ok(set)
    }

    fn set_trash_set_deleted(
        &mut self,
        set: &DeckTrashSet,
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        apply_trash_set(self.conn(), set, deleted_at)
    }
}

/// `deleted_at` of a deck; `NotFound` if the deck does not exist.
fn deck_deleted_at(conn: &Connection, deck: DeckId) -> StorageResult<Option<Timestamp>> {
    one(
        conn,
        T,
        "SELECT deleted_at FROM decks WHERE id = ?1",
        [deck.to_string()],
        |r| opt_ts(r, 0),
    )?
    .ok_or_else(|| StorageError::NotFound {
        entity: "deck",
        id: deck.to_string(),
    })
}

fn apply_trash_set(
    conn: &Connection,
    set: &DeckTrashSet,
    deleted_at: Option<Timestamp>,
) -> StorageResult<u32> {
    Ok(set_deleted(conn, "decks", &set.decks, deleted_at)?
        + set_deleted(conn, "cards", &set.cards, deleted_at)?
        + set_deleted(conn, "notes", &set.notes, deleted_at)?)
}

/// Shared trash/restore: trash touches live rows only, restore trashed rows only.
pub(super) fn set_deleted<I: std::fmt::Display>(
    conn: &rusqlite::Connection,
    table: &'static str,
    ids: &[I],
    deleted_at: Option<Timestamp>,
) -> StorageResult<u32> {
    let sql = match deleted_at {
        Some(_) => format!("UPDATE {table} SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL"),
        None => format!("UPDATE {table} SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL AND ?2 IS NULL"),
    };
    let mut changed = 0;
    for i in ids {
        changed += exec(
            conn,
            table,
            &sql,
            params![i.to_string(), opt_ms(deleted_at)],
        )?;
    }
    Ok(count(changed))
}

// ------------------------------------------------------------------ presets

const PRESET_COLS: &str = "id, name, config, created_at, updated_at";

fn preset_row(r: &Row<'_>) -> rusqlite::Result<Preset> {
    Ok(Preset {
        id: id(r, 0)?,
        name: r.get(1)?,
        config: json(r, 2)?,
        created_at: ts(r, 3)?,
        updated_at: ts(r, 4)?,
    })
}

impl<M: Mode> PresetRead for SqliteTx<'_, M> {
    fn get_preset(&self, preset: PresetId) -> StorageResult<Option<Preset>> {
        one(
            self.conn(),
            "presets",
            &format!("SELECT {PRESET_COLS} FROM presets WHERE id = ?1"),
            [preset.to_string()],
            preset_row,
        )
    }

    fn list_presets(&self) -> StorageResult<Vec<Preset>> {
        all(
            self.conn(),
            "presets",
            &format!("SELECT {PRESET_COLS} FROM presets ORDER BY name COLLATE NOCASE, id"),
            [],
            preset_row,
        )
    }
}

impl PresetWrite for SqliteTx<'_, WriteMode> {
    fn upsert_preset(&mut self, p: &Preset) -> StorageResult<()> {
        exec(
            self.conn(),
            "presets",
            &format!(
                "INSERT INTO presets ({PRESET_COLS}) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, config = excluded.config,
                     created_at = excluded.created_at, updated_at = excluded.updated_at"
            ),
            params![
                p.id.to_string(),
                p.name,
                to_json("presets", &p.config)?,
                p.created_at.0,
                p.updated_at.0,
            ],
        )?;
        Ok(())
    }

    fn delete_preset(&mut self, id: PresetId) -> StorageResult<bool> {
        let n = exec(
            self.conn(),
            "presets",
            "DELETE FROM presets WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(n > 0)
    }
}
