//! Attributes (classifications) and their values.

use super::*;

/// What [`AttributeWrite::remove_attribute_option`] changed: everything
/// [`AttributeWrite::restore_attribute_option`] needs to undo it exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionRemoval {
    /// The removed option as it was (label, color, order before removal).
    pub option: AttributeOption,
    /// Its index in the definition's sorted option list before removal.
    pub index: usize,
    /// The option values were moved to (`None` = values were dropped).
    pub replaced_with: Option<AttributeOptionId>,
    /// Entities whose `option` row was rewritten to `replaced_with`.
    pub moved: Vec<EntityRef>,
    /// Entities whose `option` row was deleted (no replacement, or the entity already had
    /// `replaced_with` — multi-select).
    pub dropped: Vec<EntityRef>,
}

/// Attribute reads.
pub trait AttributeRead {
    /// Definition by id.
    fn get_attribute_def(&self, id: AttributeDefId) -> StorageResult<Option<AttributeDef>>;
    /// All definitions.
    fn list_attribute_defs(&self, include_deleted: bool) -> StorageResult<Vec<AttributeDef>>;
    /// Values on one entity (definitions resolved for decoding; trashed definitions skipped).
    fn get_attribute_values(
        &self,
        entity: EntityRef,
    ) -> StorageResult<Vec<(AttributeDefId, AttributeValue)>>;
    /// Entities whose value of `attr` includes `option`.
    fn entities_with_option(
        &self,
        attr: AttributeDefId,
        option: AttributeOptionId,
    ) -> StorageResult<Vec<EntityRef>>;
    /// Definitions that apply to `deck` (`None` = collection only), resolved with
    /// `fc_core::attribute::effective_attribute_defs` over the deck's ancestry: live
    /// definitions only, deepest `key` wins. Each comes with its depth (`0` = collection,
    /// `ancestry.len()` = defined on `deck` itself), in the resolver's order.
    fn effective_attribute_defs_for_deck(
        &self,
        deck: Option<DeckId>,
    ) -> StorageResult<Vec<(AttributeDef, usize)>>;
    /// Note-level (`target = note`) definitions that apply to `note`: resolved like
    /// [`AttributeRead::effective_attribute_defs_for_deck`] over the note's **home deck**,
    /// then filtered by target (shadowing by `key` happens before filtering). `NotFound` for
    /// an unknown note.
    fn effective_attribute_defs_for_note(
        &self,
        note: NoteId,
    ) -> StorageResult<Vec<(AttributeDef, usize)>>;
    /// Card-level (`target = card`) definitions that apply to `card`, resolved over the
    /// **card's deck**. `NotFound` for an unknown card.
    fn effective_attribute_defs_for_card(
        &self,
        card: CardId,
    ) -> StorageResult<Vec<(AttributeDef, usize)>>;
}

/// Attribute writes.
pub trait AttributeWrite: AttributeRead {
    /// Inserts a definition.
    fn insert_attribute_def(&mut self, def: &AttributeDef) -> StorageResult<()>;
    /// Replaces a definition's columns (options JSON included).
    fn update_attribute_def(&mut self, def: &AttributeDef) -> StorageResult<()>;
    /// Trash/restore a definition. Values are kept.
    fn set_attribute_def_deleted(
        &mut self,
        id: AttributeDefId,
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<()>;
    /// Inserts `option` (caller's id) into `attr`'s option list at `index` of the sorted list
    /// (clamped to the end); orders are renumbered `0..n` and `updated_at = now`. Fails with
    /// `Constraint { "attribute_option_id" }` if the id is taken, `NotFound` for an unknown
    /// definition.
    fn insert_attribute_option(
        &mut self,
        attr: AttributeDefId,
        option: &AttributeOption,
        index: usize,
        now: Timestamp,
    ) -> StorageResult<()>;
    /// Removes `option` from `attr` (orders renumbered, `updated_at = now`) and rewrites its
    /// values: with `replace_with = Some(r)` each entity's `option` row becomes `r` (dropped
    /// if the entity already has `r`); with `None` the rows are deleted. Merge = `Some(into)`.
    /// Returns everything changed. `NotFound { entity: "attribute_option" }` for an unknown
    /// option or replacement.
    fn remove_attribute_option(
        &mut self,
        attr: AttributeDefId,
        option: AttributeOptionId,
        replace_with: Option<AttributeOptionId>,
        now: Timestamp,
    ) -> StorageResult<OptionRemoval>;
    /// Exact inverse of [`AttributeWrite::remove_attribute_option`], atomically: re-inserts
    /// `removal.option` at `removal.index`, rewrites `replaced_with` rows of `moved` back to
    /// the option and re-adds the option row for `dropped`.
    fn restore_attribute_option(
        &mut self,
        attr: AttributeDefId,
        removal: &OptionRemoval,
        now: Timestamp,
    ) -> StorageResult<()>;
    /// Sets values on one entity: `Some` replaces that attribute's rows, `None` removes them.
    /// Attributes not listed are untouched. Rows use `AttributeValue::to_stored()`.
    fn set_attribute_values(
        &mut self,
        entity: EntityRef,
        values: &[(AttributeDefId, Option<AttributeValue>)],
    ) -> StorageResult<()>;
}
