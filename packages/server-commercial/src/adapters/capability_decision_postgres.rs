//! PostgreSQL entitlement decision source.
//!
//! The reader runs over the caller's existing `&mut PgConnection`. It never
//! opens, commits or rolls back a transaction and never takes a writer fence:
//! command callers that need one keep using the existing purchase witness /
//! package command boundary. A read here is an observation, not authority.

use crate::application::capability_decision::{
    CapabilityCheck, CapabilityDecisionObservation, CapabilityDecisionPort, CapabilityScope,
    MAX_GRANTS, MAX_HISTORY_RIGHTS, PurchasedAccess, check_purchased_capability,
    decide_effective_capability, purchased_access, purchased_accesses,
    require_purchased_capability, validate_purchase_facts,
};
use crate::application::purchases::CompanyPurchaseFacts;
use crate::domain::capability_decision::CapabilityGate;
use crate::domain::packages::{
    CapabilityPurpose, CommercialCatalogue, CompanyPackageGrant, CompanyPackageHistoryRight,
    PackageEnforcementMode,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{PgConnection, Row};
use std::collections::BTreeMap;

type Error = PgTransactionError<BusinessError>;

/// Current Commercial facts only. It supplies no personal, record or release
/// authority and creates no transaction or command witness.
pub struct PgCapabilityDecisionReader<'c> {
    company_id: CompanyId,
    connection: &'c mut PgConnection,
    catalogue: &'c CommercialCatalogue,
}

impl<'c> PgCapabilityDecisionReader<'c> {
    pub fn new(
        company_id: CompanyId,
        connection: &'c mut PgConnection,
        catalogue: &'c CommercialCatalogue,
    ) -> Self {
        Self {
            company_id,
            connection,
            catalogue,
        }
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.company_id
    }

    /// One owner-effective decision for the selected capability and purpose.
    /// The caller supplies fresh release, personal authority and record gates.
    #[allow(clippy::too_many_arguments)]
    pub async fn decide(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
        release: &CapabilityGate,
        authority: &CapabilityGate,
        record: &CapabilityGate,
    ) -> Result<CapabilityDecisionObservation, Error> {
        let facts = self.facts().await?;
        Ok(decide_effective_capability(
            self.catalogue,
            &facts,
            self.company_id.as_str(),
            capability,
            purpose,
            now,
            release,
            authority,
            record,
        )?)
    }

    /// Purchase-only command observation. Disabled mode does not read facts.
    pub async fn check(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<CapabilityCheck, Error> {
        let scope = CapabilityScope {
            company_id: self.company_id.as_str().to_owned(),
            capability: capability.to_owned(),
            purpose,
        };
        if mode == PackageEnforcementMode::Disabled {
            return Ok(check_purchased_capability(
                self.catalogue,
                &CompanyPurchaseFacts {
                    grants: Vec::new(),
                    history: Vec::new(),
                },
                &scope,
                mode,
                now,
            )?);
        }
        let facts = self.facts().await?;
        Ok(check_purchased_capability(
            self.catalogue,
            &facts,
            &scope,
            mode,
            now,
        )?)
    }

    /// Enforced mode refuses with the retained package code. Shadow never
    /// blocks; Disabled returns the explicit no-decision bypass.
    pub async fn require(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<CapabilityCheck, Error> {
        let scope = CapabilityScope {
            company_id: self.company_id.as_str().to_owned(),
            capability: capability.to_owned(),
            purpose,
        };
        if mode == PackageEnforcementMode::Disabled {
            return Ok(check_purchased_capability(
                self.catalogue,
                &CompanyPurchaseFacts {
                    grants: Vec::new(),
                    history: Vec::new(),
                },
                &scope,
                mode,
                now,
            )?);
        }
        let facts = self.facts().await?;
        Ok(require_purchased_capability(
            self.catalogue,
            &facts,
            &scope,
            mode,
            now,
        )?)
    }

    /// Grouped access for one capability. Disabled mode has revision
    /// `"disabled"` and reads no facts; Shadow observes codes but allows all.
    pub async fn access(
        &mut self,
        capability: &str,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<PurchasedAccess, Error> {
        if mode == PackageEnforcementMode::Disabled {
            return Ok(purchased_access(
                self.catalogue,
                &CompanyPurchaseFacts {
                    grants: Vec::new(),
                    history: Vec::new(),
                },
                self.company_id.as_str(),
                capability,
                mode,
                now,
            )?);
        }
        let facts = self.facts().await?;
        Ok(purchased_access(
            self.catalogue,
            &facts,
            self.company_id.as_str(),
            capability,
            mode,
            now,
        )?)
    }

    /// Related owner controls share one Company purchase/history snapshot.
    pub async fn accesses(
        &mut self,
        capabilities: &[String],
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<BTreeMap<String, PurchasedAccess>, Error> {
        if mode == PackageEnforcementMode::Disabled {
            return Ok(purchased_accesses(
                self.catalogue,
                &CompanyPurchaseFacts {
                    grants: Vec::new(),
                    history: Vec::new(),
                },
                self.company_id.as_str(),
                capabilities,
                mode,
                now,
            )?);
        }
        let facts = self.facts().await?;
        Ok(purchased_accesses(
            self.catalogue,
            &facts,
            self.company_id.as_str(),
            capabilities,
            mode,
            now,
        )?)
    }

    /// Exact current grants and retained history for the selected Company.
    /// The query is bounded and fails closed on malformed or overflowed facts.
    pub async fn facts(&mut self) -> Result<CompanyPurchaseFacts, Error> {
        let company = self.company_id.as_str();
        let rows = sqlx::query(
            "SELECT company_id,package_id,package_version::bigint AS package_version,state,starts_at,ends_at,version::bigint AS version,reason,evidence \
             FROM company_package_grants WHERE company_id=$1 ORDER BY package_id LIMIT $2",
        )
        .bind(company)
        .bind((MAX_GRANTS + 1) as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.len() > MAX_GRANTS {
            return Err(PersistenceError::new("COMMERCIAL_PURCHASE_FACTS_LIMIT").into());
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
        .bind((MAX_HISTORY_RIGHTS + 1) as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.len() > MAX_HISTORY_RIGHTS {
            return Err(PersistenceError::new("COMMERCIAL_PURCHASE_HISTORY_LIMIT").into());
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
        let facts = CompanyPurchaseFacts { grants, history };
        validate_purchase_facts(company, &facts)?;
        Ok(facts)
    }
}

impl CapabilityDecisionPort for PgCapabilityDecisionReader<'_> {
    type Error = Error;

    async fn decide(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
        release: &CapabilityGate,
        authority: &CapabilityGate,
        record: &CapabilityGate,
    ) -> Result<CapabilityDecisionObservation, Error> {
        PgCapabilityDecisionReader::decide(
            self, capability, purpose, now, release, authority, record,
        )
        .await
    }

    async fn check(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<CapabilityCheck, Error> {
        PgCapabilityDecisionReader::check(self, capability, purpose, mode, now).await
    }

    async fn require(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<CapabilityCheck, Error> {
        PgCapabilityDecisionReader::require(self, capability, purpose, mode, now).await
    }

    async fn access(
        &mut self,
        capability: &str,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<PurchasedAccess, Error> {
        PgCapabilityDecisionReader::access(self, capability, mode, now).await
    }

    async fn accesses(
        &mut self,
        capabilities: &[String],
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> Result<BTreeMap<String, PurchasedAccess>, Error> {
        PgCapabilityDecisionReader::accesses(self, capabilities, mode, now).await
    }
}
