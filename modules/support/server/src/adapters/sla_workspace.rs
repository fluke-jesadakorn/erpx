//! PostgreSQL adapter for the native Support SLA workspace read.
//!
//! Every port executes a real, bound statement on the caller's already-open
//! serializable Holding unit through the owner read functions in
//! `support_ticket_read`. The adapter maps rows into domain facts and never
//! opens, commits or retries a transaction and never shapes a response here.
use crate::adapters::support_ticket_read as owner_read;
use crate::application::sla_workspace::{SlaWorkspaceFuture, SlaWorkspacePorts};
use crate::domain::sla_workspace::{SlaPolicyRow, SlaTicketRow, SlaWorkspaceScope};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_SLA_UNAVAILABLE",
            "The Support SLA workspace is unavailable.",
        ),
    }
}

/// One bounded read unit bound to the caller's connection, selected Support
/// scope and `support.sla` purchase gates.
pub struct PgSlaWorkspace<'c> {
    connection: &'c mut PgConnection,
    read: owner_read::SupportReadScope,
    actor: SlaWorkspaceScope,
    now: i64,
}

impl<'c> PgSlaWorkspace<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        read: owner_read::SupportReadScope,
        new_work: bool,
        settle_existing: bool,
        now: i64,
    ) -> Self {
        let actor = SlaWorkspaceScope {
            company_id: read.company_id.as_str().to_owned(),
            user_id: read.user_id.clone(),
            branch_id: read.branch_id.clone(),
            department_id: read.department_id.clone(),
            support_manage: read.support_manage,
            support_read: read.support_read,
            new_work,
            settle_existing,
        };
        Self {
            connection,
            read,
            actor,
            now,
        }
    }

    pub fn actor(&self) -> SlaWorkspaceScope {
        self.actor.clone()
    }
}

impl SlaWorkspacePorts for PgSlaWorkspace<'_> {
    fn now(&self) -> i64 {
        self.now
    }

    /// Executes `read_sla_policy_records`: Company policies joined to the exact
    /// independent-reviewer eligibility predicate, with `actor_eligible` bound
    /// to the current user.
    fn policies<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<SlaPolicyRow>> {
        Box::pin(async move {
            let read = self.read.clone();
            let rows = owner_read::read_sla_policy_records(
                &mut *self.connection,
                &read.company_id,
                &read.user_id,
            )
            .await
            .map_err(business)?;
            Ok(rows
                .into_iter()
                .map(|row| SlaPolicyRow {
                    id: row.id,
                    name: row.name,
                    version: row.version,
                    status: row.status,
                    created_by: row.created_by,
                    policy_json: row.policy_json,
                    actor_eligible: row.actor_eligible,
                })
                .collect())
        })
    }

    /// Executes `read_sla_tickets`: active SLA clocks in the selected scope.
    fn tickets<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<SlaTicketRow>> {
        Box::pin(async move {
            let read = self.read.clone();
            let rows = owner_read::read_sla_tickets(&mut *self.connection, &read)
                .await
                .map_err(business)?;
            Ok(rows
                .into_iter()
                .map(|row| SlaTicketRow {
                    id: row.id,
                    title: row.title,
                    status: row.status,
                    owner_id: row.owner_id,
                    policy_id: row.sla_policy_id,
                    policy_version: row.sla_policy_version,
                    clock_json: Some(row.sla_clock_json),
                })
                .collect())
        })
    }

    /// Executes `read_sla_alerts`: breach alerts with the exact acknowledgement
    /// and recipient facts for the current actor.
    fn alerts<'a>(&'a mut self) -> SlaWorkspaceFuture<'a, Vec<Value>> {
        Box::pin(async move {
            let read = self.read.clone();
            owner_read::read_sla_alerts(&mut *self.connection, &read)
                .await
                .map_err(business)
        })
    }
}

/// Composition entry point: plans nothing and keeps no TypeScript fallback.
/// The caller supplies the already-authorized serializable unit, current
/// Support scope, `support.sla` purchase gates and clock instant.
pub async fn load_sla_workspace(
    connection: &mut PgConnection,
    read: owner_read::SupportReadScope,
    new_work: bool,
    settle_existing: bool,
    now: i64,
) -> Result<Value, BusinessError> {
    let mut ports = PgSlaWorkspace::new(connection, read, new_work, settle_existing, now);
    let actor = ports.actor();
    let view = crate::application::sla_workspace::read_sla_workspace(&mut ports, &actor).await?;
    serde_json::to_value(view).map_err(|_| {
        BusinessError::invalid("SUPPORT_SLA_RESPONSE_INVALID", "Invalid Support SLA response.")
    })
}
