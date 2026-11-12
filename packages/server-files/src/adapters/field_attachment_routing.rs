//! No-TEMP Files routing source for repository-provisioned public Holding schema.
//! This witness retains selected link identity and FieldVisit routing booleans,
//! not an exact full ancestry preimage or present authority. Consumers must use
//! lineage only for routing and separately retain selected authority/target, then
//! re-evaluate all owner predicates after deferred effects. SQL/TOAST memory is
//! not bounded and selected target strings are not artificially capped.
use super::field_attachment_ancestry::FieldVisitRouting;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, Vec<i64>);
type Selected = (
    (
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    ),
    (
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        String,
        i64,
        i64,
    ),
);
/// Audited Files-owned source query. $1 is selected link, $2 selected Company;
/// consumers may wrap this fixed query as a CTE but never accept request SQL.
pub const FIELD_ATTACHMENT_ANCESTRY_SQL: &str = r#"WITH root AS (
 SELECT l.id link_id,l.attachment_id,l.upload_intent_id,l.set_id
 FROM public.attachment_links l JOIN public.attachment_sets s ON s.id=l.set_id
 WHERE l.id=$1 AND s.company_id=$2
), direct AS (
 SELECT r.link_id,r.attachment_id,r.upload_intent_id,r.set_id,
        s.target_type st,s.target_id sr,u.owner_type ut,u.owner_id ur,us.target_type ust,us.target_id usr
 FROM root r LEFT JOIN public.attachment_sets s ON s.id=r.set_id
 LEFT JOIN public.upload_intents u ON u.id=r.upload_intent_id
 LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id
), att AS (
 SELECT a.id attachment_id,a.upload_id,u.attachment_set_id,
        a.owner_type at,a.owner_id ar,u.owner_type ut,u.owner_id ur,s.target_type st,s.target_id sr
 FROM root r JOIN public.attachments a ON a.id=r.attachment_id
 LEFT JOIN public.upload_intents u ON u.id=a.upload_id LEFT JOIN public.attachment_sets s ON s.id=u.attachment_set_id
), links AS (
 SELECT l.id link_id,l.attachment_id,l.upload_intent_id,l.set_id,u.attachment_set_id,
        s.target_type st,s.target_id sr,u.owner_type ut,u.owner_id ur,us.target_type ust,us.target_id usr
 FROM root r JOIN public.attachment_links l ON l.attachment_id=r.attachment_id
 LEFT JOIN public.attachment_sets s ON s.id=l.set_id LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id
 LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id
)
 SELECT v.path,d.link_id,d.attachment_id,d.upload_intent_id,d.set_id,NULL::text upload_set_id,v.kind,v.target FROM direct d CROSS JOIN LATERAL (VALUES ('direct-set',d.st,d.sr),('direct-upload',d.ut,d.ur),('direct-upload-set',d.ust,d.usr)) v(path,kind,target)
 UNION ALL SELECT v.path,NULL::text,a.attachment_id,a.upload_id,NULL::text,a.attachment_set_id,v.kind,v.target FROM att a CROSS JOIN LATERAL (VALUES ('source-attachment',a.at,a.ar),('source-upload',a.ut,a.ur),('source-upload-set',a.st,a.sr)) v(path,kind,target)
 UNION ALL SELECT v.path,l.link_id,l.attachment_id,l.upload_intent_id,l.set_id,l.attachment_set_id,v.kind,v.target FROM links l CROSS JOIN LATERAL (VALUES ('linked-set',l.st,l.sr),('linked-upload',l.ut,l.ur),('linked-upload-set',l.ust,l.usr)) v(path,kind,target)"#;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FILES_ATTACHMENT_ROUTING_CHANGED",
        "The selected Files routing source changed.",
    )
    .into()
}
/// Acquire before *any* snapshot alongside all participating owners. NOWAIT
/// conflict requires retry of the entire outer transaction. Only public is
/// supported: repository provisioning grants the Holding runtime that schema.
pub async fn lock_field_attachment_routing_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_ROUTING_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.attachment_sets,public.attachment_links,public.attachments,public.upload_intents IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FILES_ROUTING_SERIALIZABLE_REQUIRED").into());
    }
    let valid:bool=sqlx::query_scalar("SELECT count(*)=4 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.attachment_sets'::regclass,'public.attachment_links'::regclass,'public.attachments'::regclass,'public.upload_intents'::regclass)").fetch_one(&mut *c).await?;
    if !valid {
        return Err(PersistenceError::new("FILES_ROUTING_LOCKS_OR_RLS_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,current_setting('search_path'),ARRAY['public.attachment_sets'::regclass::oid::bigint,'public.attachment_links'::regclass::oid::bigint,'public.attachments'::regclass::oid::bigint,'public.upload_intents'::regclass::oid::bigint]").fetch_one(c).await?)
}
async fn selected(
    c: &mut PgConnection,
    company: &str,
    link: &str,
) -> Result<Option<Selected>, Error> {
    let row=sqlx::query("SELECT l.id,s.id,s.company_id,s.target_type,s.target_id,l.attachment_id,l.upload_intent_id,l.state,s.state,s.branch_id,s.department_id,s.component_kind,s.component_ref,s.purpose_key,s.expected_target_version,s.policy_version FROM public.attachment_links l JOIN public.attachment_sets s ON s.id=l.set_id WHERE l.id=$1 AND s.company_id=$2").bind(link).bind(company).fetch_optional(c).await?;
    row.map(|r| {
        Ok((
            (
                r.try_get(0)?,
                r.try_get(1)?,
                r.try_get(2)?,
                r.try_get(3)?,
                r.try_get(4)?,
                r.try_get(5)?,
                r.try_get(6)?,
                r.try_get(7)?,
            ),
            (
                r.try_get(8)?,
                r.try_get(9)?,
                r.try_get(10)?,
                r.try_get(11)?,
                r.try_get(12)?,
                r.try_get(13)?,
                r.try_get(14)?,
                r.try_get(15)?,
            ),
        ))
    })
    .transpose()
}
async fn visit(
    c: &mut PgConnection,
    company: &str,
    link: &str,
) -> Result<FieldVisitRouting, Error> {
    let sql = format!(
        "WITH file_ancestry AS ({FIELD_ATTACHMENT_ANCESTRY_SQL}) SELECT EXISTS(SELECT 1 FROM file_ancestry WHERE path='direct-set' AND kind='field_service_visit'),EXISTS(SELECT 1 FROM file_ancestry WHERE path IN ('direct-set','direct-upload','direct-upload-set') AND kind='field_service_visit'),EXISTS(SELECT 1 FROM file_ancestry WHERE path IN ('source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set') AND kind='field_service_visit')"
    );
    let (direct_set, direct, source): (bool, bool, bool) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(link)
        .bind(company)
        .fetch_one(c)
        .await?;
    Ok(FieldVisitRouting {
        direct_set,
        direct,
        source,
    })
}
/// Routing-only proof; this does not prove unchanged ancestry or authority.
pub struct CurrentFieldAttachmentRouting {
    unit: Unit,
    selected: Selected,
    visit: FieldVisitRouting,
    company: String,
    link: String,
}
pub async fn inspect_field_attachment_routing(
    c: &mut PgConnection,
    company: &CompanyId,
    link: &str,
) -> Result<Option<CurrentFieldAttachmentRouting>, Error> {
    let captured_unit = unit(&mut *c).await?;
    let Some(selected_image) = selected(&mut *c, company.as_str(), link).await? else {
        return Ok(None);
    };
    let visit_image = visit(&mut *c, company.as_str(), link).await?;
    if unit(&mut *c).await? != captured_unit
        || selected(&mut *c, company.as_str(), link).await?.as_ref() != Some(&selected_image)
        || visit(&mut *c, company.as_str(), link).await? != visit_image
    {
        return Err(changed());
    }
    Ok(Some(CurrentFieldAttachmentRouting {
        unit: captured_unit,
        selected: selected_image,
        visit: visit_image,
        company: company.as_str().into(),
        link: link.into(),
    }))
}

/// Retains either the selected Company's link routing witness or its absence.
/// Absence proves only the scoped link lookup; it does not establish a global
/// ancestry negative, target permission, or an HTTP not-found result.
pub struct CurrentFieldAttachmentRoutingObservation {
    unit: Unit,
    company: String,
    link: String,
    routing: Option<CurrentFieldAttachmentRouting>,
}
pub async fn observe_field_attachment_routing(
    c: &mut PgConnection,
    company: &CompanyId,
    link: &str,
) -> Result<CurrentFieldAttachmentRoutingObservation, Error> {
    let captured_unit = unit(&mut *c).await?;
    let routing = inspect_field_attachment_routing(&mut *c, company, link).await?;
    if unit(&mut *c).await? != captured_unit {
        return Err(changed());
    }
    if let Some(current) = &routing {
        current.require_current(&mut *c).await?;
    } else if selected(&mut *c, company.as_str(), link).await?.is_some() {
        return Err(changed());
    }
    if unit(&mut *c).await? != captured_unit {
        return Err(changed());
    }
    Ok(CurrentFieldAttachmentRoutingObservation {
        unit: captured_unit,
        company: company.as_str().into(),
        link: link.into(),
        routing,
    })
}
impl CurrentFieldAttachmentRoutingObservation {
    pub fn company_id(&self) -> &str {
        &self.company
    }
    pub fn selected_link_id(&self) -> &str {
        &self.link
    }
    pub fn routing(&self) -> Option<&CurrentFieldAttachmentRouting> {
        self.routing.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit {
            return Err(changed());
        }
        if let Some(current) = &self.routing {
            current.require_current(&mut *c).await?;
        } else if selected(&mut *c, &self.company, &self.link)
            .await?
            .is_some()
        {
            return Err(changed());
        }
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        Ok(())
    }
}
/// Actual selected Files route fields, without any target authority or
/// assertion that the global Field/Finance routing decision is complete.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentAttachmentReadRoute<'a> {
    pub set_id: &'a str,
    pub target_type: &'a str,
    pub target_id: &'a str,
    pub purpose_key: &'a str,
    pub component_kind: &'a str,
    pub component_ref: Option<&'a str>,
}
impl CurrentFieldAttachmentRouting {
    /// Mirrors attachmentReadRoute; use only after selected source revalidation.
    pub fn read_route(&self) -> CurrentAttachmentReadRoute<'_> {
        CurrentAttachmentReadRoute {
            set_id: &self.selected.0.1,
            target_type: &self.selected.0.3,
            target_id: &self.selected.0.4,
            purpose_key: &self.selected.1.5,
            component_kind: &self.selected.1.3,
            component_ref: self.selected.1.4.as_deref(),
        }
    }
    pub fn selected_link_id(&self) -> &str {
        &self.link
    }
    pub fn company_id(&self) -> &str {
        &self.company
    }
    pub fn field_visit_routing(&self) -> FieldVisitRouting {
        self.visit
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || selected(&mut *c, &self.company, &self.link).await?.as_ref() != Some(&self.selected)
            || visit(&mut *c, &self.company, &self.link).await? != self.visit
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}
// Trusted composition must use this concrete Files witness. The public trait
// is not sealed and arbitrary implementations must never be accepted from a
// request; CTE SQL is the fixed Files-owned constant above.
impl erpx_server_workflow::public::record_origin::FieldAttachmentRoutingSource
    for CurrentFieldAttachmentRouting
{
    fn selected_link_id(&self) -> &str {
        &self.link
    }
    fn company_id(&self) -> &str {
        &self.company
    }
    fn ancestry_sql(&self) -> &'static str {
        FIELD_ATTACHMENT_ANCESTRY_SQL
    }
    fn require_current<'a>(
        &'a self,
        c: &'a mut PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move { CurrentFieldAttachmentRouting::require_current(self, c).await })
    }
}

/// Global Field Visit inputs for the TS link marker only. Finance payment and
/// journal markers are separate earlier branches; this is never a complete
/// routing decision, selected-Company authority, or an authorization grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalFieldVisitLinkFacts {
    pub marker: bool,
    pub scoped: bool,
}
pub struct CurrentGlobalFieldVisitLink {
    unit: Unit,
    company: CompanyId,
    link: String,
    facts: GlobalFieldVisitLinkFacts,
}
async fn global_field_visit_link(
    c: &mut PgConnection,
    company: &CompanyId,
    link: &str,
) -> Result<GlobalFieldVisitLinkFacts, Error> {
    let (marker,scoped):(bool,bool)=sqlx::query_as(r#"SELECT EXISTS(
 SELECT 1 FROM public.attachment_links l
 LEFT JOIN public.attachment_sets s ON s.id=l.set_id
 LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id
 LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id
 WHERE l.id=$1 AND (s.target_type='field_service_visit' OR u.owner_type='field_service_visit' OR us.target_type='field_service_visit' OR EXISTS(
   SELECT 1 FROM public.attachments a LEFT JOIN public.upload_intents au ON au.id=a.upload_id LEFT JOIN public.attachment_sets aset ON aset.id=au.attachment_set_id
   WHERE a.id=l.attachment_id AND (a.owner_type='field_service_visit' OR au.owner_type='field_service_visit' OR aset.target_type='field_service_visit')
 ) OR EXISTS(
   SELECT 1 FROM public.attachment_links al LEFT JOIN public.attachment_sets als ON als.id=al.set_id LEFT JOIN public.upload_intents alu ON alu.id=al.upload_intent_id LEFT JOIN public.attachment_sets alus ON alus.id=alu.attachment_set_id
   WHERE al.attachment_id=l.attachment_id AND (als.target_type='field_service_visit' OR alu.owner_type='field_service_visit' OR alus.target_type='field_service_visit')
 ))) ,EXISTS(SELECT 1 FROM public.attachment_links l JOIN public.attachment_sets s ON s.id=l.set_id WHERE l.id=$1 AND s.company_id=$2)"#).bind(link).bind(company.as_str()).fetch_one(c).await?;
    Ok(GlobalFieldVisitLinkFacts { marker, scoped })
}
/// Locks and actual Company are coordinator-supplied; the witness retains the
/// global marker and scoped existence independently, including foreign markers.
pub async fn observe_global_field_visit_link(
    c: &mut PgConnection,
    company: &CompanyId,
    link: &str,
) -> Result<CurrentGlobalFieldVisitLink, Error> {
    let identity = unit(&mut *c).await?;
    let facts = global_field_visit_link(&mut *c, company, link).await?;
    if unit(&mut *c).await? != identity
        || global_field_visit_link(&mut *c, company, link).await? != facts
    {
        return Err(changed());
    }
    Ok(CurrentGlobalFieldVisitLink {
        unit: identity,
        company: company.clone(),
        link: link.into(),
        facts,
    })
}
impl CurrentGlobalFieldVisitLink {
    pub fn facts(&self) -> GlobalFieldVisitLinkFacts {
        self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || global_field_visit_link(&mut *c, &self.company, &self.link).await? != self.facts
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Global raw Files ancestry for the attachment ID actually retained on a
/// selected link. This retains no global row image and grants no authority.
/// Consumers must re-evaluate their complete joined decision after effects.
pub struct CurrentGlobalAttachmentAncestry<'a> {
    source: &'a CurrentFieldAttachmentRouting,
}
impl CurrentGlobalAttachmentAncestry<'_> {
    /// Fixed comma-separated CTE bodies. Bind $1 to selected_link_id and $2 to
    /// company_id. The roots are scoped only to obtain the actual selected link;
    /// ancestry joins have no Company, state or revocation filters.
    pub fn cte_sql(&self) -> &'static str {
        r#"file_global_selected AS (
 SELECT l.attachment_id FROM public.attachment_links l JOIN public.attachment_sets s ON s.id=l.set_id WHERE l.id=$1 AND s.company_id=$2
), file_global_attachment_source AS (
 SELECT a.id AS attachment_id,a.owner_type AS attachment_owner_type,a.owner_id AS attachment_owner_id,
        u.owner_type AS upload_owner_type,u.owner_id AS upload_owner_id,
        s.target_type AS upload_set_type,s.target_id AS upload_set_id
 FROM public.attachments a LEFT JOIN public.upload_intents u ON u.id=a.upload_id LEFT JOIN public.attachment_sets s ON s.id=u.attachment_set_id
 WHERE a.id=(SELECT attachment_id FROM file_global_selected)
), file_global_related_links AS (
 SELECT l.id AS link_id,s.target_type AS set_type,s.target_id AS set_id,
        u.owner_type AS upload_owner_type,u.owner_id AS upload_owner_id,
        us.target_type AS upload_set_type,us.target_id AS upload_set_id
 FROM public.attachment_links l LEFT JOIN public.attachment_sets s ON s.id=l.set_id
 LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id
 WHERE l.attachment_id=(SELECT attachment_id FROM file_global_selected)
)"#
    }
    pub fn selected_link_id(&self) -> &str {
        self.source.selected_link_id()
    }
    pub fn company_id(&self) -> &str {
        self.source.company_id()
    }
    /// None means the selected link exists but has no attachment ID. It is not
    /// the same as a missing selected link (which cannot construct this witness).
    pub fn attachment_id(&self) -> Option<&str> {
        self.source.selected.0.5.as_deref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.source.require_current(c).await
    }
}
pub async fn observe_global_attachment_ancestry<'a>(
    c: &mut PgConnection,
    source: &'a CurrentFieldAttachmentRouting,
) -> Result<CurrentGlobalAttachmentAncestry<'a>, Error> {
    source.require_current(c).await?;
    Ok(CurrentGlobalAttachmentAncestry { source })
}

/// Fixed unscoped Files link source for the first two TS routing phases.
/// It retains source selection only, never a Finance/Field classification,
/// target authority, personal authority or purchase admission.
pub struct CurrentUnscopedAttachmentLink {
    unit: Unit,
    company: CompanyId,
    link: String,
    image: Option<String>,
    scoped: bool,
}
async fn unscoped_link_image(
    c: &mut PgConnection,
    company: &CompanyId,
    link: &str,
) -> Result<(Option<String>, bool), Error> {
    let row:(Option<String>,bool)=sqlx::query_as(r#"SELECT (
 SELECT jsonb_build_array(l.id,l.set_id,l.attachment_id,l.upload_intent_id,
 s.id,s.company_id,s.target_type,s.target_id,u.id,u.owner_type,u.owner_id,u.attachment_set_id,
 us.id,us.target_type,us.target_id,a.id,a.owner_type,a.owner_id)::text
 FROM public.attachment_links l LEFT JOIN public.attachment_sets s ON s.id=l.set_id
 LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id
 LEFT JOIN public.attachments a ON a.id=l.attachment_id WHERE l.id=$1
 ),EXISTS(SELECT 1 FROM public.attachment_links l JOIN public.attachment_sets s ON s.id=l.set_id WHERE l.id=$1 AND s.company_id=$2)"#)
 .bind(link).bind(company.as_str()).fetch_one(c).await?;
    Ok(row)
}
/// Bind only after all owner utility locks have been acquired before snapshot.
/// The selected link may be absent or global and foreign; scoped is retained
/// separately exactly as the TS marker queries do.
pub async fn observe_unscoped_attachment_link(
    c: &mut PgConnection,
    company: &CompanyId,
    link: &str,
) -> Result<CurrentUnscopedAttachmentLink, Error> {
    let identity = unit(&mut *c).await?;
    let (image, scoped) = unscoped_link_image(&mut *c, company, link).await?;
    if unit(&mut *c).await? != identity
        || unscoped_link_image(&mut *c, company, link).await? != (image.clone(), scoped)
    {
        return Err(changed());
    }
    Ok(CurrentUnscopedAttachmentLink {
        unit: identity,
        company: company.clone(),
        link: link.into(),
        image,
        scoped,
    })
}
impl CurrentUnscopedAttachmentLink {
    pub fn selected_link_id(&self) -> &str {
        &self.link
    }
    pub fn company_id(&self) -> &str {
        self.company.as_str()
    }
    pub fn exists(&self) -> bool {
        self.image.is_some()
    }
    pub fn scoped(&self) -> bool {
        self.scoped
    }
    /// Fixed CTE fragments: bind $1 to selected_link_id, $2 to company_id.
    /// Source columns are selected individually to preserve SQL collations;
    /// retain NULLs and evaluate each phase's exact predicates separately.
    pub fn cte_sql(&self) -> &'static str {
        r#"file_unscoped_link AS (
 SELECT l.id AS link_id,l.set_id,l.attachment_id,l.upload_intent_id,
 s.target_type AS set_type,s.target_id AS set_target,
 u.owner_type AS upload_type,u.owner_id AS upload_target,
 us.target_type AS upload_set_type,us.target_id AS upload_set_target,
 a.owner_type AS attachment_type,a.owner_id AS attachment_target
 FROM public.attachment_links l LEFT JOIN public.attachment_sets s ON s.id=l.set_id
 LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id
 LEFT JOIN public.attachments a ON a.id=l.attachment_id WHERE l.id=$1
), file_link_scope AS (
 SELECT EXISTS(SELECT 1 FROM public.attachment_links l JOIN public.attachment_sets s ON s.id=l.set_id WHERE l.id=$1 AND s.company_id=$2) AS scoped
)"#
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || unscoped_link_image(&mut *c, &self.company, &self.link).await?
                != (self.image.clone(), self.scoped)
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Global attachment ancestry rooted in an unscoped link, including foreign
/// links. This is routing input only, never an authority or full marker.
/// The borrowed source retains link identity and Company scope separately.
/// CTE rows are not snapshotted: evaluate the complete joined decision again
/// after all owner effects and retain the other owners' witnesses independently.
pub struct CurrentUnscopedGlobalAttachmentAncestry<'a> {
    source: &'a CurrentUnscopedAttachmentLink,
}
impl CurrentUnscopedGlobalAttachmentAncestry<'_> {
    /// Append these fixed bodies after `source_cte_sql()`. Bind $1 and $2 from
    /// this witness, never a caller-supplied attachment ID or SQL fragment.
    pub fn cte_sql(&self) -> &'static str {
        r#"file_global_attachment_source AS (
 SELECT a.id AS attachment_id,a.owner_type AS attachment_owner_type,a.owner_id AS attachment_owner_id,
        u.owner_type AS upload_owner_type,u.owner_id AS upload_owner_id,
        s.target_type AS upload_set_type,s.target_id AS upload_set_id
 FROM public.attachments a LEFT JOIN public.upload_intents u ON u.id=a.upload_id LEFT JOIN public.attachment_sets s ON s.id=u.attachment_set_id
 WHERE a.id=(SELECT attachment_id FROM file_unscoped_link)
), file_global_related_links AS (
 SELECT l.id AS link_id,s.target_type AS set_type,s.target_id AS set_id,
        u.owner_type AS upload_owner_type,u.owner_id AS upload_owner_id,
        us.target_type AS upload_set_type,us.target_id AS upload_set_id
 FROM public.attachment_links l LEFT JOIN public.attachment_sets s ON s.id=l.set_id
 LEFT JOIN public.upload_intents u ON u.id=l.upload_intent_id LEFT JOIN public.attachment_sets us ON us.id=u.attachment_set_id
 WHERE l.attachment_id=(SELECT attachment_id FROM file_unscoped_link)
)"#
    }
    pub fn source_cte_sql(&self) -> &'static str {
        self.source.cte_sql()
    }
    pub fn selected_link_id(&self) -> &str {
        self.source.selected_link_id()
    }
    pub fn company_id(&self) -> &str {
        self.source.company_id()
    }
    /// `exists=false` is distinct from an existing link with a NULL attachment.
    pub fn link_exists(&self) -> bool {
        self.source.exists()
    }
    pub fn scoped(&self) -> bool {
        self.source.scoped()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.source.require_current(c).await
    }
}
pub async fn observe_unscoped_global_attachment_ancestry<'a>(
    c: &mut PgConnection,
    source: &'a CurrentUnscopedAttachmentLink,
) -> Result<CurrentUnscopedGlobalAttachmentAncestry<'a>, Error> {
    source.require_current(c).await?;
    Ok(CurrentUnscopedGlobalAttachmentAncestry { source })
}
