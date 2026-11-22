//! The user-facing transport dispatch shared by the desktop shell's IPC command and the dev
//! bridge (`fc dev serve`, v1.3): one function so both reach the app the same way — lock
//! gating, the user actor, `null` params meaning "none". A thin adapter, no business logic.

use crate::App;
use fc_api::v1::oplog::Actor;
use fc_api::ApiError;
use serde_json::Value;

/// Dispatches a qualified method (`v1.decks.create`) with its params as the user. A missing
/// payload (`null`) means "no params" (`{}`). While the app is locked only the lock screen's
/// methods get through (`PERMISSION_APP_LOCKED`, v1.0). Internal methods (undo inverses) are
/// refused by the registry for every transport actor.
pub fn dispatch(app: &App, command: &str, payload: Value) -> Result<Value, ApiError> {
    crate::modules::profiles::gate(app, command)?;
    let params = match payload {
        Value::Null => Value::Object(serde_json::Map::new()),
        other => other,
    };
    app.invoke(command, params, &Actor::User)
}
