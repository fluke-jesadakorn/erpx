//! PostgreSQL subscription/grace source.
//!
//! Uses the caller's `&mut PgConnection` and starts no transaction. It reads
//! only the retained Company package grant, history-right and grant-event
//! rows; there is no separate Commercial subscription table to fabricate.

use crate::application::subscriptions::{
    CompanySubscriptionReadPort, SubscriptionFacts, current_subscriptions,
};
use crate::domain::packages::{CompanyPackageGrant, CompanyPackageHistoryRight};
use crate::domain::subscriptions::{
    CompanySubscriptionRead, MAX_SUBSCRIPTION_HISTORY, MAX_SUBSCRIPTION_TRANSITIONS,
    MAX_SUBSCRIPTIONS, SubscriptionTransition,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgCompanySubscriptionReader<'c> {
    company_id: CompanyId,
    connection: &'c mut PgConnection,
}

impl<'c> PgCompanySubscriptionReader<'c> {
    pub fn new(company_id: CompanyId, connection: &'c mut PgConnection) -> Self {
        Self {
            company_id,
            connection,
        }
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.company_id
    }

    pub async fn inspect(
        &mut self,
        now: TimestampMillis,
    ) -> Result<CompanySubscriptionRead, Error> {
        let facts = self.facts().await?;
        Ok(current_subscriptions(&self.company_id, &facts, now)?)
    }

    /// Bounded current grant, retained history-right and grant-transition rows.
    pub async fn facts(&mut self) -> Result<SubscriptionFacts, Error> {
        let company = self.company_id.as_str();
        let rows = sqlx::query(
            "SELECT company_id,package_id,package_version::bigint AS package_version,state,starts_at,ends_at,version::bigint AS version,reason,evidence \
             FROM company_package_grants WHERE company_id=$1 ORDER BY package_id LIMIT $2",
        )
        .bind(company)
        .bind((MAX_SUBSCRIPTIONS + 1) as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.len() > MAX_SUBSCRIPTIONS {
            return Err(PersistenceError::new("COMMERCIAL_SUBSCRIPTION_LIMIT").into());
        }
        let grants = rows
            .into_iter()
            .map(|row| {
                Ok(CompanyPackageGrant {
                    company_id: row.try_get("company_id")?,
                    package_id: row.try_get("package_id")?,
                    package_version: row.try_get("package_version")?,
                    state: row.try_get("state")?,
                    starts_at: row.try_get("starts_at")?,
                    ends_at: row.try_get("ends_at")?,
                    version: row.try_get("version")?,
                    reason: row.try_get("reason")?,
                    evidence: row.try_get("evidence")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let rows = sqlx::query(
            "SELECT company_id,package_id,package_version::bigint AS package_version,source_grant_version::bigint AS source_grant_version,source_event_id,retained_at \
             FROM company_package_history_rights WHERE company_id=$1 ORDER BY package_id,package_version LIMIT $2",
        )
        .bind(company)
        .bind((MAX_SUBSCRIPTION_HISTORY + 1) as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.len() > MAX_SUBSCRIPTION_HISTORY {
            return Err(PersistenceError::new("COMMERCIAL_SUBSCRIPTION_HISTORY_LIMIT").into());
        }
        let history = rows
            .into_iter()
            .map(|row| {
                Ok(CompanyPackageHistoryRight {
                    company_id: row.try_get("company_id")?,
                    package_id: row.try_get("package_id")?,
                    package_version: row.try_get("package_version")?,
                    source_grant_version: row.try_get("source_grant_version")?,
                    source_event_id: row.try_get("source_event_id")?,
                    retained_at: row.try_get("retained_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let rows = sqlx::query(
            "SELECT company_id,package_id,after_json,created_at \
             FROM company_package_grant_events WHERE company_id=$1 ORDER BY created_at,id LIMIT $2",
        )
        .bind(company)
        .bind((MAX_SUBSCRIPTION_TRANSITIONS + 1) as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.len() > MAX_SUBSCRIPTION_TRANSITIONS {
            return Err(PersistenceError::new("COMMERCIAL_SUBSCRIPTION_TRANSITION_LIMIT").into());
        }
        let transitions = rows
            .into_iter()
            .map(|row| {
                let company_id: String = row.try_get("company_id")?;
                let package_id: String = row.try_get("package_id")?;
                let recorded_at: i64 = row.try_get("created_at")?;
                let image: serde_json::Value = row.try_get("after_json")?;
                let grant: CompanyPackageGrant = serde_json::from_value(image).map_err(|_| {
                    sqlx::Error::Protocol("COMMERCIAL_SUBSCRIPTION_EVENT_INVALID".into())
                })?;
                if company_id != company
                    || grant.company_id != company
                    || grant.package_id != package_id
                {
                    return Err(sqlx::Error::Protocol(
                        "COMMERCIAL_SUBSCRIPTION_EVENT_INVALID".into(),
                    ));
                }
                Ok(SubscriptionTransition {
                    package_id,
                    package_version: grant.package_version,
                    state: grant.state,
                    starts_at: grant.starts_at,
                    ends_at: grant.ends_at,
                    version: grant.version,
                    recorded_at,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        Ok(SubscriptionFacts {
            grants,
            history,
            transitions,
        })
    }
}

impl CompanySubscriptionReadPort for PgCompanySubscriptionReader<'_> {
    type Error = Error;

    async fn read(
        &mut self,
        company: &CompanyId,
        now: TimestampMillis,
    ) -> Result<CompanySubscriptionRead, Error> {
        if company != &self.company_id {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "COMMERCIAL_SUBSCRIPTION_CHANGED",
                "The current Commercial subscription source changed.",
            )
            .into());
        }
        self.inspect(now).await
    }
}
