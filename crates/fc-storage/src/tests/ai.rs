//! AI tables (migration 0700).

use super::*;
use crate::migrate::{self, MIGRATIONS};
use crate::repo::{AiPriceRecord, AiProviderRecord, AiRouteRecord, AiUsageRecord};
use fc_core::AiProviderId;

fn provider(n: u128, pos: i32) -> AiProviderRecord {
    AiProviderRecord {
        id: AiProviderId(u(0x7000 + n)),
        kind: "anthropic".into(),
        name: format!("Claude {n}"),
        base_url: "https://api.anthropic.com".into(),
        default_model: "claude-sonnet-5-5".into(),
        models: vec!["claude-opus-5-5".into(), "claude-sonnet-5-5".into()],
        is_default: n == 1,
        position: pos,
        created_at: Timestamp(10),
        updated_at: Timestamp(11),
        deleted_at: None,
    }
}

/// 0700 on a database from before v0.8 (with usage rows written by 0001's table): rows
/// survive, the new tables exist with the seeded prices, and no column can hold a key.
#[test]
fn migration_0700_upgrades_an_older_database_and_has_no_key_column() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    {
        let mut conn = Connection::open(&path).unwrap();
        migrate::configure_connection(&conn).unwrap();
        let older: Vec<_> = MIGRATIONS
            .iter()
            .copied()
            .filter(|m| m.version < 700)
            .collect();
        migrate::migrate_with(&mut conn, &older).unwrap();
        assert!(!migrate::schema_object_exists(&conn, "ai_providers").unwrap());
        conn.execute(
            "INSERT INTO ai_usage VALUES ('u1', 'p', 'm', 'explain', 3, 4, 0.5, 7)",
            [],
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    assert!(store.schema_version().unwrap() >= 700);
    let tx = store.read().unwrap();
    let prices = tx.list_ai_prices().unwrap();
    let models: Vec<&str> = prices.iter().map(|p| p.model.as_str()).collect();
    assert_eq!(
        models,
        ["claude-haiku-5-5", "claude-opus-5-5", "claude-sonnet-5-5"]
    );
    assert!(prices.iter().all(|p| p.output_per_mtok > p.input_per_mtok));
    let usage = tx.list_ai_usage(Timestamp(0)).unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].cost_estimate, Some(0.5));
    drop(tx);

    let conn = Connection::open(&path).unwrap();
    for table in ["ai_providers", "ai_routes", "ai_prices", "ai_usage"] {
        let cols: Vec<String> = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!cols.is_empty(), "{table}");
        for c in cols {
            assert!(
                !["api_key", "secret", "password", "credential"]
                    .iter()
                    .any(|w| c.contains(w))
                    && !["key", "token"].contains(&c.as_str()),
                "{table}.{c}"
            );
        }
    }
    // Kinds are constrained.
    assert!(conn
        .execute(
            "INSERT INTO ai_providers (id, kind, name, base_url, created_at, updated_at)
             VALUES ('x', 'gemini', 'x', 'x', 0, 0)",
            []
        )
        .is_err());
}

#[test]
fn providers_round_trip_order_and_trash() {
    let fx = Fx::new();
    let (a, mut b) = (provider(1, 1), provider(2, 0));
    b.kind = "ollama".into();
    b.models.clear();
    fx.write(|tx| {
        tx.upsert_ai_provider(&a).unwrap();
        tx.upsert_ai_provider(&b).unwrap();
    });
    fx.read(|r| {
        assert_eq!(
            r.list_ai_providers(false).unwrap(),
            vec![b.clone(), a.clone()]
        );
        assert_eq!(r.get_ai_provider(a.id).unwrap(), Some(a.clone()));
    });
    let mut trashed = a.clone();
    trashed.deleted_at = Some(Timestamp(20));
    trashed.name = "renamed".into();
    fx.write(|tx| tx.upsert_ai_provider(&trashed).unwrap());
    fx.read(|r| {
        assert_eq!(r.list_ai_providers(false).unwrap(), vec![b.clone()]);
        assert_eq!(r.list_ai_providers(true).unwrap().len(), 2);
        assert_eq!(r.get_ai_provider(a.id).unwrap(), Some(trashed.clone()));
    });
}

#[test]
fn routes_prices_and_usage() {
    let fx = Fx::new();
    let route = AiRouteRecord {
        feature: "check_answer".into(),
        provider_id: Some(AiProviderId(u(0x7001))),
        model: Some("claude-haiku-5-5".into()),
        updated_at: Timestamp(5),
    };
    let price = AiPriceRecord {
        model: "local-x".into(),
        input_per_mtok: 0.0,
        output_per_mtok: 0.25,
        updated_at: Timestamp(5),
    };
    fx.write(|tx| {
        tx.upsert_ai_route(&route).unwrap();
        tx.upsert_ai_route(&AiRouteRecord {
            feature: "explain".into(),
            provider_id: None,
            model: None,
            updated_at: Timestamp(6),
        })
        .unwrap();
        tx.upsert_ai_price(&price).unwrap();
        for (i, at) in [(1, 100), (2, 300), (3, 200)] {
            tx.insert_ai_usage(&AiUsageRecord {
                id: format!("u{i}"),
                provider: "p".into(),
                model: "m".into(),
                feature: "explain".into(),
                input_tokens: 10 * i,
                output_tokens: i,
                cost_estimate: None,
                created_at: Timestamp(at),
            })
            .unwrap();
        }
    });
    fx.read(|r| {
        let routes = r.list_ai_routes().unwrap();
        assert_eq!(routes[0], route);
        assert_eq!(routes[1].provider_id, None);
        assert!(r.list_ai_prices().unwrap().contains(&price));
        let ids: Vec<String> = r
            .list_ai_usage(Timestamp(150))
            .unwrap()
            .into_iter()
            .map(|u| u.id)
            .collect();
        assert_eq!(ids, ["u2", "u3"]);
    });
    fx.write(|tx| {
        assert!(tx.delete_ai_route("check_answer").unwrap());
        assert!(!tx.delete_ai_route("check_answer").unwrap());
        assert!(tx.delete_ai_price("local-x").unwrap());
        assert!(!tx.delete_ai_price("local-x").unwrap());
    });
    fx.read(|r| {
        assert_eq!(r.list_ai_routes().unwrap().len(), 1);
        assert!(!r.list_ai_prices().unwrap().contains(&price));
    });
}

/// Worker lanes: the AI lane claims only its kinds, the main lane everything else, each oldest
/// first (v0.8: AI jobs never wait behind an import).
#[test]
fn jobs_are_claimed_per_lane() {
    use crate::repo::JobRecord;
    let job = |n: u128, kind: &str, at: i64| JobRecord {
        id: fc_core::JobId(u(0x7100 + n)),
        kind: kind.into(),
        status: "queued".into(),
        progress: None,
        payload: serde_json::json!({}),
        result: None,
        error: None,
        created_at: Timestamp(at),
        updated_at: Timestamp(at),
    };
    let fx = Fx::new();
    fx.write(|tx| {
        tx.upsert_job(&job(1, "io.import", 10)).unwrap();
        tx.upsert_job(&job(2, "ai.request", 20)).unwrap();
        tx.upsert_job(&job(3, "io.export", 30)).unwrap();
        tx.upsert_job(&job(4, "ai.models", 40)).unwrap();
    });
    let ai = ["ai.request", "ai.models", "ai.source"];
    let claim = |kinds: &[&str], only: bool| {
        fx.write(|tx| tx.claim_next_job_in(Timestamp(50), kinds, only).unwrap())
            .map(|j| (j.kind, j.status))
    };
    assert_eq!(
        claim(&ai, true),
        Some(("ai.request".into(), "running".into()))
    );
    assert_eq!(
        claim(&ai, false),
        Some(("io.import".into(), "running".into()))
    );
    assert_eq!(
        claim(&ai, true),
        Some(("ai.models".into(), "running".into()))
    );
    assert_eq!(claim(&ai, true), None);
    assert_eq!(
        claim(&ai, false),
        Some(("io.export".into(), "running".into()))
    );
    assert_eq!(claim(&ai, false), None);
}
