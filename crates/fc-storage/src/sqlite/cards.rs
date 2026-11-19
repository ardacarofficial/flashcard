//! Cards.

use super::decks::set_deleted;
use super::{all, enum_col, enum_str, exec, exec_one, id, json_list, one, opt_ms, opt_ts, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{CardFilter, CardRead, CardWrite};
use fc_core::card::{Card, SchedulingState};
use fc_core::{CardId, Timestamp};
use rusqlite::{params, Row};

const T: &str = "cards";
const COLS: &str = "id, note_id, deck_id, template_key, card_type, state, due, stability, \
                    difficulty, reps, lapses, last_review, flags, suspended, buried_until, \
                    user_note, created_at, updated_at, deleted_at";

fn card_row(r: &Row<'_>) -> rusqlite::Result<Card> {
    Ok(Card {
        id: id(r, 0)?,
        note_id: id(r, 1)?,
        deck_id: id(r, 2)?,
        template_key: r.get(3)?,
        card_type: r.get(4)?,
        scheduling: SchedulingState {
            state: enum_col(r, 5)?,
            due: opt_ts(r, 6)?,
            stability: r.get(7)?,
            difficulty: r.get(8)?,
            reps: r.get(9)?,
            lapses: r.get(10)?,
            last_review: opt_ts(r, 11)?,
        },
        flags: r.get(12)?,
        suspended: r.get(13)?,
        buried_until: opt_ts(r, 14)?,
        user_note: r.get(15)?,
        created_at: ts(r, 16)?,
        updated_at: ts(r, 17)?,
        deleted_at: opt_ts(r, 18)?,
    })
}

impl<M: Mode> CardRead for SqliteTx<'_, M> {
    fn get_card(&self, card: CardId) -> StorageResult<Option<Card>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM cards WHERE id = ?1"),
            [card.to_string()],
            card_row,
        )
    }

    fn list_cards(&self, filter: &CardFilter) -> StorageResult<Vec<Card>> {
        all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS} FROM cards
                 WHERE (?1 IS NULL OR deck_id IN (SELECT value FROM json_each(?1)))
                   AND (?2 IS NULL OR note_id IN (SELECT value FROM json_each(?2)))
                   AND (?3 OR deleted_at IS NULL)
                 ORDER BY created_at, id"
            ),
            params![
                filter.deck_ids.as_deref().map(json_list),
                filter.note_ids.as_deref().map(json_list),
                filter.include_deleted,
            ],
            card_row,
        )
    }
}

fn card_params(c: &Card) -> StorageResult<[Box<dyn rusqlite::ToSql + '_>; 19]> {
    let s = &c.scheduling;
    Ok([
        Box::new(c.id.to_string()),
        Box::new(c.note_id.to_string()),
        Box::new(c.deck_id.to_string()),
        Box::new(&c.template_key),
        Box::new(&c.card_type),
        Box::new(enum_str(T, &s.state)?),
        Box::new(opt_ms(s.due)),
        Box::new(s.stability),
        Box::new(s.difficulty),
        Box::new(s.reps),
        Box::new(s.lapses),
        Box::new(opt_ms(s.last_review)),
        Box::new(c.flags),
        Box::new(c.suspended),
        Box::new(opt_ms(c.buried_until)),
        Box::new(&c.user_note),
        Box::new(c.created_at.0),
        Box::new(c.updated_at.0),
        Box::new(opt_ms(c.deleted_at)),
    ])
}

impl CardWrite for SqliteTx<'_, WriteMode> {
    fn insert_card(&mut self, c: &Card) -> StorageResult<()> {
        let p = card_params(c)?;
        exec(
            self.conn(),
            T,
            &format!(
                "INSERT INTO cards ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, \
                 ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)"
            ),
            rusqlite::params_from_iter(p.iter()),
        )?;
        Ok(())
    }

    fn update_card(&mut self, c: &Card) -> StorageResult<()> {
        let p = card_params(c)?;
        exec_one(
            self.conn(),
            T,
            "card",
            &c.id,
            "UPDATE cards SET note_id = ?2, deck_id = ?3, template_key = ?4, card_type = ?5,
                 state = ?6, due = ?7, stability = ?8, difficulty = ?9, reps = ?10,
                 lapses = ?11, last_review = ?12, flags = ?13, suspended = ?14,
                 buried_until = ?15, user_note = ?16, created_at = ?17, updated_at = ?18,
                 deleted_at = ?19
             WHERE id = ?1",
            rusqlite::params_from_iter(p.iter()),
        )
    }

    fn set_cards_deleted(
        &mut self,
        ids: &[CardId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        set_deleted(self.conn(), "cards", ids, deleted_at)
    }
}
