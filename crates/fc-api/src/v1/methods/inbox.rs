//! Inbox, deep links and clipboard suggestions (v0.9).

use crate::v1::common::Empty;
use crate::v1::inbox::*;

methods! {
    // ---- inbox ----
    /// Proposes a card (HTTP API, extension, quick add, clipboard).
    InboxAdd = "inbox.add", Command, "inbox", "inbox:write", Public, undo: true, AddInboxItem => InboxItemDto;
    /// Inbox items, newest first.
    InboxList = "inbox.list", Query, "inbox", "inbox:read", Public, undo: false, ListInbox => InboxPage;
    /// Pending item count (menu badge).
    InboxCountMethod = "inbox.count", Query, "inbox", "inbox:read", Public, undo: false, Empty => InboxCount;
    /// Accepts pending items as notes (all or nothing, one undo step).
    InboxAccept = "inbox.accept", Command, "inbox", "inbox:write", Public, undo: true, AcceptInbox => AcceptInboxResult;
    /// Rejects pending items.
    InboxReject = "inbox.reject", Command, "inbox", "inbox:write", Public, undo: true, InboxItemIds => InboxChangeCount;
    /// Internal inverse of add: deletes rows.
    InboxRemove = "inbox.remove", Command, "inbox", "inbox:write", Internal, undo: true, InboxItemIds => InboxChangeCount;
    /// Internal inverse of remove/accept/reject: puts rows back exactly.
    InboxRestore = "inbox.restore", Command, "inbox", "inbox:write", Internal, undo: true, RestoreInboxItems => InboxChangeCount;

    // ---- deep_links ----
    /// Handles a `flashcard://` link: navigation, or an `add` link → inbox item.
    DeepLinksOpen = "deep_links.open", Command, "deep_links", "inbox:write", Public, undo: true, OpenDeepLink => DeepLinkOutcome;

    // ---- clipboard ----
    /// Whether copied text should be suggested as a card (nothing is stored).
    ClipboardCheck = "clipboard.check", Query, "clipboard", "clipboard:read", Public, undo: false, CheckClipboard => ClipboardVerdict;
}
