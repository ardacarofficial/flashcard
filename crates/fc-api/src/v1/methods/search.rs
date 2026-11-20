//! Search (query language) and saved queries.

use crate::v1::common::*;
use crate::v1::search::*;

methods! {
    // ---- search ----
    /// A page of cards matching a query.
    SearchCardsMethod = "search.cards", Query, "browse", "search:read", Public, undo: false, SearchCards => CardSearchPage;
    /// Every card/note matching a query.
    SearchIdsMethod = "search.ids", Query, "browse", "search:read", Public, undo: false, QueryText => SearchIds;
    /// Validates a query (positioned error) and returns its canonical form.
    SearchCheck = "search.check", Query, "browse", "search:read", Public, undo: false, QueryText => QueryCheck;

    // ---- saved queries ----
    /// Saved queries.
    SavedQueriesList = "saved_queries.list", Query, "browse", "search:read", Public, undo: false, Empty => SavedQueryList;
    /// Saves a query under a name.
    SavedQueriesCreate = "saved_queries.create", Command, "browse", "search:write", Public, undo: true, CreateSavedQuery => SavedQueryDto;
    /// Renames / edits a saved query.
    SavedQueriesUpdate = "saved_queries.update", Command, "browse", "search:write", Public, undo: true, UpdateSavedQuery => SavedQueryDto;
    /// Deletes a saved query.
    SavedQueriesDelete = "saved_queries.delete", Command, "browse", "search:write", Public, undo: true, SavedQueryRef => SavedQueryDto;
    /// Internal inverse of delete: puts the row back.
    SavedQueriesRestore = "saved_queries.restore", Command, "browse", "search:write", Internal, undo: true, RestoreSavedQuery => SavedQueryDto;
}
