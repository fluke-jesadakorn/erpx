//! Independent historical review is rechecked against current IAM authority.
use crate::adapters::postgres_effective_authority::lock_effective_authority_sources;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;

pub struct ReleaseReviewerRequest {
    pub company_id: CompanyId,
    pub assignment_id: RecordId,
    pub reviewer_id: UserId,
    pub requested_by: UserId,
    pub permission: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseReviewerFacts {
    pub allowed: bool,
    pub revision: String,
}

/// Before the first owner snapshot, retain every source read by the immutable
/// assignment/principal/same-person invariant functions. No Work selection is
/// fabricated for this original independent-review operation.
pub async fn lock_release_reviewer_sources(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("IAM_OWNER_TRANSACTION_REQUIRED").into());
    }
    lock_effective_authority_sources(connection).await
}
pub struct PgReleaseReviewerReader<'c> {
    connection: &'c mut PgConnection,
}

/// Scoped current person correlation only; this fact grants no permission.
/// The caller retains the same IAM/People source leases as release review.
pub struct PgSamePersonReader<'c> {
    connection: &'c mut PgConnection,
}
impl<'c> PgSamePersonReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    pub async fn same_person(
        &mut self,
        company: &CompanyId,
        left: &UserId,
        right: &UserId,
    ) -> Result<bool, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("IAM_OWNER_TRANSACTION_REQUIRED").into());
        }
        Ok(
            sqlx::query_scalar("SELECT COALESCE(iam_same_person($1,$2,$3),false)")
                .bind(left.as_str())
                .bind(right.as_str())
                .bind(company.as_str())
                .fetch_one(&mut *self.connection)
                .await?,
        )
    }
}
impl<'c> PgReleaseReviewerReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    pub async fn inspect(
        &mut self,
        request: &ReleaseReviewerRequest,
    ) -> Result<ReleaseReviewerFacts, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("IAM_OWNER_TRANSACTION_REQUIRED").into());
        }
        if request.permission.is_empty()
            || request.permission.len() > 128
            || request.permission.chars().any(char::is_control)
        {
            return Err(BusinessError::invalid(
                "IAM_PERMISSION_REQUIRED",
                "Choose a bounded canonical permission.",
            )
            .into());
        }
        // The original release predicate samples the database clock, rather
        // than trusting a caller timestamp. Tuple fences preserve 40001.
        let row=sqlx::query("WITH observed AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms), reviewer AS MATERIALIZED(SELECT a.* FROM assignments a WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.scope_kind='holding' FOR SHARE), decision AS (SELECT a.*,COALESCE(iam_assignment_active(a.id,o.now_ms) AND iam_principal_active(a.user_id,a.company_id,o.now_ms) AND iam_role_allows(a.role,a.company_id,$4) AND NOT iam_same_person(a.user_id,$5,a.company_id),false) AS allowed FROM reviewer a CROSS JOIN observed o) SELECT allowed,encode(sha256(convert_to(jsonb_build_object('assignment',to_jsonb(d)-'allowed','allowed',d.allowed,'publishedRole',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.company_id) FROM iam_roles r WHERE r.id=d.role AND r.company_id IN ('*',d.company_id)))::text,'UTF8')),'hex') revision FROM decision d")
            .bind(request.assignment_id.as_str()).bind(request.reviewer_id.as_str()).bind(request.company_id.as_str()).bind(&request.permission).bind(request.requested_by.as_str()).fetch_optional(&mut *self.connection).await?;
        row.map(|row| {
            Ok(ReleaseReviewerFacts {
                allowed: row.try_get("allowed")?,
                revision: row.try_get("revision")?,
            })
        })
        .unwrap_or_else(|| {
            Ok(ReleaseReviewerFacts {
                allowed: false,
                revision: "missing-current-reviewer".into(),
            })
        })
    }
}
