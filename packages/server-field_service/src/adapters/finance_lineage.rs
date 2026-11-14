//! Minimal original parts lineage for an independent Finance reviewer. This
//! intentionally does not borrow operational Person, Agreement, Support, Asset,
//! technician, status, records.write or current stock eligibility.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Eq, PartialEq)]
pub struct FieldServicePartsFinanceRequest {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub order_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub minimum_order_version: i64,
    pub owner_key: String,
    pub consumption_id: String,
    pub journal_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub creator_user_id: String,
}
pub struct CurrentFieldServicePartsFinanceLineage {
    request: FieldServicePartsFinanceRequest,
    unit: Unit,
}
impl CurrentFieldServicePartsFinanceLineage {
    pub fn request(&self) -> &FieldServicePartsFinanceRequest {
        &self.request
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || !exists(c, &self.request).await? {
            return Err(changed());
        }
        Ok(())
    }
}
/// Compose these owner-only utility fences before the first Holding snapshot.
pub async fn lock_field_service_parts_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE field_service_orders,field_service_order_events,field_service_part_reservations IN SHARE MODE").execute(c).await?;
    Ok(())
}
pub async fn read_field_service_parts_finance_lineage(
    c: &mut PgConnection,
    request: FieldServicePartsFinanceRequest,
) -> Result<Option<CurrentFieldServicePartsFinanceLineage>, Error> {
    let unit = unit(c).await?;
    if [
        request.minimum_order_version,
        request.quantity_milli,
        request.value_minor,
    ]
    .iter()
    .any(|n| !(1..=9_007_199_254_740_991).contains(n))
        || request.consumption_id.as_str()
            != format!("operational-part-{}", request.owner_key.as_str())
    {
        return Err(invalid());
    }
    Ok(exists(c, &request)
        .await?
        .then_some(CurrentFieldServicePartsFinanceLineage { request, unit }))
}
async fn exists(c: &mut PgConnection, r: &FieldServicePartsFinanceRequest) -> Result<bool, Error> {
    // Only an existence predicate leaves the owner. Unused reason/details/body
    // size cannot hide valid original provenance or leak personal data.
    Ok(sqlx::query_scalar::<_,bool>("SELECT true FROM field_service_orders o JOIN field_service_order_events e ON e.order_id=o.id AND e.version=$5 AND e.action='issue-part' AND e.id=$6 AND e.actor_id=$7 AND e.details_json->>'consumptionId'=$8 AND e.details_json->>'journalId'=$9 AND e.details_json->>'quantityMilli'=$10::bigint::text AND e.details_json->>'costMinor'=$11::bigint::text JOIN field_service_part_reservations p ON p.id=e.details_json->>'reservationId' AND p.order_id=o.id AND p.warehouse_id=$12 AND p.product_id=$13 WHERE o.id=$1 AND o.company_id=$2 AND o.branch_id=$3 AND o.department_id IS NOT DISTINCT FROM $4 AND o.version>=$5 FOR SHARE OF o,e,p")
        .bind(r.order_id.as_str()).bind(r.company_id.as_str()).bind(r.branch_id.as_str()).bind(r.department_id.as_deref()).bind(r.minimum_order_version).bind(format!("service-order-event-{}",r.owner_key.as_str())).bind(r.creator_user_id.as_str()).bind(r.consumption_id.as_str()).bind(r.journal_id.as_str()).bind(r.quantity_milli).bind(r.value_minor).bind(r.warehouse_id.as_str()).bind(r.product_id.as_str()).fetch_optional(c).await?.unwrap_or(false))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("FIELD_SERVICE_FINANCE_LINEAGE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_FINANCE_LINEAGE_CHANGED",
        "The original immutable parts event/reservation and current Job scope changed.",
    )
    .into()
}

/// Field-owned original billing facts for a Finance reader; no Service role is implied.
#[derive(Clone, Eq, PartialEq)]
pub struct FieldServiceInvoiceFinanceRequest {
    pub company_id: CompanyId,
    pub order_id: String,
    pub release_id: String,
    pub key: String,
    pub actor_id: String,
    pub currency: String,
    pub amount_units: String,
    pub customer_id: String,
    pub pricing_date: String,
}
pub struct CurrentFieldServiceInvoiceFinanceLineage {
    request: FieldServiceInvoiceFinanceRequest,
    unit: Unit,
    digest: String,
    origin: erpx_server_workflow::public::record_origin::PgRawRecordOrigin,
}
pub async fn lock_field_service_invoice_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE field_service_agreements,field_service_orders,field_service_order_events,field_service_billing_releases IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn invoice_image(
    c: &mut PgConnection,
    r: &FieldServiceInvoiceFinanceRequest,
) -> Result<Option<(String, i64)>, Error> {
    Ok(sqlx::query_as("SELECT encode(sha256(convert_to(jsonb_build_array(to_jsonb(b),to_jsonb(o),to_jsonb(g),to_jsonb(e))::text,'UTF8')),'hex'),b.source_order_version::bigint FROM field_service_billing_releases b JOIN field_service_orders o ON o.id=b.order_id AND o.agreement_id=b.agreement_id AND o.version>=b.source_order_version JOIN field_service_agreements g ON g.id=o.agreement_id AND g.company_id=o.company_id AND g.branch_id=o.branch_id AND g.department_id IS NOT DISTINCT FROM o.department_id AND g.customer_id=b.customer_id JOIN field_service_order_events e ON e.order_id=o.id AND e.id=$3 AND e.version=b.source_order_version AND e.action='release-billing' AND e.status='billed' AND e.actor_id=b.released_by AND e.reason=b.reason WHERE b.id=$1 AND o.company_id=$2 AND o.id=$4 AND b.released_by=$5 AND b.currency=$6 AND b.amount_minor::numeric=$7::numeric AND b.customer_id=$8 AND b.pricing_date=$9 AND b.status='released' FOR SHARE OF b,o,g,e").bind(&r.release_id).bind(r.company_id.as_str()).bind(format!("service-order-event-{}",r.key)).bind(&r.order_id).bind(&r.actor_id).bind(&r.currency).bind(&r.amount_units).bind(&r.customer_id).bind(&r.pricing_date).fetch_optional(c).await?)
}
pub async fn read_field_service_invoice_finance_lineage(
    c: &mut PgConnection,
    r: FieldServiceInvoiceFinanceRequest,
) -> Result<Option<CurrentFieldServiceInvoiceFinanceLineage>, Error> {
    use erpx_server_workflow::public::record_origin::{
        RawRecordOriginRequest, read_raw_record_origin_result_with_limit,
    };
    let unit = unit(c).await?;
    if r.release_id != format!("field-billing-{}", r.key) {
        return Ok(None);
    }
    let Some((digest, version)) = invoice_image(c, &r).await? else {
        return Ok(None);
    };
    let Some((origin, result)) = read_raw_record_origin_result_with_limit(
        c,
        RawRecordOriginRequest {
            command_key: r.key.clone(),
            operation: "field.order.release-billing".into(),
            record_id: r.order_id.clone(),
            creator_user_id: r.actor_id.clone(),
            company_id: r.company_id.clone(),
        },
        2 * 1024 * 1024,
    )
    .await?
    else {
        return Ok(None);
    };
    if result
        .property("id")?
        .and_then(|v| v.string_utf16_units().ok().flatten())
        != Some(r.order_id.encode_utf16().collect())
        || result
            .property("status")?
            .and_then(|v| v.string_utf8_storage().ok().flatten())
            .as_deref()
            != Some("billed")
        || result
            .property("version")?
            .and_then(|v| v.as_str().parse::<i64>().ok())
            != Some(version)
    {
        return Ok(None);
    }
    Ok(Some(CurrentFieldServiceInvoiceFinanceLineage {
        request: r,
        unit,
        digest,
        origin,
    }))
}
impl CurrentFieldServiceInvoiceFinanceLineage {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || invoice_image(c, &self.request).await?.map(|v| v.0).as_ref() != Some(&self.digest)
        {
            return Err(changed());
        }
        erpx_server_workflow::public::record_origin::verify_raw_record_origin(c, &self.origin).await
    }
}

/// Candidate facts obtained by a trusted Finance/Workflow coordinator from
/// persisted rows, never directly from an HTTP request. This structure carries
/// no Finance, Workflow, Money, purchase or personal authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldInvoiceHistoryCandidate {
    pub company_id: CompanyId,
    pub order_id: String,
    pub command_id: String,
    pub actor_id: String,
    pub customer_id: String,
    pub currency: String,
    pub amount_minor: i64,
    pub pricing_date: String,
}
type HistoryCandidateUnit = ((String,i32,String,String,String),String,Vec<i64>);

/// Field-only source fences for generic Finance History. Acquire before any
/// snapshot together with the separate Finance, Workflow, Money, Commercial,
/// IAM and other source fences in a reviewed coordinator order. A conflict
/// must retry the whole unit; these locks are not a source or access grant.
pub async fn lock_field_invoice_history_candidate_sources(c:&mut PgConnection)->Result<(),Error> {
    if !c.is_in_transaction(){return Err(PersistenceError::new("FIELD_INVOICE_HISTORY_TRANSACTION_REQUIRED").into());}
    for sql in ["LOCK TABLE public.field_service_agreements IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_orders IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_order_events IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_billing_releases IN SHARE MODE NOWAIT"] {
        sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    }
    Ok(())
}
async fn history_candidate_unit(c:&mut PgConnection)->Result<HistoryCandidateUnit,Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(PersistenceError::new("FIELD_INVOICE_HISTORY_SERIALIZABLE_REQUIRED").into());}
    let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=4 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.field_service_agreements'::regclass,'public.field_service_orders'::regclass,'public.field_service_order_events'::regclass,'public.field_service_billing_releases'::regclass)), NOT (row_security_active('public.field_service_agreements'::regclass) OR row_security_active('public.field_service_orders'::regclass) OR row_security_active('public.field_service_order_events'::regclass) OR row_security_active('public.field_service_billing_releases'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("FIELD_INVOICE_HISTORY_LOCKS_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("FIELD_INVOICE_HISTORY_RLS_UNSUPPORTED").into());}
    let oids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.field_service_agreements'::regclass::oid::bigint,'public.field_service_orders'::regclass::oid::bigint,'public.field_service_order_events'::regclass::oid::bigint,'public.field_service_billing_releases'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
    Ok((transaction,path,oids))
}
async fn field_invoice_history_candidate_exists(c:&mut PgConnection,r:&FieldInvoiceHistoryCandidate)->Result<bool,Error> {
    // Exact Field-owned subpredicate of finance-record-sources.ts invoice.
    // Do not add event.reason, receipt result, module, status or scope rules:
    // those belong to other readers or independent owner proofs.
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.field_service_billing_releases billing JOIN public.field_service_orders service ON service.id=billing.order_id AND service.agreement_id=billing.agreement_id AND service.version>=billing.source_order_version JOIN public.field_service_agreements agreement ON agreement.id=service.agreement_id AND agreement.company_id=service.company_id AND agreement.branch_id=service.branch_id AND agreement.department_id IS NOT DISTINCT FROM service.department_id AND agreement.customer_id=billing.customer_id JOIN public.field_service_order_events event ON event.order_id=service.id AND event.version=billing.source_order_version AND event.action='release-billing' AND event.status='billed' AND event.actor_id=billing.released_by WHERE billing.status='released' AND billing.id='field-billing-'||$1 AND event.id='service-order-event-'||$1 AND service.company_id=$2 AND service.id=$3 AND billing.released_by=$4 AND billing.customer_id=$5 AND billing.currency=$6 AND billing.amount_minor=$7 AND billing.pricing_date=$8)")
        .bind(&r.command_id).bind(r.company_id.as_str()).bind(&r.order_id).bind(&r.actor_id).bind(&r.customer_id).bind(&r.currency).bind(r.amount_minor).bind(&r.pricing_date).fetch_one(c).await?)
}
/// Retains whether one actual cross-owner candidate satisfies the Field part of
/// the generic History predicate. An absent candidate is not a global negative:
/// the coordinator must enumerate every actual Finance/Workflow candidate.
pub struct CurrentFieldInvoiceHistoryCandidate { request:FieldInvoiceHistoryCandidate, unit:HistoryCandidateUnit, exists:bool }
impl CurrentFieldInvoiceHistoryCandidate {
    pub fn matches(&self)->bool {self.exists}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if history_candidate_unit(&mut *c).await?!=self.unit || field_invoice_history_candidate_exists(&mut *c,&self.request).await?!=self.exists || history_candidate_unit(c).await?!=self.unit {return Err(BusinessError::new(FailureKind::Conflict,"FIELD_INVOICE_HISTORY_SOURCE_CHANGED","The Field billing History source changed.").into());}Ok(())
    }
}
pub async fn observe_field_invoice_history_candidate(c:&mut PgConnection,request:FieldInvoiceHistoryCandidate)->Result<CurrentFieldInvoiceHistoryCandidate,Error>{
    let unit=history_candidate_unit(&mut *c).await?;
    let exists=field_invoice_history_candidate_exists(&mut *c,&request).await?;
    if history_candidate_unit(c).await?!=unit{return Err(BusinessError::new(FailureKind::Conflict,"FIELD_INVOICE_HISTORY_SOURCE_CHANGED","The Field billing History source changed.").into());}
    Ok(CurrentFieldInvoiceHistoryCandidate{request,unit,exists})
}

/// Candidate supplied by a trusted coordinator from actual Inventory and
/// Workflow rows. This carries no proof of those rows, Finance, Money, purchase
/// or personal authority, and never accepts a client-asserted source label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldPartsJournalHistoryCandidate {
    pub company_id:CompanyId,
    pub branch_id:String,
    pub department_id:Option<String>,
    pub order_id:String,
    pub source_version:i64,
    pub command_id:String,
    pub actor_id:String,
    pub consumption_id:String,
    pub journal_id:String,
    pub warehouse_id:String,
    pub product_id:String,
    pub quantity_milli:i64,
    pub value_minor:i64,
}
type PartsCandidateUnit=((String,i32,String,String,String),String,Vec<i64>);
/// Field-owned relations only. Caller acquires all remaining owner locks before
/// the first snapshot in a reviewed order and retries the whole unit on 55P03.
pub async fn lock_field_parts_journal_history_candidate_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FIELD_PARTS_HISTORY_TRANSACTION_REQUIRED").into());}
    for sql in ["LOCK TABLE public.field_service_orders IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_order_events IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_part_reservations IN SHARE MODE NOWAIT"] {
        sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    } Ok(())
}
async fn parts_candidate_unit(c:&mut PgConnection)->Result<PartsCandidateUnit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("FIELD_PARTS_HISTORY_SERIALIZABLE_REQUIRED").into());}
    let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=3 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.field_service_orders'::regclass,'public.field_service_order_events'::regclass,'public.field_service_part_reservations'::regclass)),NOT (row_security_active('public.field_service_orders'::regclass) OR row_security_active('public.field_service_order_events'::regclass) OR row_security_active('public.field_service_part_reservations'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("FIELD_PARTS_HISTORY_LOCKS_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("FIELD_PARTS_HISTORY_RLS_UNSUPPORTED").into());}
    let oids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.field_service_orders'::regclass::oid::bigint,'public.field_service_order_events'::regclass::oid::bigint,'public.field_service_part_reservations'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;Ok((transaction,path,oids))
}
async fn field_parts_journal_history_candidate_exists(c:&mut PgConnection,r:&FieldPartsJournalHistoryCandidate)->Result<bool,Error>{
    // Exact Field-owned predicates in finance-record-sources.ts partsJournal.
    // Positivity, receipt consumption ID, balanced lines and Money belong to
    // other owners; no extra event reason/status/current role is introduced.
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.field_service_orders service JOIN public.field_service_order_events event ON event.order_id=service.id AND event.version=$5 AND event.action='issue-part' AND event.details_json->>'consumptionId'=$8 AND event.details_json->>'journalId'=$9 JOIN public.field_service_part_reservations reservation ON reservation.id=event.details_json->>'reservationId' AND reservation.order_id=service.id AND reservation.warehouse_id=$10 AND reservation.product_id=$11 WHERE service.id=$1 AND service.company_id=$2 AND service.branch_id=$3 AND service.department_id IS NOT DISTINCT FROM $4 AND service.version>=$5 AND event.id='service-order-event-'||$6 AND event.actor_id=$7 AND event.details_json->>'quantityMilli'=$12::bigint::text AND event.details_json->>'costMinor'=$13::bigint::text)")
        .bind(&r.order_id).bind(r.company_id.as_str()).bind(&r.branch_id).bind(r.department_id.as_deref()).bind(r.source_version).bind(&r.command_id).bind(&r.actor_id).bind(&r.consumption_id).bind(&r.journal_id).bind(&r.warehouse_id).bind(&r.product_id).bind(r.quantity_milli).bind(r.value_minor).fetch_one(c).await?)
}
/// Retained truth of one concrete cross-owner candidate, including absence. A
/// false result is not a global negative; enumerate actual receipt/Inventory
/// candidates and retain/revalidate every other owner's matching evidence.
pub struct CurrentFieldPartsJournalHistoryCandidate{request:FieldPartsJournalHistoryCandidate,unit:PartsCandidateUnit,matches:bool}
impl CurrentFieldPartsJournalHistoryCandidate{
    pub fn matches(&self)->bool{self.matches}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if parts_candidate_unit(&mut *c).await?!=self.unit || field_parts_journal_history_candidate_exists(&mut *c,&self.request).await?!=self.matches || parts_candidate_unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"FIELD_PARTS_HISTORY_SOURCE_CHANGED","The Field parts History source changed.").into());}Ok(())}
}
pub async fn observe_field_parts_journal_history_candidate(c:&mut PgConnection,request:FieldPartsJournalHistoryCandidate)->Result<CurrentFieldPartsJournalHistoryCandidate,Error>{let unit=parts_candidate_unit(&mut *c).await?;let matches=field_parts_journal_history_candidate_exists(&mut *c,&request).await?;if parts_candidate_unit(c).await?!=unit{return Err(BusinessError::new(FailureKind::Conflict,"FIELD_PARTS_HISTORY_SOURCE_CHANGED","The Field parts History source changed.").into());}Ok(CurrentFieldPartsJournalHistoryCandidate{request,unit,matches})}

/// Field-owned inputs for generic Finance History SQL. Each simple projection
/// preserves PostgreSQL column types and collations; it is not a grant or a
/// retained classification and does not transfer source rows into Rust.
const FIELD_HISTORY_OWNER_CTES:&str=r#"field_history_orders AS (SELECT id,company_id,branch_id,department_id,agreement_id,version FROM public.field_service_orders),
field_history_order_events AS (SELECT id,order_id,version,action,actor_id,status,details_json FROM public.field_service_order_events),
field_history_agreements AS (SELECT id,company_id,branch_id,department_id,customer_id FROM public.field_service_agreements),
field_history_billing_releases AS (SELECT id,order_id,agreement_id,customer_id,currency,amount_minor,source_order_version,status,released_by,pricing_date FROM public.field_service_billing_releases),
field_history_part_reservations AS (SELECT id,order_id,warehouse_id,product_id FROM public.field_service_part_reservations)"#;
type FieldHistoryOwnerUnit=((String,i32,String,String,String),String,Vec<i64>);

/// Field utility fences only; the coordinator acquires Support, Asset,
/// Inventory, Finance, Workflow, Money, IAM and Commercial sources separately,
/// before the first snapshot in one reviewed order, retrying the whole unit.
pub async fn lock_field_history_join_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FIELD_HISTORY_JOIN_TRANSACTION_REQUIRED").into());}
    for sql in ["LOCK TABLE public.field_service_agreements IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_orders IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_order_events IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_part_reservations IN SHARE MODE NOWAIT","LOCK TABLE public.field_service_billing_releases IN SHARE MODE NOWAIT"] {
        sqlx::query(sql).execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    } Ok(())
}
async fn field_history_join_unit(c:&mut PgConnection)->Result<FieldHistoryOwnerUnit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("FIELD_HISTORY_JOIN_SERIALIZABLE_REQUIRED").into());}
    let (held,visible,path):(bool,bool,String)=sqlx::query_as("SELECT (SELECT count(*)=5 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.field_service_agreements'::regclass,'public.field_service_orders'::regclass,'public.field_service_order_events'::regclass,'public.field_service_part_reservations'::regclass,'public.field_service_billing_releases'::regclass)),NOT (row_security_active('public.field_service_agreements'::regclass) OR row_security_active('public.field_service_orders'::regclass) OR row_security_active('public.field_service_order_events'::regclass) OR row_security_active('public.field_service_part_reservations'::regclass) OR row_security_active('public.field_service_billing_releases'::regclass)),current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("FIELD_HISTORY_JOIN_LOCKS_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("FIELD_HISTORY_JOIN_RLS_UNSUPPORTED").into());}
    let oids:Vec<i64>=sqlx::query_scalar("SELECT unnest(ARRAY['public.field_service_agreements'::regclass::oid::bigint,'public.field_service_orders'::regclass::oid::bigint,'public.field_service_order_events'::regclass::oid::bigint,'public.field_service_part_reservations'::regclass::oid::bigint,'public.field_service_billing_releases'::regclass::oid::bigint])").fetch_all(&mut *c).await?;
    let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;Ok((transaction,path,oids))
}
/// A canonical Field SQL source bound to one unit. This witness checks source
/// identity/fences only, not content or classification. Consumer must retain
/// its joined result and recompute it after all late effects.
pub struct CurrentFieldHistoryJoin{unit:FieldHistoryOwnerUnit}
impl CurrentFieldHistoryJoin{
    /// Fixed fragments only; never interpolate any user-controlled SQL.
    pub fn cte_sql(&self)->&'static str{FIELD_HISTORY_OWNER_CTES}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if field_history_join_unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"FIELD_HISTORY_JOIN_UNIT_CHANGED","The Field History join source changed.").into());}Ok(())}
}
pub async fn inspect_field_history_join(c:&mut PgConnection)->Result<CurrentFieldHistoryJoin,Error>{Ok(CurrentFieldHistoryJoin{unit:field_history_join_unit(c).await?})}
