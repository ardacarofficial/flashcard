//! Subtitle extension integration (v1.2, docs/06 §5.1). Keys are security decisions, not undo
//! steps (`undo: false`): an undo must never revive a revoked key. A capture is one undo step
//! (its note, its deck when one was made, and the capture → note mapping).

use crate::v1::common::{AffectedCount, Empty};
use crate::v1::subtitle::*;

methods! {
    // ---- subtitle ----
    /// Access keys of the Subtitle extension, newest first.
    SubtitleKeysList = "subtitle.keys.list", Query, "subtitle", "subtitle:read", Public, undo: false, Empty => SubtitleKeyList;
    /// Makes a key; the response holds it once (only its hash is stored).
    SubtitleKeysCreate = "subtitle.keys.create", Command, "subtitle", "subtitle:write", Public, undo: false, CreateSubtitleKey => CreatedSubtitleKey;
    /// Revokes a key (it stops working at once).
    SubtitleKeysRevoke = "subtitle.keys.revoke", Command, "subtitle", "subtitle:write", Public, undo: false, SubtitleKeyRef => SubtitleKeyDto;
    /// Server address to enter in the extension, default deck, captures received.
    SubtitleStatusGet = "subtitle.status", Query, "subtitle", "subtitle:read", Public, undo: false, Empty => SubtitleStatus;
    /// Sets (or clears) the deck captures go to when they name none.
    SubtitleDefaultDeckSet = "subtitle.default_deck.set", Command, "subtitle", "subtitle:write", Public, undo: true, SetSubtitleDefaultDeck => SubtitleStatus;
    /// Internal: a capture from the HTTP server becomes a note (or is a duplicate).
    SubtitleCapturesCreate = "subtitle.captures.create", Command, "subtitle", "subtitle:write", Internal, undo: true (replay: false), RecordSubtitleCapture => SubtitleCaptureOutcome;
    /// Internal inverse of create/restore: forgets capture → note mappings.
    SubtitleCapturesForget = "subtitle.captures.forget", Command, "subtitle", "subtitle:write", Internal, undo: true, SubtitleCaptureIds => AffectedCount;
    /// Internal inverse of forget: puts mappings back exactly.
    SubtitleCapturesRestore = "subtitle.captures.restore", Command, "subtitle", "subtitle:write", Internal, undo: true, SubtitleCaptureRows => AffectedCount;
    /// Internal: stores an uploaded picture or sound in the media store. Never in the oplog
    /// (the note that uses it is the undo step).
    SubtitleMediaUpload = "subtitle.media.upload", Command, "subtitle", "subtitle:write", Internal, undo: true (replay: false), UploadSubtitleMedia => SubtitleUploadResponse;
    /// Internal: notes a key's last request (HTTP server, at most once a minute). Never in
    /// the oplog.
    SubtitleKeysTouch = "subtitle.keys.touch", Command, "subtitle", "subtitle:write", Internal, undo: true (replay: false), SubtitleKeyRef => Empty;
}
