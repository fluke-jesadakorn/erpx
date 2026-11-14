//! Generic Files attachment remove command on the caller's serializable unit.
//!
//! Files owns only the attachment link/set lifecycle, the withdrawal audit and
//! the idempotent replay receipt. The owning-record authority is a composition
//! port and is checked before the row lock; a Company/branch/record change
//! between authorization and the lock fails closed.
use crate::application::attachment_command::{
    AttachmentWithdrawAuthority, WithdrawAttachmentRequest, WithdrawAttachmentResult,
    WithdrawTarget, prepare_withdrawal,
};
use crate::domain::attachment_command::{
    WithdrawalDecision, has_native_owner_command, normalize_withdrawal_reason, valid_command_key,
    withdrawal_event_id,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const IMAGE: &str = r#"
SELECT CASE WHEN octet_length(image::text)<=65536 THEN image::text END
FROM (
 SELECT jsonb_build_object(
  'company_id',s.company_id,
  'branch_id',s.branch_id,
  'department_id',s.department_id,
  'set_id',s.id,
  'set_state',s.state,
  'target_type',s.target_type,
  'target_id',s.target_id,
  'purpose_key',s.purpose_key,
  'component_kind',s.component_kind,
  'expected_target_version',s.expected_target_version,
  'link_id',l.id,
  'link_state',l.state,
  'created_by',l.created_by,
  'withdrawn_by',l.withdrawn_by,
  'withdrawal_reason',l.withdrawal_reason,
  'attachment_id',l.attachment_id,
  'source_valid',(l.attachment_id IS NULL OR a.id IS NOT NULL),
  'expires_at',l.expires_at,
  'bound_at',l.bound_at,
  'withdrawn_at',l.withdrawn_at
 ) image
 FROM attachment_links l
 JOIN attachment_sets s ON s.id=l.set_id
 LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
 WHERE l.id=$1 AND s.company_id=$2
"#;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct LinkImage {
    company_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
    set_id: String,
    set_state: String,
    target_type: String,
    target_id: String,
    purpose_key: String,
    component_kind: String,
    expected_target_version: i64,
    link_id: String,
    link_state: String,
    created_by: String,
    withdrawn_by: Option<String>,
    withdrawal_reason: Option<String>,
    attachment_id: Option<String>,
    source_valid: bool,
    expires_at: i64,
    bound_at: Option<i64>,
    withdrawn_at: Option<i64>,
}

fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_LINK_LOCKED",
        "This file link is closed.",
    )
    .into()
}
fn set_locked() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_SET_LOCKED",
        "This file set is already bound or closed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_LINK_LIMIT",
        "The file link exceeds its bounded command projection.",
    )
    .into()
}
fn owner_required() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_NATIVE_OWNER_REQUIRED",
        "This file belongs to a dedicated native owner command.",
    )
    .into()
}
fn receipt_missing() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_WITHDRAW_RECEIPT_MISSING",
        "The prior withdrawal receipt is unavailable. Refresh before retrying.",
    )
    .into()
}

async fn require_serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

async fn image(
    c: &mut PgConnection,
    company_id: &str,
    link_id: &str,
    lock: bool,
) -> Result<LinkImage, Error> {
    let sql = if lock {
        format!("{IMAGE} FOR UPDATE OF l,s\n) bounded")
    } else {
        format!("{IMAGE}\n) bounded")
    };
    let payload: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(link_id)
        .bind(company_id)
        .fetch_optional(&mut *c)
        .await?
        .ok_or_else(missing)?;
    serde_json::from_str(&payload.ok_or_else(limited)?).map_err(|_| limited())
}

async fn audit_intact(
    c: &mut PgConnection,
    company_id: &str,
    link_id: &str,
    actor_id: &str,
    reason: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM attachment_events WHERE company_id=$1 AND link_id=$2 \
         AND actor_id=$3 AND action='link.withdrawn' AND details_json::jsonb->>'reason'=$4)",
    )
    .bind(company_id)
    .bind(link_id)
    .bind(actor_id)
    .bind(reason)
    .fetch_one(&mut *c)
    .await?)
}

/// Withdraw one unbound Files link. The caller owns and commits the
/// SERIALIZABLE unit; this port never begins or commits a transaction and
/// cannot accept request labels, saved rows or `created_by` as authorization.
pub async fn withdraw_attachment<A: AttachmentWithdrawAuthority>(
    c: &mut PgConnection,
    authority: &A,
    request: WithdrawAttachmentRequest<'_>,
) -> Result<WithdrawAttachmentResult, Error> {
    require_serializable(c).await?;
    if !valid_command_key(request.command_key) {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "A valid Idempotency-Key is required.",
        )
        .into());
    }
    let reason = normalize_withdrawal_reason(request.reason)?;

    // Owner authority precedes the Files row lock, matching the retained
    // TypeScript command and every owner command that locks its record first.
    let before = image(c, request.company_id, request.link_id, false).await?;
    if has_native_owner_command(&before.target_type) {
        return Err(owner_required());
    }
    let target = WithdrawTarget {
        company_id: &before.company_id,
        branch_id: before.branch_id.as_deref(),
        department_id: before.department_id.as_deref(),
        link_id: &before.link_id,
        set_id: &before.set_id,
        target_type: &before.target_type,
        target_id: &before.target_id,
        purpose_key: &before.purpose_key,
        component_kind: &before.component_kind,
        expected_target_version: before.expected_target_version,
        created_by: &before.created_by,
    };
    authority.authorize_withdraw(c, &target).await?;

    // Re-read under the row lock. A target or source change between the
    // authorization read and this lock must not inherit the earlier grant.
    let locked = image(c, request.company_id, request.link_id, true).await?;
    if locked != before {
        return Err(changed());
    }
    if !locked.source_valid {
        return Err(changed());
    }
    // The owner grant is revalidated while the Files rows are locked. A grant
    // that expired, moved or lost the owner read check cannot reach a write.
    let locked_target = WithdrawTarget {
        company_id: &locked.company_id,
        branch_id: locked.branch_id.as_deref(),
        department_id: locked.department_id.as_deref(),
        link_id: &locked.link_id,
        set_id: &locked.set_id,
        target_type: &locked.target_type,
        target_id: &locked.target_id,
        purpose_key: &locked.purpose_key,
        component_kind: &locked.component_kind,
        expected_target_version: locked.expected_target_version,
        created_by: &locked.created_by,
    };
    authority.revalidate_withdraw(c, &locked_target).await?;

    let decision = prepare_withdrawal(crate::application::attachment_command::WithdrawalFacts {
        state: &locked.link_state,
        creator: &locked.created_by,
        withdrawn_by: locked.withdrawn_by.as_deref(),
        previous_reason: locked.withdrawal_reason.as_deref(),
        actor: request.actor_id,
        reason: &reason,
    })?;
    if decision == WithdrawalDecision::Replay {
        if !audit_intact(
            c,
            request.company_id,
            request.link_id,
            request.actor_id,
            &reason,
        )
        .await?
        {
            return Err(receipt_missing());
        }
        return Ok(WithdrawAttachmentResult {
            link_id: locked.link_id,
            state: "withdrawn",
            replay: true,
        });
    }
    if locked.set_state != "draft" {
        return Err(set_locked());
    }

    let updated = sqlx::query(
        "UPDATE attachment_links SET state='withdrawn',withdrawn_at=$1,withdrawn_by=$2,\
         withdrawal_reason=$3 FROM attachment_sets s WHERE attachment_links.id=$4 \
         AND attachment_links.set_id=s.id AND s.company_id=$5 AND attachment_links.state='draft' \
         AND attachment_links.created_by=$6 AND s.state='draft'",
    )
    .bind(request.now)
    .bind(request.actor_id)
    .bind(&reason)
    .bind(request.link_id)
    .bind(request.company_id)
    .bind(request.actor_id)
    .execute(&mut *c)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }

    let event_id = withdrawal_event_id(request.company_id, request.link_id, request.command_key);
    let details = serde_json::json!({"reason": reason}).to_string();
    sqlx::query(
        "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,\
         action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,'link.withdrawn',$7,$8) \
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&event_id)
    .bind(request.company_id)
    .bind(&locked.attachment_id)
    .bind(request.link_id)
    .bind(&locked.set_id)
    .bind(request.actor_id)
    .bind(&details)
    .bind(request.now)
    .execute(&mut *c)
    .await?;
    let intact: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM attachment_events WHERE id=$1 AND company_id=$2 \
         AND attachment_id IS NOT DISTINCT FROM $3 AND link_id=$4 AND set_id=$5 \
         AND actor_id=$6 AND action='link.withdrawn' AND details_json=$7 AND created_at=$8)",
    )
    .bind(&event_id)
    .bind(request.company_id)
    .bind(&locked.attachment_id)
    .bind(request.link_id)
    .bind(&locked.set_id)
    .bind(request.actor_id)
    .bind(&details)
    .bind(request.now)
    .fetch_one(&mut *c)
    .await?;
    if !intact {
        return Err(changed());
    }

    let mut expected = locked.clone();
    expected.link_state = "withdrawn".into();
    expected.withdrawn_by = Some(request.actor_id.to_owned());
    expected.withdrawal_reason = Some(reason);
    expected.withdrawn_at = Some(request.now);
    let after = image(c, request.company_id, request.link_id, false).await?;
    if after != expected {
        return Err(changed());
    }
    Ok(WithdrawAttachmentResult {
        link_id: expected.link_id,
        state: "withdrawn",
        replay: false,
    })
}
