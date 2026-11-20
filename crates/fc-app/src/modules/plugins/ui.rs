//! Plugin operations started by the user: contributed commands and the declarative UI of
//! views and study modes. Each step is one plugin call; what it writes is one undo step
//! (rolled back if the plugin fails). UI state lives here between steps (instances do not
//! survive a call).

use super::host::{plugin_failed, Mode, Session};
use super::{inactive, no_contribution, require_active};
use crate::error::{AppError, AppResult};
use crate::App;
use fc_api::v1::common::Empty;
use fc_api::v1::oplog::Actor;
use fc_api::v1::plugins::{
    ActPluginUi, ClosePluginUi, OpenPluginUi, PluginCommandResult, PluginNotice,
    RunPluginCommand, UiNode, UiReply, UiSession,
};
use fc_api::{ApiError, ErrorCode};
use fc_plugins::abi::ops;
use serde_json::{json, Value};

pub(super) fn run(req: RunPluginCommand, app: &App, _actor: &Actor) -> AppResult<PluginCommandResult> {
    super::engines::run_command(app, &req.plugin_id, &req.command, req.params)
}

/// Sanitizes rich content before it leaves the backend.
fn sanitize(node: &mut UiNode) {
    match node {
        UiNode::Html { html } => *html = fc_core::content::sanitize_html(html),
        UiNode::Column { children } | UiNode::Row { children } => {
            children.iter_mut().for_each(sanitize);
        }
        _ => {}
    }
}

/// Largest `state` a UI session may keep between steps (serialized).
pub(crate) const MAX_SESSION_STATE_BYTES: usize = 1024 * 1024;
/// Open UI sessions per plugin; opening one more closes its oldest.
pub(crate) const MAX_SESSIONS_PER_PLUGIN: usize = 8;

/// Number of session `id` (`s12` → 12), for age order.
fn session_number(id: &str) -> u64 {
    id.trim_start_matches('s').parse().unwrap_or(0)
}

fn session_step(
    app: &App,
    session_id: String,
    plugin_id: &str,
    view: &str,
    result: Result<Value, fc_plugins::PluginError>,
    notices: Vec<PluginNotice>,
) -> AppResult<UiSession> {
    let host = app.plugins();
    let value = match result {
        Ok(v) => v,
        Err(e) => {
            if let Ok(mut s) = host.sessions.lock() {
                s.remove(&session_id);
            }
            return Err(plugin_failed(plugin_id, &e));
        }
    };
    let mut reply: UiReply = serde_json::from_value(value).map_err(|e| {
        plugin_failed(
            plugin_id,
            &fc_plugins::PluginError::new(fc_plugins::PluginErrorKind::Abi, format!("UiReply: {e}")),
        )
    })?;
    if let Some(node) = reply.node.as_mut() {
        sanitize(node);
    }
    let state_bytes = serde_json::to_vec(&reply.state).map_or(usize::MAX, |b| b.len());
    if !reply.done && state_bytes > MAX_SESSION_STATE_BYTES {
        if let Ok(mut s) = host.sessions.lock() {
            s.remove(&session_id);
        }
        return Err(plugin_failed(
            plugin_id,
            &fc_plugins::PluginError::new(
                fc_plugins::PluginErrorKind::Abi,
                format!("UiReply: state of {state_bytes} bytes (at most {MAX_SESSION_STATE_BYTES})"),
            ),
        ));
    }
    if let Ok(mut s) = host.sessions.lock() {
        if reply.done {
            s.remove(&session_id);
        } else {
            // Bounded per plugin: the oldest other session goes.
            loop {
                let mine: Vec<&String> = s
                    .iter()
                    .filter(|(id, sess)| sess.plugin_id == plugin_id && **id != session_id)
                    .map(|(id, _)| id)
                    .collect();
                if mine.len() < MAX_SESSIONS_PER_PLUGIN {
                    break;
                }
                let oldest = mine
                    .into_iter()
                    .min_by_key(|id| session_number(id))
                    .cloned();
                match oldest {
                    Some(id) => {
                        s.remove(&id);
                    }
                    None => break,
                }
            }
            s.insert(
                session_id.clone(),
                Session {
                    plugin_id: plugin_id.to_owned(),
                    view: view.to_owned(),
                    state: reply.state.clone(),
                },
            );
        }
    }
    Ok(UiSession {
        session_id,
        plugin_id: plugin_id.to_owned(),
        view: view.to_owned(),
        title: reply.title,
        node: reply.node,
        done: reply.done,
        notices,
    })
}

pub(super) fn open(req: OpenPluginUi, app: &App, _actor: &Actor) -> AppResult<UiSession> {
    let host = app.plugins();
    let plugin = require_active(host, &req.plugin_id)?;
    let c = &plugin.manifest.contributes;
    let permission = if c.views.contains(&req.view) {
        "ui:view"
    } else if c.study_modes.contains(&req.view) {
        "ui:study_mode"
    } else {
        return Err(no_contribution(&req.plugin_id, &req.view));
    };
    if !plugin.has(permission) {
        return Err(inactive(&req.plugin_id));
    }
    let session_id = host.new_session_id();
    let input = json!({ "view": req.view, "session": session_id, "input": req.input });
    let (r, notices) = host.run_op(app, &plugin, ops::UI_OPEN, input, Mode::Operation);
    session_step(app, session_id, &req.plugin_id, &req.view, r, notices)
}

pub(super) fn act(req: ActPluginUi, app: &App, _actor: &Actor) -> AppResult<UiSession> {
    let host = app.plugins();
    let (plugin_id, view, state) = {
        let s = host
            .sessions
            .lock()
            .map_err(|_| AppError::api(ErrorCode::Internal, "sessions poisoned"))?;
        let sess = s.get(&req.session_id).ok_or_else(|| {
            AppError::Api(
                ApiError::new(
                    ErrorCode::NotFoundPluginSession,
                    format!("no open session {}", req.session_id),
                )
                .with_detail("session_id", req.session_id.clone()),
            )
        })?;
        (sess.plugin_id.clone(), sess.view.clone(), sess.state.clone())
    };
    let plugin = require_active(host, &plugin_id)?;
    let input = json!({
        "view": view,
        "session": req.session_id,
        "action": req.action,
        "values": req.values,
        "state": state,
    });
    let (r, notices) = host.run_op(app, &plugin, ops::UI_ACT, input, Mode::Operation);
    session_step(app, req.session_id, &plugin_id, &view, r, notices)
}

pub(super) fn close(req: ClosePluginUi, app: &App, _actor: &Actor) -> AppResult<Empty> {
    if let Ok(mut s) = app.plugins().sessions.lock() {
        s.remove(&req.session_id);
    }
    Ok(Empty {})
}
