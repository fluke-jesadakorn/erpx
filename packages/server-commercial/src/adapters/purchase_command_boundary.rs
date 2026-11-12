//! Native Company purchase-command boundary.
//!
//! The boundary attaches to an owner's existing atomic serializable Holding
//! transaction, before that owner takes its own aggregate/IAM locks. It owns
//! only the Company package fence, the retained `command_assertions` guards and
//! purchase error mapping; actor, release, record and receipt policy remain
//! owner responsibilities. Disabled purchase mode takes no lock, reads no clock
//! and writes no assertion.
use super::{
    postgres_purchases::PgCompanyPurchaseReader,
    purchased_capability_sql::{
        PurchaseShadowCheckedObservation, decision_and_observation, js_number_grants,
        js_number_history, push_capability_predicate, push_shadow_facts,
    },
};
use crate::{
    application::purchases::CompanyPurchaseReadPort,
    domain::packages::{
        CapabilityPurpose, CommercialCatalogue, PackageDecision, PackageEnforcementMode,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use sqlx::{Connection, PgConnection, Postgres, QueryBuilder, Row};

type Error = PgTransactionError<BusinessError>;

/// Retained step names. `purchaseCommandFailure` recognises this prefix in the
/// check-constraint detail, so owners can keep one mapping helper.
pub const PURCHASE_BEFORE_STEP: &str = "commercial-purchase-before";
pub const PURCHASE_AFTER_STEP: &str = "commercial-purchase-after";
const MAX_COMMAND_KEY: usize = 200;
const MAX_CAPABILITY: usize = 128;

/// One owner command's Company capability scope. The caller supplies a trusted
/// Company and capability; neither is derived from request SQL.
#[derive(Clone, Debug)]
pub struct PurchaseCommandScope {
    pub company_id: CompanyId,
    pub capability: String,
    pub purpose: CapabilityPurpose,
}

/// Post-phase result. It is the purchase decision for the caller's telemetry
/// and never personal authorization. Shadow telemetry that cannot be
/// interpreted yields `None`; it never fails the owner command.
#[derive(Clone, Debug)]
pub struct PurchaseCommandReceipt {
    pub decision: Option<PackageDecision>,
    pub observation: Option<PurchaseShadowCheckedObservation>,
}

/// One guarded purchase boundary. `new` returns `None` for disabled mode, so a
/// caller can only reach the lock/assertion path in shadow or enforced mode.
#[derive(Debug)]
pub struct PurchaseCommandBoundary {
    key: String,
    scope: PurchaseCommandScope,
    mode: PackageEnforcementMode,
    unit: Option<(String, i32, String)>,
}

impl PurchaseCommandBoundary {
    /// Build the boundary. Validation is bounded and side-effect free; the
    /// owner still needs the same serializable transaction it already uses.
    pub fn new(
        mode: PackageEnforcementMode,
        key: impl Into<String>,
        scope: PurchaseCommandScope,
    ) -> Result<Option<Self>, BusinessError> {
        if mode == PackageEnforcementMode::Disabled {
            return Ok(None);
        }
        let key = key.into();
        if key.is_empty()
            || key.len() > MAX_COMMAND_KEY
            || scope.capability.is_empty()
            || scope.capability.len() > MAX_CAPABILITY
        {
            return Err(BusinessError::invalid(
                "COMMERCIAL_PURCHASE_COMMAND_REQUIRED",
                "A bounded command key and owner capability are required.",
            ));
        }
        Ok(Some(Self {
            key,
            scope,
            mode,
            unit: None,
        }))
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn mode(&self) -> PackageEnforcementMode {
        self.mode
    }

    /// Take the retained Company package locks and, in enforced mode, the
    /// before-guard. Call this before any owner/IAM/aggregate lock.
    ///
    /// The Company package decision comes from `PgCompanyPurchaseReader` and
    /// the shared catalogue, so a denial is the same 409 the post-effects
    /// guard raises. The `command_assertions` row is still written so an
    /// enclosing owner batch sees the retained guard semantics.
    pub async fn begin(
        &mut self,
        connection: &mut PgConnection,
        holding: &HoldingId,
        catalogue: &CommercialCatalogue,
    ) -> Result<(), Error> {
        lock_company_purchase_read_sources(connection, &self.scope.company_id).await?;
        self.unit = Some(unit(connection).await?);
        if self.mode == PackageEnforcementMode::Enforced {
            let now = sample_clock(connection).await?;
            let decision = self.decide(connection, holding, catalogue, now).await?;
            if !decision.allowed {
                return Err(package_changed().into());
            }
            self.assert_guard(connection, catalogue, PURCHASE_BEFORE_STEP, now)
                .await?;
            require_unit(connection, self.unit.as_ref()).await?;
        }
        Ok(())
    }

    /// Write the post-effects guard. In enforced mode the decision is
    /// re-evaluated against current facts and then persisted as the
    /// `commercial-purchase-after` assertion. In shadow mode it emits the
    /// retained checked-phase observation from the same transaction.
    ///
    /// Call `cleanup` (or `complete`) after this and before commit so the
    /// transaction-local assertions do not outlive the command.
    pub async fn finish(
        &mut self,
        connection: &mut PgConnection,
        holding: &HoldingId,
        catalogue: &CommercialCatalogue,
    ) -> Result<PurchaseCommandReceipt, Error> {
        if self.unit.is_none() {
            return Err(PersistenceError::new("COMMERCIAL_PURCHASE_COMMAND_ORDER").into());
        }
        require_unit(connection, self.unit.as_ref()).await?;
        match self.mode {
            PackageEnforcementMode::Disabled => Ok(PurchaseCommandReceipt {
                decision: None,
                observation: None,
            }),
            PackageEnforcementMode::Shadow => {
                let observed = self.observe(connection, catalogue).await?;
                require_unit(connection, self.unit.as_ref()).await?;
                Ok(match observed {
                    Some((decision, observation)) => PurchaseCommandReceipt {
                        decision: Some(decision),
                        observation: Some(observation),
                    },
                    None => PurchaseCommandReceipt {
                        decision: None,
                        observation: None,
                    },
                })
            }
            PackageEnforcementMode::Enforced => {
                let now = sample_clock(connection).await?;
                let decision = self.decide(connection, holding, catalogue, now).await?;
                if !decision.allowed {
                    return Err(package_changed().into());
                }
                self.assert_guard(connection, catalogue, PURCHASE_AFTER_STEP, now)
                    .await?;
                require_unit(connection, self.unit.as_ref()).await?;
                Ok(PurchaseCommandReceipt {
                    decision: Some(decision),
                    observation: None,
                })
            }
        }
    }

    /// Finish and remove this command's assertions. Shadow mode has no
    /// assertion rows, so it is a no-op there.
    pub async fn complete(
        &mut self,
        connection: &mut PgConnection,
        holding: &HoldingId,
        catalogue: &CommercialCatalogue,
    ) -> Result<PurchaseCommandReceipt, Error> {
        let receipt = self.finish(connection, holding, catalogue).await?;
        self.cleanup(connection).await?;
        Ok(receipt)
    }

    /// Remove only this boundary's transaction-local assertions.
    pub async fn cleanup(&self, connection: &mut PgConnection) -> Result<(), Error> {
        if self.mode != PackageEnforcementMode::Enforced {
            return Ok(());
        }
        sqlx::query("DELETE FROM command_assertions WHERE command_id=$1")
            .bind(&self.key)
            .execute(&mut *connection)
            .await?;
        Ok(())
    }

    async fn decide(
        &self,
        connection: &mut PgConnection,
        holding: &HoldingId,
        catalogue: &CommercialCatalogue,
        now: i64,
    ) -> Result<PackageDecision, Error> {
        let facts = {
            let mut reader = PgCompanyPurchaseReader::new(
                holding.clone(),
                self.scope.company_id.clone(),
                connection,
                catalogue,
            );
            let facts = reader.read(&self.scope.company_id).await?;
            drop(reader);
            facts
        };
        let mut grants = facts.grants;
        let mut history = facts.history;
        js_number_grants(&mut grants);
        js_number_history(&mut history);
        Ok(catalogue.purchased_capability(
            &grants,
            self.scope.company_id.as_str(),
            &self.scope.capability,
            self.scope.purpose,
            (now as f64) as i64,
            &history,
        ))
    }

    async fn assert_guard(
        &self,
        connection: &mut PgConnection,
        catalogue: &CommercialCatalogue,
        step: &'static str,
        now: i64,
    ) -> Result<(), Error> {
        let mut query = QueryBuilder::<Postgres>::new(
            "WITH purchase_scope AS MATERIALIZED (SELECT ",
        );
        query
            .push_bind(self.scope.company_id.as_str().to_owned())
            .push("::text company_id, ")
            .push_bind(now)
            .push("::bigint now_ms) INSERT INTO command_assertions(command_id,step,ok) SELECT ")
            .push_bind(self.key.clone())
            .push("::text,")
            .push_bind(step)
            .push("::text,CAST(CASE WHEN ");
        push_capability_predicate(
            &mut query,
            catalogue,
            &self.scope.capability,
            self.scope.purpose,
        );
        query.push(" THEN 1 ELSE 0 END AS bigint) FROM purchase_scope s");
        query.build().execute(&mut *connection).await.map_err(map_sql)?;
        Ok(())
    }

    async fn observe(
        &self,
        connection: &mut PgConnection,
        catalogue: &CommercialCatalogue,
    ) -> Result<Option<(PackageDecision, PurchaseShadowCheckedObservation)>, Error> {
        let mut query = QueryBuilder::<Postgres>::new(
            "WITH purchase_scope AS MATERIALIZED (SELECT ",
        );
        query
            .push_bind(self.scope.company_id.as_str().to_owned())
            .push("::text company_id, floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT s.now_ms, ")
            .push("(");
        push_capability_predicate(
            &mut query,
            catalogue,
            &self.scope.capability,
            self.scope.purpose,
        );
        query.push(") allowed");
        push_shadow_facts(&mut query);
        query.push(" FROM purchase_scope s");
        let row = query.build().fetch_one(&mut *connection).await?;
        let sampled_at: i64 = row.try_get("now_ms")?;
        let grants: serde_json::Value = row.try_get("grants")?;
        let history: serde_json::Value = row.try_get("history")?;
        Ok(decision_and_observation(
            self.scope.company_id.as_str(),
            &self.scope.capability,
            self.scope.purpose,
            sampled_at,
            catalogue,
            grants,
            history,
        ))
    }
}

/// Retained Company writer order: package fence, Company advisory, then the
/// grant rows. Call before any owner/IAM/aggregate lock.
pub async fn lock_company_purchase_read_sources(
    connection: &mut PgConnection,
    company: &CompanyId,
) -> Result<(), Error> {
    require_transaction(connection)?;
    sqlx::query("LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE")
        .execute(&mut *connection)
        .await?;
    lock_company_purchase_advisory_sources(connection, company).await
}

/// Read-only Company fence for callers that already hold the writer fence or
/// must not take it: advisory lock then grant rows in the retained order.
pub async fn lock_company_purchase_advisory_sources(
    connection: &mut PgConnection,
    company: &CompanyId,
) -> Result<(), Error> {
    require_transaction(connection)?;
    let key = format!("commercial.package:company:{}", company.as_str());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut *connection)
        .await?;
    sqlx::query("SELECT package_id FROM company_package_grants WHERE company_id=$1 ORDER BY package_id FOR SHARE")
        .bind(company.as_str())
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Holding-wide owner sweep in the same order as normal Company writers.
/// Follow with locks on the owner's source rows before evaluating effects.
pub async fn lock_holding_purchase_read_sources(
    connection: &mut PgConnection,
) -> Result<(), Error> {
    require_transaction(connection)?;
    sqlx::query("LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE")
        .execute(&mut *connection)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:'||company_id,0)) FROM (SELECT DISTINCT company_id FROM company_package_grants ORDER BY company_id) companies")
        .execute(&mut *connection)
        .await?;
    sqlx::query("SELECT company_id,package_id FROM company_package_grants ORDER BY company_id,package_id FOR SHARE")
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Owner-visible mapping for a retained purchase assertion failure. It matches
/// `purchaseCommandFailure`: SQLSTATE `23514` whose detail carries a
/// `commercial-purchase-` step becomes the reload-and-review 409.
pub fn purchase_command_failure(error: &sqlx::Error) -> Option<BusinessError> {
    let database = error.as_database_error()?;
    if database.code().as_deref() != Some("23514") {
        return None;
    }
    let detail = database
        .try_downcast_ref::<sqlx::postgres::PgDatabaseError>()?
        .detail()?;
    detail
        .contains(", commercial-purchase-")
        .then(package_changed)
}

fn map_sql(error: sqlx::Error) -> Error {
    match purchase_command_failure(&error) {
        Some(failure) => Error::Business(failure),
        None => error.into(),
    }
}

fn package_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PACKAGE_CAPABILITY_CHANGED",
        "Package access changed. Reload and review before retrying.",
    )
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCIAL_PURCHASE_CHANGED",
        "The current purchase source or transaction changed.",
    )
}
fn require_transaction(connection: &PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCIAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
async fn sample_clock(connection: &mut PgConnection) -> Result<i64, Error> {
    Ok(
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(&mut *connection)
            .await?,
    )
}
async fn unit(connection: &mut PgConnection) -> Result<(String, i32, String), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("COMMERCIAL_SERIALIZABLE_REQUIRED").into());
    }
    Ok(
        sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text")
            .fetch_one(&mut *connection)
            .await?,
    )
}
async fn require_unit(
    connection: &mut PgConnection,
    expected: Option<&(String, i32, String)>,
) -> Result<(), Error> {
    let expected =
        expected.ok_or_else(|| PersistenceError::new("COMMERCIAL_PURCHASE_COMMAND_ORDER"))?;
    if unit(connection).await? != *expected {
        return Err(changed().into());
    }
    Ok(())
}
