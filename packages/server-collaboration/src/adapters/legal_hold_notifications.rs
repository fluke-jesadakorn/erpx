//! Collaboration-owned Legal preservation notices. The Legal owner supplies
//! the retained notice facts in the same transaction.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalHoldNotificationFacts {
    pub hold_id: String,
    pub company_id: String,
    pub item_id: String,
    pub item_version: i64,
    pub user_id: String,
    pub notified_at: i64,
    pub message: String,
}

fn invalid() -> Error {
    PersistenceError::new("LEGAL_HOLD_NOTIFICATION_INVALID").into()
}

/// Insert the bounded set of preservation notices created by the Legal owner.
/// Existing ids and unique tuples are idempotent no-ops, matching the retained
/// Legal notifier.
pub async fn stage_legal_hold_notifications(
    c: &mut PgConnection,
    facts: &[LegalHoldNotificationFacts],
) -> Result<(), Error> {
    if !c.is_in_transaction() || facts.is_empty() || facts.len() > 5_000 {
        return Err(invalid());
    }
    for f in facts {
        if f.hold_id.is_empty()
            || f.company_id.is_empty()
            || f.item_id.is_empty()
            || f.item_version < 1
            || f.user_id.is_empty()
            || f.notified_at < 0
            || f.message.is_empty()
            || f.message.len() > 400
        {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "LEGAL_HOLD_NOTIFICATION_INVALID",
                "A preservation notice is malformed.",
            )
            .into());
        }
        sqlx::query(
            "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) \
             VALUES($1,$2,$3,'legal',$4,$5,$6,$7) ON CONFLICT DO NOTHING",
        )
        .bind(format!("hold-{}-{}-{}", f.hold_id, f.item_id, f.user_id))
        .bind(&f.company_id)
        .bind(&f.user_id)
        .bind(&f.item_id)
        .bind(f.item_version)
        .bind(&f.message)
        .bind(f.notified_at)
        .execute(&mut *c)
        .await?;
    }
    Ok(())
}
