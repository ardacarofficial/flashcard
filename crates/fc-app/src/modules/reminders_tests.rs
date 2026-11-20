//! Daily study reminders (v1.0, docs/02 §19).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::testing::{MemStore, TestApp};
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::onboarding::InstallStarter;
use fc_api::v1::settings::SetSetting;
use fc_api::v1::study::{AnswerCard, StartSession, StudyKind, StudyMode};
use fc_core::card::Rating;
use fc_core::time::{FixedClock, MILLIS_PER_DAY};

const IST: i32 = 180;
/// 2026-11-20 (a Friday), local day index.
const FRIDAY: i64 = 20_777;

fn at(day: i64, h: i64, m: i64) -> Timestamp {
    Timestamp((FRIDAY + day) * MILLIS_PER_DAY + (h * 60 + m) * 60_000 - i64::from(IST) * 60_000)
}

struct Fx {
    t: TestApp,
    clock: Arc<FixedClock>,
}

impl Fx {
    fn new() -> Self {
        let clock = Arc::new(FixedClock::new(at(0, 10, 0)));
        let t = TestApp::with_clock(Arc::new(MemStore::new()), clock.clone());
        t.ok::<StarterInstall>(InstallStarter {
            id: "world_capitals".into(),
            parent_id: None,
            locale: None,
        });
        Self { t, clock }
    }

    fn set(&self, key: &str, value: Value) {
        self.t.ok::<SettingsSet>(SetSetting {
            scope: SettingScope::App,
            key: key.into(),
            value,
        });
    }

    fn due_events(&self) -> Vec<(ReminderCounts, bool)> {
        self.t
            .take_events()
            .into_iter()
            .filter_map(|e| match e.event {
                DomainEvent::ReminderDue { counts, test } => Some((counts, test)),
                _ => None,
            })
            .collect()
    }
}

/// "You have N cards today" once at the chosen time, not again that day, again the next day.
#[test]
fn reminds_once_a_day_at_the_chosen_time() {
    let fx = Fx::new();
    assert_eq!(check(&fx.t.app, IST).unwrap(), None, "off by default");
    fx.set(ENABLED, json!(true));
    fx.set(MINUTE_OF_DAY, json!(19 * 60));
    fx.t.take_events();
    assert_eq!(check(&fx.t.app, IST).unwrap(), None, "10:00: too early");
    fx.clock.set(at(0, 19, 0));
    let counts = check(&fx.t.app, IST).unwrap().expect("19:00: due");
    assert!(counts.new > 0);
    assert_eq!(fx.due_events(), vec![(counts, false)]);
    fx.clock.set(at(0, 19, 1));
    assert_eq!(check(&fx.t.app, IST).unwrap(), None, "once a day");
    fx.clock.set(at(1, 19, 2));
    assert!(check(&fx.t.app, IST).unwrap().is_some(), "next day again");

    let st = fx.t.ok::<RemindersStatus>(ReminderStatusRequest {
        tz_offset_mins: Some(IST),
    });
    assert!(st.enabled);
    assert_eq!(st.next_at, Some(at(2, 19, 0)));
}

/// Nothing when today's study is done, on an unchosen weekday, or with the module off.
#[test]
fn no_reminder_after_studying_or_on_other_days() {
    let fx = Fx::new();
    fx.set(ENABLED, json!(true));
    // Weekdays only (Mon–Fri): Saturday is quiet.
    fx.set(WEEKDAYS, json!(0b001_1111));
    fx.clock.set(at(1, 19, 0));
    assert_eq!(check(&fx.t.app, IST).unwrap(), None);

    // Friday, after one review: nothing.
    fx.clock.set(at(0, 12, 0));
    let s = fx.t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Normal,
        mode: Some(StudyMode::Scheduled),
        deck_id: None,
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: Some(1),
        order: None,
        tz_offset_mins: Some(IST),
        modes: None,
    });
    let card = s.current.clone().unwrap();
    fx.t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: card.card_id,
        rating: Rating::Good,
        duration_ms: 3_000,
        reveal_ms: None,
    });
    fx.clock.set(at(0, 19, 0));
    assert_eq!(check(&fx.t.app, IST).unwrap(), None, "already studied today");
    assert!(
        fx.t.ok::<RemindersStatus>(ReminderStatusRequest {
            tz_offset_mins: Some(IST)
        })
        .studied_today
    );

    // Module off: never.
    fx.clock.set(at(3, 19, 0)); // Monday
    fx.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: false,
    });
    assert_eq!(check(&fx.t.app, IST).unwrap(), None);
}

#[test]
fn test_reminder_publishes_right_away() {
    let fx = Fx::new();
    fx.t.take_events();
    let counts = fx.t.ok::<RemindersTest>(Empty {});
    assert_eq!(fx.due_events(), vec![(counts, true)]);
}

#[test]
fn notification_text_is_translated_and_has_only_numbers() {
    let c = ReminderCounts { due: 3, new: 5 };
    let (title, en) = notification_text(Locale::En, c);
    assert_eq!(title, "Flashcard");
    assert!(en.contains('8') && en.contains("3 due") && en.contains("5 new"));
    let (_, tr) = notification_text(Locale::Tr, c);
    assert!(tr.starts_with("Bugün 8 kart"));
    assert_ne!(TRAY_QUIT.get(Locale::En), TRAY_QUIT.get(Locale::Tr));
}

#[test]
fn keep_in_tray_follows_its_setting() {
    let fx = Fx::new();
    assert!(!keeps_running_in_tray(&fx.t.app));
    fx.set(KEEP_IN_TRAY, json!(true));
    assert!(keeps_running_in_tray(&fx.t.app));
}
