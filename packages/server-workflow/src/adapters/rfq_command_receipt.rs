//! Unmounted receipt inspection for normalized RFQ transitions. Caller owns
//! real current Identity, purchase checks and historical result conversion.
use crate::domain::source_ordered_json::{
    SourceOrderedFingerprintActor, SourceOrderedJson, source_ordered_fingerprint_metadata,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, i64);
/// Wrap an owner-provided, static SQL predicate in the legacy Workflow
/// assertion protocol. The predicate placeholders start at $3. Callers must
/// bind the command key and step first and retain the original SQL error until
/// command error translation has run. This does not authorize a command.
pub fn rfq_assertion_statement(predicate: &'static str) -> String {
    format!(
        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,$2,CASE WHEN ({predicate}) THEN 1 ELSE 0 END)"
    )
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Row {
    request_hash: String,
    result_json: Option<String>,
    operation: String,
    actor_id: String,
    authority_json: Option<String>,
}
fn invalid() -> Error {
    PersistenceError::new("RFQ_RECEIPT_UNIT_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "RFQ_RECEIPT_CHANGED",
        "The quotation receipt changed.",
    )
    .into()
}
/// Acquire before the first snapshot at the Workflow position in global lock
/// order. This conservative table fence is unmounted pending parity review.
pub async fn lock_rfq_receipts(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE public.command_receipts IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid=to_regclass('public.command_receipts') AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND l.pid=pg_backend_pid() AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !valid {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,'public.command_receipts'::regclass::oid::bigint").fetch_one(c).await?)
}
async fn read(c: &mut PgConnection, key: &str) -> Result<Option<Row>, Error> {
    let row:Option<(String,Option<String>,String,String,Option<String>)>=sqlx::query_as("SELECT request_hash,result_json,operation,actor_id,authority_json FROM public.command_receipts WHERE command_id=$1 FOR SHARE").bind(key).fetch_optional(c).await?;
    Ok(row.map(
        |(request_hash, result_json, operation, actor_id, authority_json)| Row {
            request_hash,
            result_json,
            operation,
            actor_id,
            authority_json,
        },
    ))
}
pub enum RfqReceiptMatch<'a> {
    Missing,
    Match(Option<&'a str>),
    Conflict,
}
pub struct CurrentRfqReceipt {
    unit: Unit,
    key: String,
    row: Option<Row>,
}
impl CurrentRfqReceipt {
    pub fn matches_key(&self, key: &str) -> bool {
        self.key == key
    }
    /// Exact purchase-wrapper classifier, before fingerprint replay. A true
    /// result is not a hash match or an authorization decision.
    pub async fn selects_purchase_history(
        &self,
        c: &mut PgConnection,
        operation: &str,
        actor: &str,
        company: &str,
    ) -> Result<bool, Error> {
        self.require_current(&mut *c).await?;
        let found:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.command_receipts WHERE command_id=$1 AND operation=$2 AND actor_id=$3 AND authority_json::jsonb->>'companyId'=$4)")
            .bind(&self.key).bind(operation).bind(actor).bind(company).fetch_one(&mut *c).await?;
        self.require_current(c).await?;
        Ok(found)
    }
    /// The input must be the actual Zod-normalized transition shape and actor
    /// metadata must come from current Identity. This only compares hashes;
    /// matching is not a replay grant and result JSON remains unprocessed.
    pub fn compare<'a>(
        &'a self,
        actor: &SourceOrderedFingerprintActor<'_>,
        operation: &str,
        record_id: &str,
        input: &SourceOrderedJson,
    ) -> Result<RfqReceiptMatch<'a>, BusinessError> {
        if !matches!(
            operation,
            "procurement.rfq.submit"
                | "procurement.rfq.approve"
                | "procurement.rfq.return"
                | "procurement.rfq.cancel"
        ) {
            return Err(BusinessError::invalid(
                "RFQ_RECEIPT_OPERATION_INVALID",
                "Use an RFQ transition operation.",
            ));
        }
        let Some(row) = &self.row else {
            return Ok(RfqReceiptMatch::Missing);
        };
        let digest = source_ordered_fingerprint_metadata(actor, operation, record_id, input);
        Ok(if row.request_hash == digest {
            RfqReceiptMatch::Match(row.result_json.as_deref())
        } else {
            RfqReceiptMatch::Conflict
        })
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || read(&mut *c, &self.key).await? != self.row
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn inspect_rfq_receipt(
    c: &mut PgConnection,
    key: &str,
) -> Result<CurrentRfqReceipt, Error> {
    let initial = unit(&mut *c).await?;
    let row = read(&mut *c, key).await?;
    if unit(c).await? != initial {
        return Err(changed());
    }
    Ok(CurrentRfqReceipt {
        unit: initial,
        key: key.into(),
        row,
    })
}
