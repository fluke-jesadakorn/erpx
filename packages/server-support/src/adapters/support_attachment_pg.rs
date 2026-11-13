//! Support-owned ticket facts for the native Files attachment intake.
//!
//! The caller owns the SERIALIZABLE unit and has already selected the current
//! Account/Holding/Company authority. This adapter only reads the exact ticket
//! row and its retained owner-eligibility predicate; it never authorizes a
//! Company, branch or person from request input.
use crate::adapters::support_ticket_read::recipient_eligibility;
use crate::domain::support_attachment::{
    SupportAttachmentActor, SupportAttachmentStage, SupportAttachmentTicket, authorize,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn serializable() -> Error {
    PersistenceError::new("SUPPORT_SERIALIZABLE_REQUIRED").into()
}

async fn require_serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(serializable());
    }
    Ok(())
}

fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_TARGET_NOT_FOUND",
        "The Support case was not found in this Company.",
    )
    .into()
}

/// Read the exact ticket facts required before Files may stage or read one
/// attachment set. The owner-eligibility predicate is evaluated in the same
/// statement so a stale owner never inherits a stage grant.
pub async fn read_attachment_ticket(
    c: &mut PgConnection,
    company_id: &str,
    ticket_id: &str,
) -> Result<SupportAttachmentTicket, Error> {
    require_serializable(c).await?;
    let sql = format!(
        "SELECT t.id,t.company_id,t.branch_id,t.department_id,t.status,t.source_kind,\
         t.owner_id,t.version,CASE WHEN {} THEN TRUE ELSE FALSE END owner_eligible \
         FROM support_tickets t WHERE t.id=$1 AND t.company_id=$2",
        recipient_eligibility()
    );
    let row: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
        i64,
        bool,
    )> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(ticket_id)
        .bind(company_id)
        .fetch_optional(&mut *c)
        .await?;
    let (
        id,
        company_id,
        branch_id,
        department_id,
        status,
        source_kind,
        owner_id,
        version,
        owner_eligible,
    ) = row.ok_or_else(missing)?;
    Ok(SupportAttachmentTicket {
        id,
        company_id,
        branch_id,
        department_id,
        status,
        source_kind,
        owner_id,
        version,
        owner_eligible,
    })
}

/// Authorize one stage/read against the current ticket facts. The returned
/// ticket remains the exact image the caller must re-check after its lock.
pub async fn authorize_support_attachment(
    c: &mut PgConnection,
    actor: &SupportAttachmentActor,
    ticket_id: &str,
    purpose: &str,
    expected_version: Option<i64>,
    stage: SupportAttachmentStage,
) -> Result<SupportAttachmentTicket, Error> {
    let ticket = read_attachment_ticket(c, &actor.company_id, ticket_id).await?;
    authorize(actor, &ticket, purpose, expected_version, stage)?;
    Ok(ticket)
}
