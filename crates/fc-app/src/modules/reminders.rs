//! `reminders` module (v1.0, docs/02 §19): a daily OS notification ("you have 40 cards
//! today") at a chosen time on chosen weekdays, none on days already studied, and the option
//! to keep the app running in the tray when its window is closed.
//!
//! The decision is `fc_core::ux::reminder::decide`; this module gathers its inputs (settings,
//! today's counts, whether anything was reviewed today, the last day reminded — `ux_state`,
//! not undone) and publishes `reminder_due`. The desktop shell calls [`check`] once a minute
//! ([`spawn_reminders`]) with the OS's UTC offset and turns the event into a notification;
//! the window translates the text.

use crate::error::AppResult;
use crate::handler::QueryCtx;
use crate::module::{bool_setting, main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::settings::resolve_stored;
use crate::App;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::oplog::Actor;
use fc_api::v1::reminders::*;
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::ux::reminder::{
    decide, next_at, ReminderDecision, ReminderSchedule, ALL_DAYS, DEFAULT_MINUTE_OF_DAY,
    MINUTES_PER_DAY,
};
use fc_core::i18n::{Locale, LocalizedText};
use fc_core::Timestamp;
use fc_storage::repo::UX_REMINDER_LAST_DAY;
use fc_storage::Reads;
use serde_json::{json, Value};
use std::sync::{Arc, Weak};

/// Module id.
pub const ID: &str = "reminders";
/// Setting: reminders on.
pub const ENABLED: &str = "reminders.enabled";
/// Setting: local time, minutes after midnight.
pub const MINUTE_OF_DAY: &str = "reminders.minute_of_day";
/// Setting: weekday bit mask (Monday = bit 0).
pub const WEEKDAYS: &str = "reminders.weekdays";
/// Setting: closing the window keeps the app in the tray.
pub const KEEP_IN_TRAY: &str = "reminders.keep_in_tray";

/// How often [`spawn_reminders`] checks.
pub const REMINDER_TICK: std::time::Duration = std::time::Duration::from_secs(60);

fn int_setting(key: &str, min: i64, max: i64, default: u32) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: SettingKind::Integer {
            min: Some(min),
            max: Some(max),
        },
        default: json!(default),
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced: false,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.reminders.name",
        description_key: "modules.reminders.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            bool_setting(ENABLED, false, &[SettingScopeKind::App]),
            int_setting(
                MINUTE_OF_DAY,
                0,
                i64::from(MINUTES_PER_DAY - 1),
                DEFAULT_MINUTE_OF_DAY,
            ),
            int_setting(WEEKDAYS, 0, i64::from(ALL_DAYS), ALL_DAYS),
            bool_setting(KEEP_IN_TRAY, false, &[SettingScopeKind::App]),
        ],
        ui: vec![main_menu("reminders.screen", "nav.reminders", "bell", "/reminders", 95)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<RemindersStatus>(status);
    reg.action::<RemindersTest>(test);
}

struct Settings {
    schedule: ReminderSchedule,
    keep_in_tray: bool,
}

fn setting(reads: &dyn Reads, key: &str) -> AppResult<Option<Value>> {
    Ok(resolve_stored(reads, &SettingScope::App, key)?.map(|(v, _)| v))
}

fn settings(reads: &dyn Reads) -> AppResult<Settings> {
    let flag = |k| -> AppResult<Option<bool>> { Ok(setting(reads, k)?.and_then(|v| v.as_bool())) };
    let int = |k| -> AppResult<Option<u32>> {
        Ok(setting(reads, k)?
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok()))
    };
    Ok(Settings {
        schedule: ReminderSchedule {
            enabled: flag(ENABLED)?.unwrap_or(false),
            minute_of_day: int(MINUTE_OF_DAY)?
                .unwrap_or(DEFAULT_MINUTE_OF_DAY)
                .min(MINUTES_PER_DAY - 1),
            weekdays: int(WEEKDAYS)?.unwrap_or(ALL_DAYS) & ALL_DAYS,
        },
        keep_in_tray: flag(KEEP_IN_TRAY)?.unwrap_or(false),
    })
}

/// What waits today over the whole collection — the queue `study.start { kind: normal }`
/// serves now, daily limits included (the numbers Home's Start and the deck list show) — and
/// whether anything was reviewed in the study day containing `now`.
fn today(
    reads: &dyn Reads,
    registry: &Registry,
    locale: Locale,
    now: Timestamp,
    tz_offset_mins: i32,
) -> AppResult<(ReminderCounts, bool)> {
    use crate::modules::study::today::{today_counts, Planner};
    let view = crate::modules::study::ext::View {
        reads,
        registry,
        locale,
    };
    let mut planner = Planner::new(view, now, tz_offset_mins)?;
    let (new, learning, review) = today_counts(&mut planner, None)?;
    let counts = ReminderCounts {
        due: review.saturating_add(learning),
        new,
    };
    let day = fc_study::day::StudyDay::new(tz_offset_mins, planner.day_start_hour());
    let start = day.start(day.index(now));
    let studied = !reads.review_logs_since(start, "normal")?.is_empty();
    Ok((counts, studied))
}

fn last_day(reads: &dyn Reads) -> AppResult<Option<i64>> {
    Ok(reads.ux_state(UX_REMINDER_LAST_DAY)?.and_then(|v| v.as_i64()))
}

fn status(req: ReminderStatusRequest, ctx: &QueryCtx<'_>) -> AppResult<ReminderStatus> {
    let tz = req.tz_offset_mins.unwrap_or(0);
    let s = settings(ctx.tx)?;
    let (counts, studied_today) = today(ctx.tx, ctx.registry, ctx.locale, ctx.now, tz)?;
    let next = next_at(s.schedule, ctx.now.as_millis(), tz, last_day(ctx.tx)?);
    Ok(ReminderStatus {
        enabled: s.schedule.enabled,
        minute_of_day: s.schedule.minute_of_day,
        weekdays: s.schedule.weekdays,
        keep_in_tray: s.keep_in_tray,
        counts,
        studied_today,
        next_at: next.map(Timestamp),
    })
}

fn test(_: Empty, app: &App, _actor: &Actor) -> AppResult<ReminderCounts> {
    let reads = app.store().read()?;
    // The offset the window last gave `study.start` (the study day's daily limits).
    let tz = crate::modules::study::today::client_tz(&*reads, None)?;
    let (counts, _) = today(&*reads, app.registry(), app.locale(), app.now(), tz)?;
    drop(reads);
    app.publish(vec![DomainEvent::ReminderDue { counts, test: true }], None);
    Ok(counts)
}

/// One reminder check at the app's `now` in a zone `tz_offset_mins` ahead of UTC: publishes
/// `reminder_due` and remembers the day when it is time. Returns what was announced.
pub fn check(app: &App, tz_offset_mins: i32) -> AppResult<Option<ReminderCounts>> {
    // A reminder only says how many cards wait, so it is shown while locked too; not for a
    // collection being switched away from.
    if !app.is_module_enabled(ID) || app.session().is_some_and(|s| s.is_retired()) {
        return Ok(None);
    }
    let now = app.now();
    let (decision, counts) = {
        let reads = app.store().read()?;
        let s = settings(&*reads)?;
        if !s.schedule.enabled {
            return Ok(None);
        }
        let (counts, studied) = today(&*reads, app.registry(), app.locale(), now, tz_offset_mins)?;
        let decision = decide(
            s.schedule,
            now.as_millis(),
            tz_offset_mins,
            last_day(&*reads)?,
            studied,
            counts.due.saturating_add(counts.new),
        );
        (decision, counts)
    };
    let ReminderDecision::Notify { local_day } = decision else {
        return Ok(None);
    };
    let mut tx = app.store().write()?;
    tx.set_ux_state(UX_REMINDER_LAST_DAY, &json!(local_day), now)?;
    tx.commit()?;
    app.publish(vec![DomainEvent::ReminderDue { counts, test: false }], None);
    Ok(Some(counts))
}

/// Title and body of the reminder notification in `locale`. The OS shows it while the
/// window may be hidden (tray), so the backend writes it; numbers only, never card content.
pub fn notification_text(locale: Locale, counts: ReminderCounts) -> (String, String) {
    let total = counts.due.saturating_add(counts.new);
    let body = match locale {
        Locale::Tr => format!(
            "Bugün {total} kart seni bekliyor ({} tekrar, {} yeni).",
            counts.due, counts.new
        ),
        Locale::En => format!(
            "{total} cards are waiting for you today ({} due, {} new).",
            counts.due, counts.new
        ),
    };
    (TRAY_TITLE.get(locale).to_owned(), body)
}

/// Window/tray title.
pub const TRAY_TITLE: LocalizedText = LocalizedText::same("Flashcard");
/// Tray menu: bring the window back.
pub const TRAY_OPEN: LocalizedText = LocalizedText {
    en: "Open Flashcard",
    tr: "Flashcard'ı aç",
};
/// Tray menu: lock now (shown when a lock is set).
pub const TRAY_LOCK: LocalizedText = LocalizedText {
    en: "Lock",
    tr: "Kilitle",
};
/// Tray menu: quit for real.
pub const TRAY_QUIT: LocalizedText = LocalizedText {
    en: "Quit",
    tr: "Çık",
};

/// Whether closing the window should keep the app running in the tray.
pub fn keeps_running_in_tray(app: &App) -> bool {
    app.is_module_enabled(ID)
        && app
            .store()
            .read()
            .ok()
            .and_then(|r| settings(&*r).ok())
            .is_some_and(|s| s.keep_in_tray)
}

/// Background thread running [`check`] every [`REMINDER_TICK`]; `tz_offset_mins` gives the
/// OS's current UTC offset (it changes with daylight saving time). Ends with the app.
pub fn spawn_reminders(
    app: &Arc<App>,
    tz_offset_mins: Arc<dyn Fn() -> i32 + Send + Sync>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    let weak: Weak<App> = Arc::downgrade(app);
    std::thread::Builder::new()
        .name("fc-reminders".into())
        .spawn(move || loop {
            std::thread::sleep(REMINDER_TICK);
            let Some(app) = weak.upgrade() else { break };
            let _ = check(&app, tz_offset_mins());
        })
}

#[cfg(test)]
#[path = "reminders_tests.rs"]
mod tests;
