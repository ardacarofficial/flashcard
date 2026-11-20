//! AI providers, routes, prices and usage (migration 0700).

use super::{all, exec, id, json, one, opt_id, opt_ms, opt_ts, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{AiPriceRecord, AiProviderRecord, AiRead, AiRouteRecord, AiUsageRecord, AiWrite};
use fc_core::{AiProviderId, Timestamp};
use rusqlite::{params, Row};

const P: &str = "ai_providers";
const P_COLS: &str = "id, kind, name, base_url, default_model, models, is_default, position, \
                      created_at, updated_at, deleted_at";
const R: &str = "ai_routes";
const PR: &str = "ai_prices";
const U: &str = "ai_usage";
const U_COLS: &str =
    "id, provider, model, feature, input_tokens, output_tokens, cost_estimate, created_at";

fn provider_row(r: &Row<'_>) -> rusqlite::Result<AiProviderRecord> {
    Ok(AiProviderRecord {
        id: id(r, 0)?,
        kind: r.get(1)?,
        name: r.get(2)?,
        base_url: r.get(3)?,
        default_model: r.get(4)?,
        models: json(r, 5)?,
        is_default: r.get(6)?,
        position: r.get(7)?,
        created_at: ts(r, 8)?,
        updated_at: ts(r, 9)?,
        deleted_at: opt_ts(r, 10)?,
    })
}

fn usage_row(r: &Row<'_>) -> rusqlite::Result<AiUsageRecord> {
    Ok(AiUsageRecord {
        id: r.get(0)?,
        provider: r.get(1)?,
        model: r.get(2)?,
        feature: r.get(3)?,
        input_tokens: r.get(4)?,
        output_tokens: r.get(5)?,
        cost_estimate: r.get(6)?,
        created_at: ts(r, 7)?,
    })
}

impl<M: Mode> AiRead for SqliteTx<'_, M> {
    fn get_ai_provider(&self, pid: AiProviderId) -> StorageResult<Option<AiProviderRecord>> {
        one(
            self.conn(),
            P,
            &format!("SELECT {P_COLS} FROM ai_providers WHERE id = ?1"),
            [pid.to_string()],
            provider_row,
        )
    }

    fn list_ai_providers(&self, include_deleted: bool) -> StorageResult<Vec<AiProviderRecord>> {
        all(
            self.conn(),
            P,
            &format!(
                "SELECT {P_COLS} FROM ai_providers WHERE ?1 OR deleted_at IS NULL
                 ORDER BY position, created_at, id"
            ),
            [include_deleted],
            provider_row,
        )
    }

    fn list_ai_routes(&self) -> StorageResult<Vec<AiRouteRecord>> {
        all(
            self.conn(),
            R,
            "SELECT feature, provider_id, model, updated_at FROM ai_routes ORDER BY feature",
            [],
            |r| {
                Ok(AiRouteRecord {
                    feature: r.get(0)?,
                    provider_id: opt_id(r, 1)?,
                    model: r.get(2)?,
                    updated_at: ts(r, 3)?,
                })
            },
        )
    }

    fn list_ai_prices(&self) -> StorageResult<Vec<AiPriceRecord>> {
        all(
            self.conn(),
            PR,
            "SELECT model, input_per_mtok, output_per_mtok, updated_at FROM ai_prices
             ORDER BY model",
            [],
            |r| {
                Ok(AiPriceRecord {
                    model: r.get(0)?,
                    input_per_mtok: r.get(1)?,
                    output_per_mtok: r.get(2)?,
                    updated_at: ts(r, 3)?,
                })
            },
        )
    }

    fn list_ai_usage(&self, since: Timestamp) -> StorageResult<Vec<AiUsageRecord>> {
        all(
            self.conn(),
            U,
            &format!(
                "SELECT {U_COLS} FROM ai_usage WHERE created_at >= ?1
                 ORDER BY created_at DESC, id DESC"
            ),
            [since.0],
            usage_row,
        )
    }
}

impl AiWrite for SqliteTx<'_, WriteMode> {
    fn upsert_ai_provider(&mut self, p: &AiProviderRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            P,
            &format!(
                "INSERT INTO ai_providers ({P_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, name = excluded.name,
                     base_url = excluded.base_url, default_model = excluded.default_model,
                     models = excluded.models, is_default = excluded.is_default,
                     position = excluded.position, created_at = excluded.created_at,
                     updated_at = excluded.updated_at, deleted_at = excluded.deleted_at"
            ),
            params![
                p.id.to_string(),
                p.kind,
                p.name,
                p.base_url,
                p.default_model,
                to_json(P, &p.models)?,
                p.is_default,
                p.position,
                p.created_at.0,
                p.updated_at.0,
                opt_ms(p.deleted_at),
            ],
        )?;
        Ok(())
    }

    fn upsert_ai_route(&mut self, r: &AiRouteRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            R,
            "INSERT INTO ai_routes (feature, provider_id, model, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(feature) DO UPDATE SET provider_id = excluded.provider_id,
                 model = excluded.model, updated_at = excluded.updated_at",
            params![
                r.feature,
                r.provider_id.map(|p| p.to_string()),
                r.model,
                r.updated_at.0
            ],
        )?;
        Ok(())
    }

    fn delete_ai_route(&mut self, feature: &str) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            R,
            "DELETE FROM ai_routes WHERE feature = ?1",
            [feature],
        )? > 0)
    }

    fn upsert_ai_price(&mut self, p: &AiPriceRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            PR,
            "INSERT INTO ai_prices (model, input_per_mtok, output_per_mtok, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(model) DO UPDATE SET input_per_mtok = excluded.input_per_mtok,
                 output_per_mtok = excluded.output_per_mtok, updated_at = excluded.updated_at",
            params![p.model, p.input_per_mtok, p.output_per_mtok, p.updated_at.0],
        )?;
        Ok(())
    }

    fn delete_ai_price(&mut self, model: &str) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            PR,
            "DELETE FROM ai_prices WHERE model = ?1",
            [model],
        )? > 0)
    }

    fn insert_ai_usage(&mut self, u: &AiUsageRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            U,
            &format!("INSERT INTO ai_usage ({U_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
            params![
                u.id,
                u.provider,
                u.model,
                u.feature,
                u.input_tokens,
                u.output_tokens,
                u.cost_estimate,
                u.created_at.0
            ],
        )?;
        Ok(())
    }
}
