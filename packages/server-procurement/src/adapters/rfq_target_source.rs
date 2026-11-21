//! Source-bound complete RFQ image used by the retained purchase wrapper.
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, Vec<i64>);
fn invalid() -> Error {
    PersistenceError::new("RFQ_TARGET_UNIT_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "PROCUREMENT_SOURCE_CHANGED",
        "The purchasing source changed before commit. Refresh its workflow and retry.",
    )
    .into()
}
/// Global owner lock order must be established before the first snapshot.
pub async fn lock_rfq_target_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE public.procurement_rfqs,public.procurement_rfq_lines,public.procurement_rfq_offers,public.procurement_rfq_offer_lines,public.procurement_rfq_events,public.procurement_request_orders IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
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
    let valid:bool=sqlx::query_scalar("SELECT count(DISTINCT c.oid)=6 FROM pg_class c JOIN pg_locks l ON l.relation=c.oid WHERE c.oid IN (to_regclass('public.procurement_rfqs'),to_regclass('public.procurement_rfq_lines'),to_regclass('public.procurement_rfq_offers'),to_regclass('public.procurement_rfq_offer_lines'),to_regclass('public.procurement_rfq_events'),to_regclass('public.procurement_request_orders')) AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND l.pid=pg_backend_pid() AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(&mut *c).await?;
    if !valid {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,ARRAY['public.procurement_rfqs'::regclass::oid::bigint,'public.procurement_rfq_lines'::regclass::oid::bigint,'public.procurement_rfq_offers'::regclass::oid::bigint,'public.procurement_rfq_offer_lines'::regclass::oid::bigint,'public.procurement_rfq_events'::regclass::oid::bigint,'public.procurement_request_orders'::regclass::oid::bigint]").fetch_one(c).await?)
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqTargetFacts {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub image_json: String,
}
impl RfqTargetFacts {
    pub fn within_selected_scope(&self, current: &AuthorizedWorkContext) -> bool {
        let w = current.selection().workspace();
        self.company_id == w.company_id.as_str()
            && !w
                .branch_id
                .as_ref()
                .is_some_and(|b| self.branch_id != b.as_str())
            && !w
                .department_id
                .as_ref()
                .is_some_and(|d| self.department_id.as_deref() != Some(d.as_str()))
    }
}
async fn read(c: &mut PgConnection, id: &str) -> Result<Option<RfqTargetFacts>, Error> {
    // The enabled purchase wrapper locks by ID before checking selected scope.
    let row:Option<(String,String,String,Option<String>,String)>=sqlx::query_as("SELECT r.id,r.company_id,r.branch_id,r.department_id,jsonb_build_object('record',to_jsonb(r),'lines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM public.procurement_rfq_lines line WHERE line.rfq_id=r.id),'offers',(SELECT jsonb_agg(to_jsonb(offer) ORDER BY offer.id) FROM public.procurement_rfq_offers offer WHERE offer.rfq_id=r.id),'offerLines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM public.procurement_rfq_offer_lines line JOIN public.procurement_rfq_offers offer ON offer.id=line.offer_id WHERE offer.rfq_id=r.id),'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM public.procurement_rfq_events event WHERE event.rfq_id=r.id),'handoffs',(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.request_id,binding.order_id) FROM public.procurement_request_orders binding WHERE binding.rfq_id=r.id))::text FROM public.procurement_rfqs r WHERE r.id=$1 FOR UPDATE OF r").bind(id).fetch_optional(c).await?;
    Ok(row.map(
        |(id, company_id, branch_id, department_id, image_json)| RfqTargetFacts {
            id,
            company_id,
            branch_id,
            department_id,
            image_json,
        },
    ))
}
pub struct CurrentRfqTarget {
    unit: Unit,
    facts: RfqTargetFacts,
}
impl CurrentRfqTarget {
    pub fn facts(&self) -> &RfqTargetFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || read(&mut *c, &self.facts.id).await?.as_ref() != Some(&self.facts)
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// This matches the retained Company and selected branch/department filter.
/// The caller must still check current assignment permission at the target.
pub async fn observe_rfq_target(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<Option<CurrentRfqTarget>, Error> {
    let initial = unit(&mut *c).await?;
    let row = read(&mut *c, id).await?;
    if unit(c).await? != initial {
        return Err(changed());
    }
    let Some(facts) = row else { return Ok(None) };
    if !facts.within_selected_scope(current) {
        return Ok(None);
    }
    Ok(Some(CurrentRfqTarget {
        unit: initial,
        facts,
    }))
}
