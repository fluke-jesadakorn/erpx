use crate::{
    application::purchases::{
        CapabilityAccess, CompanyPurchaseFacts, CompanyPurchaseReadPort, PurchaseRequest,
    },
    domain::packages::{CommercialCatalogue, CompanyPackageGrant, CompanyPackageHistoryRight},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Existing writer/settlement lock-order fence, not a purchase or Identity proof.
/// Acquire alongside other owner source locks before the first Holding snapshot.
pub async fn lock_commercial_authority_sources(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCIAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE company_package_grants,company_package_history_rights IN ROW EXCLUSIVE MODE",
    )
    .execute(connection)
    .await?;
    Ok(())
}

/// Current Commercial evidence only. It supplies no personal, record or release
/// authority. All inspection/retention remains in the caller's owner transaction.
pub struct PgCompanyPurchaseReader<'c> {
    holding_id: HoldingId,
    company_id: CompanyId,
    connection: &'c mut PgConnection,
    catalogue: &'c CommercialCatalogue,
}

#[derive(Clone)]
pub struct PurchaseWitness {
    holding_id: HoldingId,
    company_id: CompanyId,
    request: PurchaseRequest,
    transaction: (String, i32, String),
    revision: String,
    token: String,
}
impl PurchaseWitness {
    pub fn as_str(&self) -> &str {
        &self.token
    }
}
pub struct PurchaseInspection {
    pub access: Vec<CapabilityAccess>,
    pub revision: String,
    pub witness: PurchaseWitness,
}
impl PurchaseInspection {
    /// A source witness also exists for an observed denial. Call this before
    /// owner effects and after retention; the witness alone never grants work.
    pub fn require_allowed(&self) -> Result<&PurchaseWitness, BusinessError> {
        if let Some(denied) = self.access.iter().find(|access| !access.allowed_by_mode) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                denied.decision.code.as_code(),
                "The Company purchase does not permit this capability purpose.",
            ));
        }
        Ok(&self.witness)
    }
}

impl<'c> PgCompanyPurchaseReader<'c> {
    pub fn new(
        holding_id: HoldingId,
        company_id: CompanyId,
        connection: &'c mut PgConnection,
        catalogue: &'c CommercialCatalogue,
    ) -> Self {
        Self {
            holding_id,
            company_id,
            connection,
            catalogue,
        }
    }
    pub async fn inspect(
        &mut self,
        request: &PurchaseRequest,
        now: TimestampMillis,
    ) -> Result<PurchaseInspection, Error> {
        if request.capabilities.is_empty()
            || request.capabilities.len() > 256
            || request
                .capabilities
                .iter()
                .any(|c| c.is_empty() || c.len() > 128)
        {
            return Err(BusinessError::invalid(
                "COMMERCIAL_CAPABILITIES_REQUIRED",
                "Choose a bounded set of owner capabilities.",
            )
            .into());
        }
        let transaction = self.transaction().await?;
        let company = self.company_id.clone();
        let facts = self.read(&company).await?;
        let revision = self.revision(&facts, now)?;
        let access = request
            .capabilities
            .iter()
            .map(|capability| {
                let decision = self.catalogue.purchased_capability(
                    &facts.grants,
                    self.company_id.as_str(),
                    capability,
                    request.purpose,
                    now.get(),
                    &facts.history,
                );
                CapabilityAccess {
                    capability: capability.clone(),
                    purpose: request.purpose,
                    decision,
                    allowed_by_mode: request.mode.permits(decision),
                }
            })
            .collect();
        let token = token(
            &self.holding_id,
            &self.company_id,
            request,
            &transaction,
            &revision,
        )?;
        Ok(PurchaseInspection {
            access,
            revision: revision.clone(),
            witness: PurchaseWitness {
                holding_id: self.holding_id.clone(),
                company_id: self.company_id.clone(),
                request: request.clone(),
                transaction,
                revision,
                token,
            },
        })
    }
    pub async fn revalidate(
        &mut self,
        witness: &PurchaseWitness,
        now: TimestampMillis,
    ) -> Result<PurchaseInspection, Error> {
        if witness.holding_id != self.holding_id
            || witness.company_id != self.company_id
            || witness.transaction != self.transaction().await?
        {
            return Err(changed().into());
        }
        let current = self.inspect(&witness.request, now).await?;
        if current.revision != witness.revision || current.witness.token != witness.token {
            return Err(changed().into());
        }
        Ok(current)
    }
    async fn transaction(&mut self) -> Result<(String, i32, String), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("COMMERCIAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("COMMERCIAL_SERIALIZABLE_REQUIRED").into());
        }
        let row=sqlx::query("SELECT current_database() AS db,pg_backend_pid() AS pid,pg_current_xact_id()::text AS tx").fetch_one(&mut *self.connection).await?;
        Ok((row.try_get("db")?, row.try_get("pid")?, row.try_get("tx")?))
    }
    fn revision(
        &self,
        facts: &CompanyPurchaseFacts,
        now: TimestampMillis,
    ) -> Result<String, Error> {
        // Clock ticks alone do not invalidate evidence. Started/expiry/retention
        // boundaries do, even when no grant writer changed a row.
        let phases = facts
            .grants
            .iter()
            .map(|g| {
                if g.starts_at > now.get() {
                    "future"
                } else if g.ends_at.is_some_and(|end| end <= now.get()) {
                    "expired"
                } else {
                    "current"
                }
            })
            .collect::<Vec<_>>();
        let retained = facts
            .history
            .iter()
            .map(|h| h.retained_at <= now.get())
            .collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&(
            "commercial-native/v1",
            self.company_id.as_str(),
            self.catalogue.definitions(),
            facts,
            phases,
            retained,
        ))
        .map_err(|_| PersistenceError::new("COMMERCIAL_REVISION_ENCODING"))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

impl CompanyPurchaseReadPort for PgCompanyPurchaseReader<'_> {
    type Error = Error;
    async fn read(&mut self, company: &CompanyId) -> Result<CompanyPurchaseFacts, Error> {
        if company != &self.company_id {
            return Err(changed().into());
        }
        self.transaction().await?;
        lock_commercial_authority_sources(self.connection).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("commercial.package:company:{}", company.as_str()))
            .execute(&mut *self.connection)
            .await?;
        // Tuple locks preserve40001 if a revocation committed after an earlier
        // owner snapshot while this transaction awaited its Company fence.
        let rows=sqlx::query("SELECT company_id,package_id,package_version::bigint AS package_version,state,starts_at,ends_at,version::bigint AS version,reason,evidence FROM company_package_grants WHERE company_id=$1 ORDER BY package_id FOR SHARE").bind(company.as_str()).fetch_all(&mut *self.connection).await?;
        let grants = rows
            .into_iter()
            .map(|r| {
                Ok(CompanyPackageGrant {
                    company_id: r.try_get("company_id")?,
                    package_id: r.try_get("package_id")?,
                    package_version: r.try_get("package_version")?,
                    state: r.try_get("state")?,
                    starts_at: r.try_get("starts_at")?,
                    ends_at: r.try_get("ends_at")?,
                    version: r.try_get("version")?,
                    reason: r.try_get("reason")?,
                    evidence: r.try_get("evidence")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let rows=sqlx::query("SELECT company_id,package_id,package_version::bigint AS package_version,source_grant_version::bigint AS source_grant_version,source_event_id,retained_at FROM company_package_history_rights WHERE company_id=$1 ORDER BY package_id,package_version FOR SHARE").bind(company.as_str()).fetch_all(&mut *self.connection).await?;
        let history = rows
            .into_iter()
            .map(|r| {
                Ok(CompanyPackageHistoryRight {
                    company_id: r.try_get("company_id")?,
                    package_id: r.try_get("package_id")?,
                    package_version: r.try_get("package_version")?,
                    source_grant_version: r.try_get("source_grant_version")?,
                    source_event_id: r.try_get("source_event_id")?,
                    retained_at: r.try_get("retained_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        Ok(CompanyPurchaseFacts { grants, history })
    }
}
fn token(
    holding: &HoldingId,
    company: &CompanyId,
    request: &PurchaseRequest,
    transaction: &(String, i32, String),
    revision: &str,
) -> Result<String, Error> {
    let bytes = serde_json::to_vec(&(
        holding.as_str(),
        company.as_str(),
        &request.capabilities,
        request.purpose,
        request.mode,
        transaction,
        revision,
    ))
    .map_err(|_| PersistenceError::new("COMMERCIAL_WITNESS_ENCODING"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCIAL_PURCHASE_CHANGED",
        "The current purchase source or transaction changed.",
    )
}
