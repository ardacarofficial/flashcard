//! `themes` module: the backend owns the choice and the tokens; changes are validated, one
//! undo step, and the module can be switched off. MemStore + SQLite memory + SQLite file.

use crate::testing::{TempSqlite, TestApp};
use fc_storage::SqliteStore;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::settings::SetSetting;
use fc_api::v1::themes::*;
use fc_api::ErrorCode;
use fc_core::settings::SettingScope;
use fc_core::theme;
use serde_json::json;
use std::sync::Arc;

fn apps() -> Vec<TestApp> {
    vec![
        TestApp::mem(),
        TestApp::new(Arc::new(TempSqlite::new())),
        TestApp::new(Arc::new(SqliteStore::open_in_memory().unwrap())),
    ]
}

fn set(t: &TestApp, mode: Option<ThemeMode>, preset: Option<&str>, accent: Option<&str>) -> ThemeDto {
    t.ok::<ThemesSet>(SetTheme {
        mode,
        preset: preset.map(str::to_owned),
        accent: accent.map(str::to_owned),
    })
}

#[test]
fn a_fresh_collection_follows_the_system_with_the_default_preset() {
    for t in apps() {
        let cur = t.ok::<ThemesCurrent>(Empty {});
        assert_eq!(cur.mode, ThemeMode::System);
        assert_eq!(cur.preset, theme::DEFAULT_PRESET);
        assert_eq!(cur.accent, None);
        let def = theme::resolve(theme::DEFAULT_PRESET, None);
        assert_eq!((cur.light, cur.dark), (def.light, def.dark));
        let presets = t.ok::<ThemesPresets>(Empty {}).presets;
        let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, theme::preset_ids());
        assert!(presets.iter().all(|p| p.name_key == format!("themes.preset.{}", p.id)));
        assert!(presets.iter().all(|p| p.light.len() == theme::TOKENS.len()));
    }
}

#[test]
fn set_is_validated_and_one_undo_step() {
    for t in apps() {
        let before = t.ok::<ThemesCurrent>(Empty {});
        let after = set(&t, Some(ThemeMode::Dark), Some("sage"), Some("#C2185B"));
        assert_eq!(after.mode, ThemeMode::Dark);
        assert_eq!(after.preset, "sage");
        assert_eq!(after.accent.as_deref(), Some("#c2185b"));
        assert_eq!(after.light["accent"], "#c2185b");
        assert_eq!(after, t.ok::<ThemesCurrent>(Empty {}));
        let ev = t.take_events();
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e.event, DomainEvent::SettingChanged { .. }))
                .count(),
            3
        );

        // Invalid values: nothing is written.
        for (preset, accent) in [(Some("neon"), None), (None, Some("blue")), (None, Some("#12345"))] {
            let e = t.err::<ThemesSet>(SetTheme {
                mode: None,
                preset: preset.map(str::to_owned),
                accent: accent.map(str::to_owned),
            });
            assert_eq!(e.code, ErrorCode::ValidationSettingValue, "{preset:?} {accent:?}");
        }
        let e = t.err::<ThemesSet>(SetTheme::default());
        assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
        assert_eq!(after, t.ok::<ThemesCurrent>(Empty {}));

        // One undo step restores everything; redo brings it back.
        t.ok::<OplogUndo>(Empty {});
        assert_eq!(before, t.ok::<ThemesCurrent>(Empty {}));
        t.ok::<OplogRedo>(Empty {});
        assert_eq!(after, t.ok::<ThemesCurrent>(Empty {}));

        // `""` goes back to the preset's accent; an unchanged value writes nothing.
        let plain = set(&t, None, Some("sage"), Some(""));
        assert_eq!(plain.accent, None);
        assert_eq!(plain.light, theme::resolve("sage", None).light);
        t.ok::<OplogUndo>(Empty {});
        assert_eq!(after, t.ok::<ThemesCurrent>(Empty {}));
    }
}

#[test]
fn values_written_around_the_command_fall_back() {
    let t = TestApp::mem();
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: "themes.accent".into(),
        value: json!("garbage"),
    });
    let cur = t.ok::<ThemesCurrent>(Empty {});
    assert_eq!(cur.accent, None);
    assert_eq!(cur.light, theme::resolve(theme::DEFAULT_PRESET, None).light);
    // The preset is a choice setting: `settings.set` itself refuses unknown ids.
    let e = t.err::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: "themes.preset".into(),
        value: json!("neon"),
    });
    assert_eq!(e.code, ErrorCode::ValidationSettingValue);
}

#[test]
fn themes_is_a_switchable_module() {
    let t = TestApp::mem();
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "themes".into(),
        enabled: false,
    });
    let e = t.err::<ThemesCurrent>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    // The light/dark/system choice belongs to `core` and keeps working.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: "app.theme".into(),
        value: json!("dark"),
    });
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "themes".into(),
        enabled: true,
    });
    assert_eq!(t.ok::<ThemesCurrent>(Empty {}).mode, ThemeMode::Dark);
}
