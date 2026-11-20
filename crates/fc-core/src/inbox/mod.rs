//! Pure helpers for what reaches the inbox from outside the app (v0.9, docs/06 §4–7):
//! [`deeplink`] parses `flashcard://` links, [`clipboard`] decides whether copied text is worth
//! suggesting as a card. No I/O: the desktop shell hands strings in, the app acts on the
//! result.

pub mod clipboard;
pub mod deeplink;
