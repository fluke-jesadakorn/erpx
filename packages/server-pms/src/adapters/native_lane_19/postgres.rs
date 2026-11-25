//! sqlx executor for the lane 19 PMS evidence read.
//!
//! The statements start byte-identical to the retired TypeScript adapter (with
//! `?` placeholders rendered as positional `$n` binds). Company and branch are
//! owned by the owner router and bound as exact scoped values; the projection
//! and legacy terminology calculation are performed here, never in TypeScript.

use std::sync::Arc;

use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::Row;

use super::application::{GuestAccountEvidenceRead, Lane19ReadFuture};
use super::domain::{
    EntryEvidence, EvidenceRequest, GuestAccountEvidence, canonical_guest_account, unavailable,
};

/// Byte-identical account point read from `guest-account-finance-evidence.ts`
/// rendered through `pmsRecordPropertyPointHeadSql('guestEvidence')`, with `?`
/// replaced by positional binds in the original order.
pub const ACCOUNT_SQL: &str = "SELECT r.id,r.property_id,r.data_json,p.name,p.business_date,p.config_json::jsonb->>'currency' currency\n    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=$1 AND r.kind='guest_account'\n    AND p.company_id=$2 AND ($3::text IS NULL OR p.branch_id=$4)";

/// Byte-identical entries read from `guest-account-finance-evidence.ts` with
/// `?` replaced by positional binds in the original order.
pub const ENTRIES_SQL: &str = "SELECT e.id,e.data_json,pp.journal_id FROM pms_records e LEFT JOIN pms_postings pp ON pp.property_id=e.property_id AND pp.record_id=e.id\n    WHERE e.property_id=$1 AND e.kind='entry' AND e.data_json::jsonb->>'guestAccountId'=$2 ORDER BY e.updated_at,e.id";

#[derive(Clone, Debug)]
struct AccountRow {
    id: String,
    property_id: String,
    data_json: String,
    name: String,
    business_date: String,
    currency: String,
}

#[derive(Clone, Debug)]
struct EntryRow {
    id: String,
    data_json: String,
    journal_id: Option<String>,
}

/// Pure projection from owner rows to the pinned response shape. Kept separate
/// from sqlx so the result oracle can be exercised without a database.
fn project(account: AccountRow, entries: Vec<EntryRow>) -> Result<GuestAccountEvidence, BusinessError> {
    let account_data: Value = serde_json::from_str(&account.data_json).map_err(|_| unavailable())?;
    let account_data = canonical_guest_account(account_data)?;
    let entries = entries
        .into_iter()
        .map(|entry| {
            let data: Value = serde_json::from_str(&entry.data_json).map_err(|_| unavailable())?;
            Ok(EntryEvidence {
                id: entry.id,
                journal_id: entry.journal_id,
                data: canonical_guest_account(data)?,
            })
        })
        .collect::<Result<Vec<_>, BusinessError>>()?;
    Ok(GuestAccountEvidence {
        id: account.id,
        property_id: account.property_id,
        hotel: account.name,
        business_date: account.business_date,
        currency: account.currency,
        account: account_data,
        entries,
    })
}

pub struct PgGuestAccountEvidenceRead {
    pool: DatabasePool,
}

impl PgGuestAccountEvidenceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl GuestAccountEvidenceRead for PgGuestAccountEvidenceRead {
    fn evidence<'a>(
        &'a self,
        request: &'a EvidenceRequest,
    ) -> Lane19ReadFuture<'a, Option<GuestAccountEvidence>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = request.company.clone();
        let branch = request.branch.clone();
        let id = request.id.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch = branch.clone();
                let id = id.clone();
                Box::pin(async move {
                    let account = sqlx::query(ACCOUNT_SQL)
                        .bind(id.as_str())
                        .bind(company.as_str())
                        .bind(branch.as_deref())
                        .bind(branch.as_deref())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let Some(account) = account else {
                        return Ok(None);
                    };
                    let property_id = account
                        .try_get::<String, _>("property_id")
                        .map_err(|_| unavailable())?;
                    let account_row = AccountRow {
                        id: account.try_get::<String, _>("id").map_err(|_| unavailable())?,
                        property_id: property_id.clone(),
                        data_json: account.try_get::<String, _>("data_json").map_err(|_| unavailable())?,
                        name: account.try_get::<String, _>("name").map_err(|_| unavailable())?,
                        business_date: account
                            .try_get::<String, _>("business_date")
                            .map_err(|_| unavailable())?,
                        currency: account.try_get::<String, _>("currency").map_err(|_| unavailable())?,
                    };
                    let rows = sqlx::query(ENTRIES_SQL)
                        .bind(property_id.as_str())
                        .bind(id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let entries = rows
                        .into_iter()
                        .map(|row| {
                            Ok(EntryRow {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                data_json: row.try_get::<String, _>("data_json").map_err(|_| unavailable())?,
                                journal_id: row.try_get::<Option<String>, _>("journal_id").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(project(account_row, entries).map(Some)?)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

/// Convenience constructor used by the mounted owner router.
pub fn reader(pool: DatabasePool) -> Arc<dyn GuestAccountEvidenceRead> {
    Arc::new(PgGuestAccountEvidenceRead::new(pool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    pub(crate) fn sha256(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn sql_oracles_are_byte_exact() {
        assert_eq!(sha256(ACCOUNT_SQL), "b225b7a8d0b1df87772c8adc7dee240fb75b950e69b8988b4831323bd7b31132");
        assert_eq!(sha256(ENTRIES_SQL), "8be96a1f25320ba1d72d8bba865f4a115cad5c94c6cef953d3ef487d6a14b43f");
    }

    #[test]
    fn result_oracle_is_byte_exact() {
        let account = AccountRow {
            id: "acct-1".into(),
            property_id: "prop-1".into(),
            data_json: r#"{"id":"acct-1","folioId":"old-1","kind":"folio","state":"open"}"#.into(),
            name: "Riverside".into(),
            business_date: "2026-11-25".into(),
            currency: "THB".into(),
        };
        let entries = vec![EntryRow {
            id: "entry-1".into(),
            data_json: r#"{"id":"entry-1","kind":"charge","total":117,"toFolioId":"old-2"}"#.into(),
            journal_id: Some("journal-1".into()),
        }];
        let evidence = project(account, entries).expect("projection");
        let rendered = serde_json::to_string(&evidence).expect("json");
        assert_eq!(
            rendered,
            r#"{"id":"acct-1","propertyId":"prop-1","hotel":"Riverside","businessDate":"2026-11-25","currency":"THB","account":{"guestAccountId":"old-1","id":"acct-1","kind":"guest_account","state":"open"},"entries":[{"id":"entry-1","journalId":"journal-1","data":{"id":"entry-1","kind":"charge","toGuestAccountId":"old-2","total":117}}]}"#
        );
        assert_eq!(
            sha256(&rendered),
            "9f5e00d2e082305bbc3c8ec14a5ddee3affcdc7078a06ab5cdfe5910b4690fec"
        );
    }
}
