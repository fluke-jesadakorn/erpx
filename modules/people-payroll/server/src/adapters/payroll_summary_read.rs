//! PostgreSQL read adapter for native Payroll summaries and run lists.
//!
//! The caller owns the retained serializable Holding transaction. Only
//! People-owned payroll tables are read; the payment-batch join is a Payroll
//! source fact owned by this owner. No command, purchase, IAM or independent
//! transaction is created here.
use crate::application::payroll_summary::{
    PayrollReadFuture, PayrollReadRepository, payroll_work_rows,
};
use crate::domain::payroll_summary::{
    PayrollMonth, PayrollReadActor, PayrollRunRow, PayrollSummaryRow, PayrollWorkRow,
};
use base64::{
    Engine, alphabet,
    engine::{
        DecodePaddingMode,
        general_purpose::{GeneralPurpose, GeneralPurposeConfig, URL_SAFE_NO_PAD},
    },
};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Deserialize, Serialize)]
struct HistoryCursor {
    key: String,
    id: String,
    reverse: bool,
    scope: String,
}

fn invalid_history_cursor() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "INVALID_CURSOR",
        "Restart this search to load results.",
    )
}

fn history_scope(company: &CompanyId) -> String {
    // This is JSON.stringify([[companyId, 'payroll'], true, 30]).
    let input = serde_json::to_vec(&serde_json::json!([
        [company.as_str(), "payroll"],
        true,
        30
    ]))
    .expect("JSON scope");
    format!("{:x}", Sha256::digest(input))
}

fn decode_history_cursor(
    raw: Option<&str>,
    scope: &str,
) -> Result<Option<HistoryCursor>, BusinessError> {
    let Some(raw) = raw.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if raw.encode_utf16().count() > 4096 {
        return Err(invalid_history_cursor());
    }
    // Buffer.from(value, 'base64url') also accepts standard alphabet and ignores
    // non-alphabet characters; stop at padding as Node does.
    let mut bytes: Vec<u8> = raw
        .bytes()
        .take_while(|b| *b != b'=')
        .filter_map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => Some(b),
            b'+' => Some(b'-'),
            b'/' => Some(b'_'),
            _ => None,
        })
        .collect();
    if bytes.len() % 4 == 1 {
        bytes.pop();
    }
    let decoder = GeneralPurpose::new(
        &alphabet::URL_SAFE,
        GeneralPurposeConfig::new()
            .with_decode_allow_trailing_bits(true)
            .with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    let decoded = decoder
        .decode(bytes)
        .map_err(|_| invalid_history_cursor())?;
    let value: HistoryCursor = serde_json::from_str(&String::from_utf8_lossy(&decoded))
        .map_err(|_| invalid_history_cursor())?;
    if value.scope != scope {
        return Err(invalid_history_cursor());
    }
    Ok(Some(value))
}

#[cfg(test)]
mod history_cursor_tests {
    use super::*;

    #[test]
    fn node_buffer_base64url_accepts_padding_whitespace_standard_alphabet_and_tail_bits() {
        let scope = history_scope(&CompanyId::new("company").unwrap());
        let (original, encoded) = ["\u{ffff}", "a\u{ffff}", "ab\u{ffff}"]
            .into_iter()
            .map(|key| {
                let cursor = HistoryCursor {
                    key: key.into(),
                    id: "id".into(),
                    reverse: false,
                    scope: scope.clone(),
                };
                let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap());
                (cursor, encoded)
            })
            .find(|(_, value)| value.contains('_') || value.contains('-'))
            .unwrap();
        assert!(encoded.contains('_') || encoded.contains('-'));
        let standard = encoded.replace('-', "+").replace('_', "/");
        for raw in [
            format!("!{encoded}==ignored"),
            format!(" \n{standard}=ignored"),
            encoded.clone(),
        ] {
            let cursor = decode_history_cursor(Some(&raw), &scope).unwrap().unwrap();
            assert_eq!(cursor.key, original.key);
            assert_eq!(cursor.id, original.id);
        }
        let mut json = serde_json::to_vec(&original).unwrap();
        while json.len() % 3 == 0 {
            json.push(b' ');
        }
        let mut noncanonical = URL_SAFE_NO_PAD.encode(json).into_bytes();
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let last = noncanonical.len() - 1;
        let value = alphabet
            .iter()
            .position(|v| *v == noncanonical[last])
            .unwrap();
        noncanonical[last] = alphabet[value | 1];
        let noncanonical = String::from_utf8(noncanonical).unwrap();
        assert_eq!(
            decode_history_cursor(Some(&noncanonical), &scope)
                .unwrap()
                .unwrap()
                .key,
            original.key
        );
        assert!(decode_history_cursor(Some("YQ=="), &scope).is_err());
        assert!(decode_history_cursor(Some(&encoded), "foreign-scope").is_err());
        assert!(decode_history_cursor(Some(""), &scope).unwrap().is_none());
    }
}

/// Full history page. Authority and Company scope are checked before decoding
/// the cursor, as in the TypeScript reader. The caller owns the transaction.
pub async fn payroll_history(
    c: &mut PgConnection,
    actor: &crate::domain::payroll_summary::PayrollReadActor,
    cursor: Option<&str>,
    now: TimestampMillis,
) -> Result<crate::domain::payroll_summary::PayrollHistoryPage, Error> {
    actor.require_summary(now)?;
    serializable(c).await?;
    let scope = history_scope(&actor.company_id);
    let cursor = decode_history_cursor(cursor, &scope)?;
    let reverse = cursor.as_ref().is_some_and(|value| value.reverse);
    let query = if reverse {
        "WITH list AS (SELECT r.*,r.pay_date||':'||r.created_at::text AS page_key FROM hr_payroll_runs r WHERE r.company_id=$1) SELECT id,page_key,to_jsonb(list) AS item FROM list WHERE ($2::text IS NULL OR (page_key,id) > ($2,$3)) ORDER BY page_key ASC,id ASC LIMIT 31"
    } else {
        "WITH list AS (SELECT r.*,r.pay_date||':'||r.created_at::text AS page_key FROM hr_payroll_runs r WHERE r.company_id=$1) SELECT id,page_key,to_jsonb(list) AS item FROM list WHERE ($2::text IS NULL OR (page_key,id) < ($2,$3)) ORDER BY page_key DESC,id DESC LIMIT 31"
    };
    let rows = sqlx::query(query)
        .bind(actor.company_id.as_str())
        .bind(cursor.as_ref().map(|value| value.key.as_str()))
        .bind(cursor.as_ref().map(|value| value.id.as_str()))
        .fetch_all(&mut *c)
        .await?;
    let overflow = rows.len() > 30;
    let mut items = Vec::with_capacity(rows.len().min(30));
    for row in rows.into_iter().take(30) {
        let item: serde_json::Value = row.try_get("item")?;
        // The existing PostgreSQL adapter rejects bigint values outside JS's
        // exact integer range before handing rows to the page.
        if item.as_object().is_some_and(|object| {
            object.values().any(|value| {
                value
                    .as_i64()
                    .is_some_and(|n| n.unsigned_abs() > 9_007_199_254_740_991)
            })
        }) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "DATABASE_NUMBER_OUT_OF_RANGE",
                "Payroll data is unavailable.",
            )
            .into());
        }
        items.push(item);
    }
    if reverse {
        items.reverse();
    }
    let encode = |item: &serde_json::Value, reverse| -> Result<String, BusinessError> {
        let cursor = HistoryCursor {
            key: item["page_key"]
                .as_str()
                .ok_or_else(invalid_history_cursor)?
                .to_owned(),
            id: item["id"]
                .as_str()
                .ok_or_else(invalid_history_cursor)?
                .to_owned(),
            reverse,
            scope: scope.clone(),
        };
        Ok(URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&cursor).map_err(|_| invalid_history_cursor())?))
    };
    let previous_cursor = if !items.is_empty() && if reverse { overflow } else { cursor.is_some() }
    {
        Some(encode(&items[0], true)?)
    } else {
        None
    };
    let next_cursor = if !items.is_empty() && if reverse { cursor.is_some() } else { overflow } {
        Some(encode(items.last().expect("nonempty"), false)?)
    } else {
        None
    };
    Ok(crate::domain::payroll_summary::PayrollHistoryPage {
        has_more: next_cursor.is_some(),
        items,
        next_cursor,
        previous_cursor,
    })
}

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PAYROLL_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PAYROLL_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgPayrollRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPayrollRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn summary(
        &mut self,
        company: &CompanyId,
        month: Option<&PayrollMonth>,
    ) -> Result<Vec<PayrollSummaryRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT r.company_id,substring(r.pay_date FROM 1 FOR 7) AS pay_month,r.currency,\
             COUNT(DISTINCT r.id)::bigint AS run_count,\
             COUNT(DISTINCT i.engagement_id)::bigint AS headcount,\
             COALESCE(SUM(i.gross_minor),0)::bigint AS gross_minor,\
             COALESCE(SUM(i.deductions_minor),0)::bigint AS deductions_minor,\
             COALESCE(SUM(i.employer_cost_minor),0)::bigint AS employer_cost_minor,\
             COALESCE(SUM(i.net_minor),0)::bigint AS net_minor \
             FROM hr_payroll_runs r JOIN hr_payroll_items i ON i.run_id=r.id \
             WHERE r.company_id=$1 AND r.status='posted' \
             AND ($2::text IS NULL OR substring(r.pay_date FROM 1 FOR 7)=$2) \
             GROUP BY r.company_id,substring(r.pay_date FROM 1 FOR 7),r.currency \
             ORDER BY pay_month DESC,r.currency ASC LIMIT 121",
        )
        .bind(company.as_str())
        .bind(month.map(PayrollMonth::as_str))
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(PayrollSummaryRow {
                    company_id: row.try_get("company_id")?,
                    pay_month: row.try_get("pay_month")?,
                    currency: row.try_get("currency")?,
                    run_count: row.try_get("run_count")?,
                    headcount: row.try_get("headcount")?,
                    gross_minor: row.try_get("gross_minor")?,
                    deductions_minor: row.try_get("deductions_minor")?,
                    employer_cost_minor: row.try_get("employer_cost_minor")?,
                    net_minor: row.try_get("net_minor")?,
                })
            })
            .collect()
    }

    pub async fn runs(
        &mut self,
        company: &CompanyId,
        full: bool,
    ) -> Result<Vec<PayrollRunRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = if full {
            sqlx::query(
                "SELECT company_id,id,pay_group_id,period_start,period_end,pay_date,currency,\
                 status,mode,revision,version,totals_json,NULL::text AS payment_batch_id,\
                 NULL::text AS payment_status,NULL::bigint AS net_payable_minor \
                 FROM hr_payroll_runs WHERE company_id=$1 \
                 ORDER BY pay_date DESC,created_at DESC,id ASC LIMIT 2001",
            )
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?
        } else {
            sqlx::query(
                "SELECT r.company_id,r.id,NULL::text AS pay_group_id,r.period_start,\
                 r.period_end,r.pay_date,r.currency,r.status,r.mode,r.revision,r.version,\
                 NULL::text AS totals_json,b.id AS payment_batch_id,\
                 b.status AS payment_status,b.total_minor AS net_payable_minor \
                 FROM hr_payroll_runs r LEFT JOIN LATERAL (\
                   SELECT b.id,b.status,b.total_minor FROM hr_payment_batches b \
                   WHERE b.source_type='payroll' AND b.source_id=r.id \
                   AND b.company_id=r.company_id AND b.status<>'cancelled' \
                   ORDER BY b.created_at DESC,b.id ASC LIMIT 1\
                 ) b ON true \
                 WHERE r.company_id=$1 AND r.status='posted' \
                 ORDER BY r.pay_date DESC,r.created_at DESC,r.id ASC LIMIT 2001",
            )
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?
        };
        rows.iter()
            .map(|row| {
                Ok(PayrollRunRow {
                    company_id: row.try_get("company_id")?,
                    id: row.try_get("id")?,
                    pay_group_id: row.try_get("pay_group_id")?,
                    period_start: row.try_get("period_start")?,
                    period_end: row.try_get("period_end")?,
                    pay_date: row.try_get("pay_date")?,
                    currency: row.try_get("currency")?,
                    status: row.try_get("status")?,
                    mode: row.try_get("mode")?,
                    revision: row.try_get("revision")?,
                    version: row.try_get("version")?,
                    totals_json: row.try_get("totals_json")?,
                    payment_batch_id: row.try_get("payment_batch_id")?,
                    payment_status: row.try_get("payment_status")?,
                    net_payable_minor: row.try_get("net_payable_minor")?,
                })
            })
            .collect()
    }

    /// Work rows stay ordered by owner `updated_at DESC` so Workflow can keep
    /// recency and own-item ordering without reading payroll SQL itself.
    pub async fn work_rows(&mut self, company: &CompanyId) -> Result<Vec<PayrollWorkRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT r.company_id,r.id,r.pay_group_id,r.pay_date,r.status,r.mode,\
             r.created_by,r.updated_at \
             FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id \
             WHERE r.company_id=$1 \
             ORDER BY r.updated_at DESC,r.id ASC LIMIT 2001",
        )
        .bind(company.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(PayrollWorkRow {
                    company_id: row.try_get("company_id")?,
                    id: row.try_get("id")?,
                    pay_group_id: row.try_get("pay_group_id")?,
                    pay_date: row.try_get("pay_date")?,
                    status: row.try_get("status")?,
                    mode: row.try_get("mode")?,
                    created_by: row.try_get("created_by")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => BusinessError::new(
            erpx_platform_shared_server::public::FailureKind::Unavailable,
            error.code,
            "The current payroll work rows are unavailable.",
        ),
    }
}

/// In-process payroll work rows on a caller-owned serializable transaction.
///
/// No HTTP target, registered Host, cookie or Account session is required: the
/// composition supplies the current Company payroll actor and owns the
/// transaction. The owner read still enforces Company scope, selected-context
/// expiry and the payroll read permission before one bounded query.
pub async fn payroll_work_rows_on(
    connection: &mut PgConnection,
    actor: &PayrollReadActor,
    now: TimestampMillis,
) -> Result<Vec<PayrollWorkRow>, BusinessError> {
    let mut repository = PgPayrollRead::new(connection);
    payroll_work_rows(actor, &mut repository, now)
        .await
        .map_err(business)
}

impl PayrollReadRepository for PgPayrollRead<'_> {
    type Error = Error;

    fn summary<'a>(
        &'a mut self,
        company: &'a CompanyId,
        month: Option<&'a PayrollMonth>,
    ) -> PayrollReadFuture<'a, Vec<PayrollSummaryRow>, Self::Error> {
        Box::pin(async move { PgPayrollRead::summary(self, company, month).await })
    }

    fn runs<'a>(
        &'a mut self,
        company: &'a CompanyId,
        full: bool,
    ) -> PayrollReadFuture<'a, Vec<PayrollRunRow>, Self::Error> {
        Box::pin(async move { PgPayrollRead::runs(self, company, full).await })
    }

    fn work_rows<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> PayrollReadFuture<'a, Vec<PayrollWorkRow>, Self::Error> {
        Box::pin(async move { PgPayrollRead::work_rows(self, company).await })
    }
}
