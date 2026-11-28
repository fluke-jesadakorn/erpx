//! Native retained-destruction execution authority.
//!
//! The retired TypeScript execution unit bound physical deletion to an exact
//! queued reviewer with a `WeakMap` lease that lived only in one owner unit.
//! This module expresses that invariant natively: an opaque, non-`Clone` unit
//! witness bound to one serializable transaction, a real `iam_same_person`
//! independent-review check, and an explicit deletion authorization that can
//! only be granted inside that unit and is rechecked immediately before the
//! irreversible Files I/O. It never grants authority by itself; the composition
//! owner still supplies the current IAM/Commercial/Files bridge through
//! `HoldingWorkAuthorizer::execute_with_sources`.

use erpx_platform_runtime_server::public::postgres::{DatabasePool, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind, IdentifierSource};
use sqlx::{Connection, PgConnection};

use crate::adapters::destruction_queued_authority::{
    CurrentLegalQueuedAuthority, read_legal_queued_authority,
};
use crate::adapters::postgres_destruction::{
    LegalDestructionJournal, PgLegalDestructionBridge, execute_legal_destruction,
    journal_legal_destruction, legal_destruction_require_review,
};
use crate::application::destruction_sources::LegalDestructionManifest;

type Error = PgTransactionError<BusinessError>;

fn independent_review_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_INDEPENDENT_REVIEW",
        "A current independent manager must authorize retained destruction work.",
    )
}

fn unit_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_RETENTION_EXECUTION_UNIT_REQUIRED",
        "Retained destruction requires its complete same-unit source lease.",
    )
}

/// Independent-review fact port. The PostgreSQL implementation calls the real
/// `iam_same_person` function inside the caller's owner transaction; keeping it
/// behind a port lets the policy be exercised without a database.
pub trait IndependentReviewFacts {
    type Error: From<BusinessError> + Send;
    fn same_person(
        &mut self,
        actor: &str,
        requester: &str,
        company: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

pub async fn require_independent_review<F: IndependentReviewFacts>(
    facts: &mut F,
    actor: &str,
    requester: &str,
    company: &str,
) -> Result<(), F::Error> {
    if actor.is_empty() || requester.is_empty() || company.is_empty() {
        return Err(independent_review_error().into());
    }
    if facts.same_person(actor, requester, company).await? {
        return Err(independent_review_error().into());
    }
    Ok(())
}

pub struct PgIndependentReview<'c> {
    connection: &'c mut PgConnection,
}
impl<'c> PgIndependentReview<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}
impl IndependentReviewFacts for PgIndependentReview<'_> {
    type Error = Error;
    async fn same_person(
        &mut self,
        actor: &str,
        requester: &str,
        company: &str,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
            .bind(actor)
            .bind(requester)
            .bind(company)
            .fetch_one(&mut *self.connection)
            .await?)
    }
}

/// One-shot, same-unit deletion authorization. It is plain state, not an owned
/// connection or a reusable grant: arming requires a live current-unit check and
/// every use demands that check again, so a stale or copied unit cannot delete.
#[derive(Default)]
struct DeletionAuthorization {
    armed: bool,
}
impl DeletionAuthorization {
    fn authorize(&mut self, current: bool) -> Result<(), BusinessError> {
        if !current {
            return Err(unit_error());
        }
        self.armed = true;
        Ok(())
    }
    fn require_armed(&self) -> Result<(), BusinessError> {
        if !self.armed {
            return Err(unit_error());
        }
        Ok(())
    }
}

/// Opaque witness that an exact destruction was read in this exact serializable
/// owner transaction. It is deliberately not `Clone`: every use rechecks the
/// backend transaction identity and the byte-identical authorization image, and
/// it cannot be carried into another unit or reused after commit.
pub struct LegalRetentionUnit {
    authority: CurrentLegalQueuedAuthority,
    company: String,
    authorization: DeletionAuthorization,
}

impl LegalRetentionUnit {
    /// Read the unit inside a serializable owner transaction that has already
    /// acquired the Legal/Files/identity fences. Missing or stale rows fail closed.
    pub async fn read(
        connection: &mut PgConnection,
        company: &str,
        destruction: &str,
    ) -> Result<Self, Error> {
        if company.is_empty() || destruction.is_empty() {
            return Err(unit_error().into());
        }
        if !connection.is_in_transaction() {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "LEGAL_OWNER_TRANSACTION_REQUIRED",
                "The retention unit must be read inside an owner transaction.",
            )
            .into());
        }
        let authority = read_legal_queued_authority(connection, company, destruction).await?;
        require_independent_review(
            &mut PgIndependentReview::new(connection),
            authority.actor_id(),
            authority.requested_by(),
            company,
        )
        .await?;
        Ok(Self {
            authority,
            company: company.to_owned(),
            authorization: DeletionAuthorization::default(),
        })
    }

    pub fn authority(&self) -> &CurrentLegalQueuedAuthority {
        &self.authority
    }

    /// Revalidate the same-unit authority and the independent reviewer. Caller
    /// must hold the originating transaction open.
    pub async fn require_current(&self, connection: &mut PgConnection) -> Result<(), Error> {
        self.authority.require_current(connection).await?;
        require_independent_review(
            &mut PgIndependentReview::new(connection),
            self.authority.actor_id(),
            self.authority.requested_by(),
            &self.company,
        )
        .await
    }

    /// Arm the one-shot deletion authorization inside this unit. Used by the
    /// composition bridge immediately before irreversible Files I/O.
    pub async fn authorize_deletion(&mut self, connection: &mut PgConnection) -> Result<(), Error> {
        self.require_current(connection).await?;
        self.authorization.authorize(true)?;
        Ok(())
    }

    /// The byte-deletion gate. A stale unit, a reused unit or an unarmed unit is
    /// refused even if an implementation still holds a connected database handle.
    pub async fn require_deletion(&self, connection: &mut PgConnection) -> Result<(), Error> {
        self.authorization.require_armed()?;
        self.require_current(connection).await
    }
}

/// Delegating bridge that consumes the same-unit deletion witness at the exact
/// byte boundary. The composition owner supplies `inner`; no TS or HTTP flag can
/// bypass this gate because the wrapper is constructed inside this owner crate.
pub struct AuthorizedLegalDestructionBridge<'u, B> {
    inner: B,
    unit: &'u LegalRetentionUnit,
}
impl<'u, B> AuthorizedLegalDestructionBridge<'u, B> {
    pub fn new(inner: B, unit: &'u LegalRetentionUnit) -> Self {
        Self { inner, unit }
    }
}
impl<B: PgLegalDestructionBridge> PgLegalDestructionBridge
    for AuthorizedLegalDestructionBridge<'_, B>
{
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        destruction: &str,
    ) -> Result<(), Error> {
        self.inner.prepare(c, company, destruction).await
    }
    async fn assert_current(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        destruction: &str,
    ) -> Result<(), Error> {
        self.inner.assert_current(c, company, destruction).await
    }
    async fn locked_source(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
        source: &str,
    ) -> Result<Option<crate::application::destruction_sources::LockedDestructionSource>, Error>
    {
        self.inner.locked_source(c, company, item, source).await
    }
    async fn link_current(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
        source: &str,
        link: &str,
    ) -> Result<bool, Error> {
        self.inner
            .link_current(c, company, item, source, link)
            .await
    }
    async fn source_shared(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        item: &str,
        source: &str,
    ) -> Result<bool, Error> {
        self.inner.source_shared(c, company, item, source).await
    }
    async fn retained_ledger(
        &mut self,
        c: &mut PgConnection,
        source: &str,
    ) -> Result<Vec<crate::application::destruction_sources::DestructionLedgerEntry>, Error> {
        self.inner.retained_ledger(c, source).await
    }
    async fn object_shared(
        &mut self,
        c: &mut PgConnection,
        source: &str,
        key: &str,
    ) -> Result<bool, Error> {
        self.inner.object_shared(c, source, key).await
    }
    async fn destroy(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        destruction: &str,
        manifest: &LegalDestructionManifest,
    ) -> Result<(), Error> {
        self.unit.require_deletion(c).await?;
        self.inner.destroy(c, company, destruction, manifest).await
    }
    async fn seal(
        &mut self,
        c: &mut PgConnection,
        company: &str,
        destruction: &str,
        manifest: &LegalDestructionManifest,
        now: i64,
    ) -> Result<(), Error> {
        self.inner
            .seal(c, company, destruction, manifest, now)
            .await
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LegalDestructionExecution {
    NoCandidate,
    Blocked,
    Journaled,
    Completed,
}

async fn require_review(pool: &DatabasePool, company: &str, destruction: &str) {
    let Ok(mut connection) = pool.raw().acquire().await else {
        return;
    };
    let Ok(mut transaction) = connection
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
    else {
        return;
    };
    let _ = legal_destruction_require_review(&mut transaction, company, destruction).await;
    let _ = transaction.commit().await;
}

/// Run the two native destruction phases, each in its own serializable unit,
/// with the same-unit authority witness and independent reviewer revalidated in
/// both. The journal commits before physical deletion; a failure after the
/// journal leaves the durable executing row for the existing retry path.
pub async fn execute_with_sources<B: PgLegalDestructionBridge>(
    pool: &DatabasePool,
    mut bridge: B,
    company: &str,
    destruction: &str,
    now: i64,
    identifiers: &mut impl IdentifierSource,
) -> Result<LegalDestructionExecution, Error> {
    let mut journal_connection = pool.raw().acquire().await?;
    let mut journal_tx = journal_connection
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await?;
    let unit = LegalRetentionUnit::read(&mut journal_tx, company, destruction).await?;
    unit.require_current(&mut journal_tx).await?;
    let event = identifiers.next();
    let journal = journal_legal_destruction(
        &mut journal_tx,
        &mut bridge,
        company,
        destruction,
        event.as_str(),
        now,
    )
    .await;
    let journaled = match journal {
        Ok(LegalDestructionJournal::Started) => {
            journal_tx.commit().await?;
            true
        }
        Ok(LegalDestructionJournal::Blocked | LegalDestructionJournal::NoChange) => {
            journal_tx.commit().await?;
            return Ok(LegalDestructionExecution::Blocked);
        }
        Err(error) => {
            drop(journal_tx);
            require_review(pool, company, destruction).await;
            return Err(error);
        }
    };
    if !journaled {
        return Ok(LegalDestructionExecution::NoCandidate);
    }

    let mut execution_connection = pool.raw().acquire().await?;
    let mut execution_tx = execution_connection
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await?;
    let executed = async {
        let mut unit = LegalRetentionUnit::read(&mut execution_tx, company, destruction).await?;
        unit.authorize_deletion(&mut execution_tx).await?;
        let event = identifiers.next();
        execute_legal_destruction(
            &mut execution_tx,
            &mut AuthorizedLegalDestructionBridge::new(bridge, &unit),
            company,
            destruction,
            event.as_str(),
            now,
        )
        .await
    }
    .await;
    match executed {
        Ok(true) => {
            execution_tx.commit().await?;
            Ok(LegalDestructionExecution::Completed)
        }
        Ok(false) => {
            execution_tx.commit().await?;
            Ok(LegalDestructionExecution::Journaled)
        }
        Err(error) => {
            drop(execution_tx);
            require_review(pool, company, destruction).await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Review {
        same: bool,
        calls: Vec<(String, String, String)>,
    }
    impl IndependentReviewFacts for Review {
        type Error = BusinessError;
        async fn same_person(
            &mut self,
            actor: &str,
            requester: &str,
            company: &str,
        ) -> Result<bool, BusinessError> {
            self.calls
                .push((actor.into(), requester.into(), company.into()));
            Ok(self.same)
        }
    }

    #[tokio::test]
    async fn independent_review_rejects_same_person_and_empty_identities() {
        let mut same = Review {
            same: true,
            calls: Vec::new(),
        };
        assert_eq!(
            require_independent_review(&mut same, "reviewer", "requester", "company")
                .await
                .unwrap_err()
                .code,
            "LEGAL_INDEPENDENT_REVIEW"
        );
        assert_eq!(
            same.calls,
            vec![("reviewer".into(), "requester".into(), "company".into())]
        );
        let mut distinct = Review {
            same: false,
            calls: Vec::new(),
        };
        assert!(
            require_independent_review(&mut distinct, "reviewer", "requester", "company")
                .await
                .is_ok()
        );
        assert!(
            require_independent_review(&mut distinct, "", "requester", "company")
                .await
                .is_err()
        );
        assert!(
            require_independent_review(&mut distinct, "reviewer", "", "company")
                .await
                .is_err()
        );
        assert!(
            require_independent_review(&mut distinct, "reviewer", "requester", "")
                .await
                .is_err()
        );
        assert_eq!(
            distinct.calls.len(),
            1,
            "empty identities never reach the database"
        );
    }

    #[test]
    fn deletion_authorization_is_one_shot_and_same_unit_only() {
        let mut authorization = DeletionAuthorization::default();
        assert_eq!(
            authorization.require_armed().unwrap_err().code,
            "LEGAL_RETENTION_EXECUTION_UNIT_REQUIRED"
        );
        // A stale or foreign unit cannot arm deletion at all.
        assert_eq!(
            authorization.authorize(false).unwrap_err().code,
            "LEGAL_RETENTION_EXECUTION_UNIT_REQUIRED"
        );
        assert!(authorization.require_armed().is_err());
        authorization.authorize(true).unwrap();
        assert!(authorization.require_armed().is_ok());
        // The authorization never escapes as a reusable grant: a new unit starts
        // unarmed even though the previous one was armed in its own transaction.
        let other = DeletionAuthorization::default();
        assert!(other.require_armed().is_err());
    }

    // A same-company identity alias is resolved by iam_same_person, not by raw
    // string inequality. The port receives the exact Company scope so the SQL
    // adapter cannot accidentally compare identities from another Company.
    #[tokio::test]
    async fn same_person_check_is_company_scoped() {
        let mut review = Review {
            same: false,
            calls: Vec::new(),
        };
        require_independent_review(&mut review, "actor", "requester", "company-a")
            .await
            .unwrap();
        require_independent_review(&mut review, "actor", "requester", "company-b")
            .await
            .unwrap();
        assert_eq!(review.calls[0].2, "company-a");
        assert_eq!(review.calls[1].2, "company-b");
    }
}
