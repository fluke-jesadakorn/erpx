//! Domain-grouped integration test harness. Individual cases remain in their original files.

use erpx_infrastructure::PersistenceError;
use sqlx::{PgPool, Postgres, Transaction};

mod finance_persistence {
    use super::*;

    pub(crate) async fn begin_scoped<'a>(
        pool: &'a PgPool,
        holding_ref: &str,
    ) -> Result<Transaction<'a, Postgres>, PersistenceError> {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
            .bind(holding_ref)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}

#[path = "finance_backfill_upgrade.rs"]
mod finance_backfill_upgrade;
#[path = "finance_banking_persistence.rs"]
mod finance_banking_persistence;
#[path = "finance_banking_provider.rs"]
mod finance_banking_provider;
#[path = "finance_banking_provider_accounts.rs"]
mod finance_banking_provider_accounts;
#[path = "finance_budget_authoritative_gap.rs"]
mod finance_budget_authoritative_gap;
#[path = "finance_budget_persistence.rs"]
mod finance_budget_persistence;
#[path = "finance_chart_persistence.rs"]
mod finance_chart_persistence;
#[path = "finance_chart_projection_read.rs"]
mod finance_chart_projection_read;
#[path = "finance_close_authoritative_gap.rs"]
mod finance_close_authoritative_gap;
#[path = "finance_close_control_authoritative_gap.rs"]
mod finance_close_control_authoritative_gap;
#[path = "finance_close_control_evidence.rs"]
mod finance_close_control_evidence;
#[path = "finance_close_persistence.rs"]
mod finance_close_persistence;
#[path = "finance_close_projection.rs"]
mod finance_close_projection;
#[path = "finance_consolidated_trial_balance.rs"]
mod finance_consolidated_trial_balance;
#[path = "finance_persistence.rs"]
mod finance_persistence_cases;
#[path = "finance_subledger_handoff.rs"]
mod finance_subledger_handoff;
#[path = "finance_subledger_read.rs"]
mod finance_subledger_read;
#[path = "finance_thai_tax_projection.rs"]
mod finance_thai_tax_projection;
