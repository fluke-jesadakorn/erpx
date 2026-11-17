//! Quality-owned effects of an independently reviewed Inventory Count.
//! The caller retains the actual Inventory Count, IAM, purchase, Finance,
//! Workflow and notification witnesses in this same serializable transaction.
//! This port does not authorize an actor or create a command receipt.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualityCountDispositionAction {
    Post,
    Reject,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityCountDispositionFacts {
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_id: Option<String>,
    pub hold_id: String,
    pub count_id: String,
    pub command_key: String,
    pub actor_id: String,
    pub reason: String,
    pub action: QualityCountDispositionAction,
    pub now: i64,
}
/// Inventory supplies an opaque actual Count witness, not request JSON.
/// `require_before` checks the original Count and selected action; after the
/// Inventory transition `require_count_after` proves that same Count's exact
/// expected after-image. Neither method is an IAM or purchase grant.
pub trait QualityCountDispositionSource: Sync {
    fn facts(&self) -> &QualityCountDispositionFacts;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_count_before_decision(
        &self,
        c: &mut PgConnection,
        remaining_hold_inserted: bool,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_count_after(
        &self,
        c: &mut PgConnection,
        remaining_hold_inserted: bool,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_DISPOSITION_SOURCE_CHANGED",
        "The original Quality disposition changed.",
    )
    .into()
}
fn invalid() -> Error {
    PersistenceError::new("QUALITY_DISPOSITION_TRANSACTION_REQUIRED").into()
}

/// Acquire before any participating owner takes its transaction snapshot.
pub async fn lock_quality_count_disposition_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE inventory_quality_holds,inventory_quality_disposition_events IN SHARE ROW EXCLUSIVE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
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
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation IN ('inventory_quality_holds'::regclass,'inventory_quality_disposition_events'::regclass) AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')").fetch_one(&mut *c).await?;
    if !held {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Hold {
    image: String,
    id: String,
    company_id: String,
    branch_id: String,
    warehouse_id: String,
    product_id: String,
    reason: String,
    due_date: String,
    created_by: String,
    lot_id: Option<String>,
    source_type: Option<String>,
    source_id: Option<String>,
    spec_id: Option<String>,
    spec_version: Option<i64>,
    held: Option<i64>,
    disposition: Option<i64>,
    count_id: Option<String>,
    requester: Option<String>,
    status: String,
    result: Option<String>,
    version: i64,
}
async fn read_hold(
    c: &mut PgConnection,
    facts: &QualityCountDispositionFacts,
) -> Result<Option<Hold>, Error> {
    let row = sqlx::query("SELECT to_jsonb(h)::text image,id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,lot_id,source_type,source_id,spec_id,spec_version::bigint,held_quantity_milli::bigint held,disposition_quantity_milli::bigint disposition,disposition_count_id,disposition_requested_by,status,last_result,version::bigint FROM inventory_quality_holds h WHERE id=$1 AND company_id=$2 AND branch_id=$3 FOR UPDATE")
        .bind(&facts.hold_id).bind(&facts.company_id).bind(&facts.branch_id).fetch_optional(c).await?;
    row.map(|r| {
        Ok(Hold {
            image: r.try_get("image")?,
            id: r.try_get("id")?,
            company_id: r.try_get("company_id")?,
            branch_id: r.try_get("branch_id")?,
            warehouse_id: r.try_get("warehouse_id")?,
            product_id: r.try_get("product_id")?,
            reason: r.try_get("reason")?,
            due_date: r.try_get("due_date")?,
            created_by: r.try_get("created_by")?,
            lot_id: r.try_get("lot_id")?,
            source_type: r.try_get("source_type")?,
            source_id: r.try_get("source_id")?,
            spec_id: r.try_get("spec_id")?,
            spec_version: r.try_get("spec_version")?,
            held: r.try_get("held")?,
            disposition: r.try_get("disposition")?,
            count_id: r.try_get("disposition_count_id")?,
            requester: r.try_get("disposition_requested_by")?,
            status: r.try_get("status")?,
            result: r.try_get("last_result")?,
            version: r.try_get("version")?,
        })
    })
    .transpose()
}

pub struct PreparedQualityCountDisposition<S: QualityCountDispositionSource> {
    source: std::sync::Arc<S>,
    facts: QualityCountDispositionFacts,
    hold: Hold,
    unit: Unit,
}
/// Prepare before the Inventory Count is transitioned. The caller must acquire
/// all other owner and authority fences before this function.
pub async fn prepare_quality_count_disposition<S: QualityCountDispositionSource>(
    c: &mut PgConnection,
    source: std::sync::Arc<S>,
) -> Result<PreparedQualityCountDisposition<S>, Error> {
    let unit = unit(c).await?;
    let facts = source.facts().clone();
    if facts.company_id.is_empty()
        || facts.branch_id.is_empty()
        || facts.warehouse_id.is_empty()
        || facts.product_id.is_empty()
        || facts.hold_id.is_empty()
        || facts.count_id.is_empty()
        || facts.command_key.is_empty()
        || facts.actor_id.is_empty()
        || facts.reason.is_empty()
    {
        return Err(invalid());
    }
    source.require_before(c).await?;
    if *source.facts() != facts {
        return Err(changed());
    }
    let hold = read_hold(c, &facts).await?.ok_or_else(changed)?;
    if hold.warehouse_id != facts.warehouse_id
        || hold.product_id != facts.product_id
        || hold.lot_id != facts.lot_id
    {
        return Err(changed());
    }
    if facts.action == QualityCountDispositionAction::Reject
        && (hold.status != "disposition_pending"
            || hold.count_id.as_deref() != Some(facts.count_id.as_str()))
    {
        return Err(changed());
    }
    if facts.action == QualityCountDispositionAction::Post {
        let quantity = hold.disposition.unwrap_or(0);
        let held = hold.held.unwrap_or(0);
        if hold.status != "disposition_pending"
            || hold.result.as_deref() != Some("fail")
            || hold.count_id.as_deref() != Some(facts.count_id.as_str())
            || quantity <= 0
            || quantity > held
        {
            return Err(changed());
        }
    }
    Ok(PreparedQualityCountDisposition {
        source,
        facts,
        hold,
        unit,
    })
}

/// Quality stage after disposal but before Inventory stock/lot writes.
/// Retaining this value is mandatory until `finish_after_count` succeeds; any
/// outer error must roll back the entire owner unit.
pub struct StagedQualityCountDisposition<S: QualityCountDispositionSource> {
    source: std::sync::Arc<S>,
    facts: QualityCountDispositionFacts,
    hold: Hold,
    unit: Unit,
    intermediate_hold_image: String,
}

pub struct RetainedQualityCountDisposition<S: QualityCountDispositionSource> {
    source: std::sync::Arc<S>,
    facts: QualityCountDispositionFacts,
    unit: Unit,
    hold_image: String,
    remaining: Option<(String, String)>,
    event_image: String,
    requester: Option<String>,
    posted_version: i64,
}
impl<S: QualityCountDispositionSource> PreparedQualityCountDisposition<S> {
    /// For a post, dispose the parent before Inventory adjusts stock. This UPDATE
    /// does not bump stock version under the current disposition trigger. For a
    /// reject this stage has no effect. The subsequent remaining-hold insert
    /// does bump stock version and must be included in final source checks.
    pub async fn apply_before_count(
        self,
        c: &mut PgConnection,
    ) -> Result<StagedQualityCountDisposition<S>, Error> {
        if unit(c).await? != self.unit || *self.source.facts() != self.facts {
            return Err(changed());
        }
        self.source.require_before(c).await?;
        if read_hold(c, &self.facts).await?.as_ref() != Some(&self.hold) {
            return Err(changed());
        }
        let mut sp = c.begin().await?;
        let result: Result<String, Error> = async {
            let f = &self.facts; let h = &self.hold;
            if f.action == QualityCountDispositionAction::Post {
                let quantity = h.disposition.unwrap_or(0); let held = h.held.unwrap_or(0);
                let n = sqlx::query("UPDATE inventory_quality_holds SET status='disposed',disposed_by=$1,disposed_at=$2,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND branch_id=$5 AND version=$6 AND status='disposition_pending' AND disposition_count_id=$7 AND disposition_quantity_milli=$8 AND held_quantity_milli=$9 AND last_result='fail'")
                    .bind(&f.actor_id).bind(f.now).bind(&h.id).bind(&h.company_id).bind(&h.branch_id).bind(h.version).bind(&f.count_id).bind(quantity).bind(held).execute(&mut *sp).await?.rows_affected();
                if n != 1 { return Err(changed()); }
            }
            Ok(sqlx::query_scalar("SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1").bind(&h.id).fetch_one(&mut *sp).await?)
        }.await;
        match result {
            Ok(intermediate_hold_image) => {
                sp.commit().await?;
                Ok(StagedQualityCountDisposition {
                    source: self.source,
                    facts: self.facts,
                    hold: self.hold,
                    unit: self.unit,
                    intermediate_hold_image,
                })
            }
            Err(error) => {
                sp.rollback().await?;
                Err(error)
            }
        }
    }
}
impl<S: QualityCountDispositionSource> StagedQualityCountDisposition<S> {
    pub fn source(&self) -> &S {
        self.source.as_ref()
    }
    pub fn facts(&self) -> &QualityCountDispositionFacts {
        &self.facts
    }
    /// Actual held lot from the locked Quality row, not the hold identifier.
    pub fn lot_id(&self) -> Option<&str> {
        self.hold.lot_id.as_deref()
    }
    pub async fn require_intermediate(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || *self.source.facts() != self.facts {
            return Err(changed());
        }
        let image: Option<String> = sqlx::query_scalar(
            "SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1",
        )
        .bind(&self.hold.id)
        .fetch_optional(&mut *c)
        .await?;
        if image.as_ref() != Some(&self.intermediate_hold_image) {
            return Err(changed());
        }
        Ok(())
    }
    /// Capture the remaining hold after Inventory effects and before Count decision.
    pub async fn stage_remaining(
        self,
        c: &mut PgConnection,
    ) -> Result<StagedQualityCountDecision<S>, Error> {
        self.require_intermediate(c).await?;
        self.source.require_count_before_decision(c, false).await?;
        let f = &self.facts;
        let h = &self.hold;
        let mut remaining = None;
        if f.action == QualityCountDispositionAction::Post {
            let quantity = h.disposition.unwrap_or(0);
            let held = h.held.unwrap_or(0);
            if held > quantity {
                let id = format!("{}-remaining-{}", h.id, f.count_id);
                let source_id = h
                    .source_id
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("{s}:remaining:{}", f.count_id));
                sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,version,created_at,updated_at,lot_id,held_quantity_milli,source_type,source_id,spec_id,spec_version,quantity_scope) VALUES($1,$2,$3,$4,$5,$6,$7,'held',$8,1,$9,$9,$10,$11,$12,$13,$14,$15,'partial')")
                    .bind(&id).bind(&h.company_id).bind(&h.branch_id).bind(&h.warehouse_id).bind(&h.product_id).bind(format!("Remaining quantity after partial quality scrap: {}", h.reason)).bind(&h.due_date).bind(&h.created_by).bind(f.now).bind(&h.lot_id).bind(held-quantity).bind(&h.source_type).bind(source_id).bind(&h.spec_id).bind(h.spec_version).execute(&mut *c).await?;
                let image: String = sqlx::query_scalar(
                    "SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1",
                )
                .bind(&id)
                .fetch_one(&mut *c)
                .await?;
                remaining = Some((id, image));
            }
        }
        self.source
            .require_count_before_decision(c, remaining.is_some())
            .await?;
        Ok(StagedQualityCountDecision {
            staged: self,
            remaining,
        })
    }
}

pub struct StagedQualityCountDecision<S: QualityCountDispositionSource> {
    staged: StagedQualityCountDisposition<S>,
    remaining: Option<(String, String)>,
}
impl<S: QualityCountDispositionSource> StagedQualityCountDecision<S> {
    pub fn source(&self) -> &S {
        self.staged.source.as_ref()
    }
    pub fn facts(&self) -> &QualityCountDispositionFacts {
        &self.staged.facts
    }
    pub fn remaining_hold_id(&self) -> Option<&str> {
        self.remaining.as_ref().map(|v| v.0.as_str())
    }
    /// Mint only after the actual child exists and all owner images are current.
    pub async fn child_notice_proof(
        &self,
        c: &mut PgConnection,
    ) -> Result<Option<QualityCountChildNoticeProof<S>>, Error> {
        self.require_before_decision(c).await?;
        Ok(self
            .remaining
            .as_ref()
            .map(|(id, image)| QualityCountChildNoticeProof {
                source: self.staged.source.clone(),
                facts: self.staged.facts.clone(),
                unit: self.staged.unit.clone(),
                hold_image: self.staged.intermediate_hold_image.clone(),
                child_id: id.clone(),
                child_image: image.clone(),
            }))
    }
    pub async fn require_before_decision(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.staged.require_intermediate(c).await?;
        self.staged
            .source
            .require_count_before_decision(c, self.remaining.is_some())
            .await?;
        if let Some((id, image)) = &self.remaining {
            let current: Option<String> = sqlx::query_scalar(
                "SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1",
            )
            .bind(id)
            .fetch_optional(&mut *c)
            .await?;
            if current.as_ref() != Some(image) {
                return Err(changed());
            }
        }
        Ok(())
    }
    pub async fn finish_after_count(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedQualityCountDisposition<S>, Error> {
        let staged = self.staged;
        staged.require_intermediate(c).await?;
        staged
            .source
            .require_count_after(c, self.remaining.is_some())
            .await?;
        if let Some((id, image)) = &self.remaining {
            let current: Option<String> = sqlx::query_scalar(
                "SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1",
            )
            .bind(id)
            .fetch_optional(&mut *c)
            .await?;
            if current.as_ref() != Some(image) {
                return Err(changed());
            }
        }
        let mut sp = c.begin().await?;
        let result: Result<(String,String), Error> = async {
            let f = &staged.facts; let h = &staged.hold;
            if f.action == QualityCountDispositionAction::Reject {
                let updated = sqlx::query("UPDATE inventory_quality_holds SET status='held',disposition_kind=NULL,disposition_reason=NULL,disposition_requested_by=NULL,disposition_count_id=NULL,disposition_quantity_milli=NULL,version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND branch_id=$4 AND version=$5 AND status='disposition_pending' AND disposition_count_id=$6")
                    .bind(f.now).bind(&h.id).bind(&h.company_id).bind(&h.branch_id).bind(h.version).bind(&f.count_id).execute(&mut *sp).await?.rows_affected();
                if updated != 1 { return Err(changed()); }
            }
            let action = match f.action { QualityCountDispositionAction::Post => "post", QualityCountDispositionAction::Reject => "reject" };
            let event_id = format!("quality-disposition-{}", f.command_key);
            sqlx::query("INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(&event_id).bind(&h.id).bind(&f.count_id).bind(action).bind(&f.actor_id).bind(h.version).bind(&f.reason).bind(f.now).execute(&mut *sp).await?;
            staged.source.require_count_after(&mut sp, self.remaining.is_some()).await?;
            let hold_image: String = sqlx::query_scalar("SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1").bind(&h.id).fetch_one(&mut *sp).await?;
            let event_image: String = sqlx::query_scalar("SELECT to_jsonb(e)::text FROM inventory_quality_disposition_events e WHERE id=$1").bind(event_id).fetch_one(&mut *sp).await?;
            Ok((hold_image,event_image))
        }.await;
        match result {
            Ok((hold_image, event_image)) => {
                sp.commit().await?;
                Ok(RetainedQualityCountDisposition {
                    source: staged.source,
                    facts: staged.facts,
                    unit: staged.unit,
                    hold_image,
                    remaining: self.remaining,
                    event_image,
                    requester: staged.hold.requester,
                    posted_version: staged.hold.version.checked_add(1).ok_or_else(changed)?,
                })
            }
            Err(error) => {
                sp.rollback().await?;
                Err(error)
            }
        }
    }
}

/// Opaque same-unit child proof survives consuming the pre-decision stage.
pub struct QualityCountChildNoticeProof<S: QualityCountDispositionSource> {
    source: std::sync::Arc<S>,
    facts: QualityCountDispositionFacts,
    unit: Unit,
    hold_image: String,
    child_id: String,
    child_image: String,
}
impl<S: QualityCountDispositionSource> QualityCountChildNoticeProof<S> {
    pub fn facts(&self) -> &QualityCountDispositionFacts {
        &self.facts
    }
    pub fn child_id(&self) -> &str {
        &self.child_id
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || *self.source.facts() != self.facts {
            return Err(changed());
        }
        if self
            .source
            .require_count_before_decision(c, true)
            .await
            .is_err()
        {
            self.source.require_count_after(c, true).await?;
        }
        let parent: Option<String> = sqlx::query_scalar(
            "SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1",
        )
        .bind(&self.facts.hold_id)
        .fetch_optional(&mut *c)
        .await?;
        let child: Option<String> = sqlx::query_scalar(
            "SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1",
        )
        .bind(&self.child_id)
        .fetch_optional(&mut *c)
        .await?;
        if parent.as_ref() != Some(&self.hold_image) || child.as_ref() != Some(&self.child_image) {
            return Err(changed());
        }
        Ok(())
    }
}
/// Requester identity/version derive from the actual finalized parent hold.
pub struct QualityCountRequesterNoticeProof<S: QualityCountDispositionSource> {
    retained: std::sync::Arc<RetainedQualityCountDisposition<S>>,
}
impl<S: QualityCountDispositionSource> QualityCountRequesterNoticeProof<S> {
    pub fn facts(&self) -> &QualityCountDispositionFacts {
        &self.retained.facts
    }
    pub fn requester(&self) -> &str {
        self.retained
            .requester
            .as_deref()
            .expect("proof requires requester")
    }
    pub fn version(&self) -> i64 {
        self.retained.posted_version
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.retained.require_current(c).await
    }
}

impl<S: QualityCountDispositionSource> RetainedQualityCountDisposition<S> {
    pub(crate) fn disposition_facts(&self) -> &QualityCountDispositionFacts {
        &self.facts
    }
    pub fn requester_notice_proof(
        self: &std::sync::Arc<Self>,
    ) -> Option<QualityCountRequesterNoticeProof<S>> {
        if self.facts.action != QualityCountDispositionAction::Post || self.requester.is_none() {
            return None;
        }
        Some(QualityCountRequesterNoticeProof {
            retained: self.clone(),
        })
    }
    pub fn remaining_hold_id(&self) -> Option<&str> {
        self.remaining.as_ref().map(|v| v.0.as_str())
    }
    pub fn requester(&self) -> Option<&str> {
        self.requester.as_deref()
    }
    /// Call again after Workflow, Finance and notification effects, before commit.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || *self.source.facts() != self.facts {
            return Err(changed());
        }
        self.source
            .require_count_after(c, self.remaining.is_some())
            .await?;
        let hold: Option<String> = sqlx::query_scalar(
            "SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1",
        )
        .bind(&self.facts.hold_id)
        .fetch_optional(&mut *c)
        .await?;
        let event: Option<String> = sqlx::query_scalar(
            "SELECT to_jsonb(e)::text FROM inventory_quality_disposition_events e WHERE id=$1",
        )
        .bind(format!("quality-disposition-{}", self.facts.command_key))
        .fetch_optional(&mut *c)
        .await?;
        if hold.as_ref() != Some(&self.hold_image) || event.as_ref() != Some(&self.event_image) {
            return Err(changed());
        }
        if let Some((id, image)) = &self.remaining {
            let current: Option<String> = sqlx::query_scalar(
                "SELECT to_jsonb(h)::text FROM inventory_quality_holds h WHERE id=$1",
            )
            .bind(id)
            .fetch_optional(&mut *c)
            .await?;
            if current.as_ref() != Some(image) {
                return Err(changed());
            }
        }
        Ok(())
    }
}
