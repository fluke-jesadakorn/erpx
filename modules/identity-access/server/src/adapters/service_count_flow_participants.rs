//! Current Identity roster facts for Stock Count Flow. People supplies the
//! active staff roster; this owner evaluates only IAM assignments/principals.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn invalid() -> Error {
    PersistenceError::new("STOCK_COUNT_FLOW_PARTICIPANTS_INVALID").into()
}
/// Acquire the complete Identity authority and workflow permission fences as
/// utility locks, before the enclosing Holding's first data snapshot.
pub async fn lock_stock_count_flow_assignment_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    super::postgres_effective_authority::lock_effective_authority_sources(&mut *c).await?;
    super::permission_catalog_lock::lock_permission_catalog_sources(c).await
}
/// Caller first holds Identity and People owner locks; facts do not grant any
/// action and must be read again before transaction completion.
pub async fn service_count_flow_reviewers(
    c: &mut PgConnection,
    company: &CompanyId,
    active_staff: &[String],
    creator: &str,
    preparer: &str,
    now: i64,
) -> Result<Vec<String>, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    // TS concatenates the sorted finance pool before the sorted holding-owner
    // pool and removes duplicate users while preserving first occurrence.
    Ok(sqlx::query_scalar("WITH eligible AS MATERIALIZED (SELECT a.user_id,a.role,a.company_id FROM assignments a WHERE a.company_id=$1 AND a.user_id=ANY($2::text[]) AND a.user_id<>$3 AND a.user_id<>$4 AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=$5 AND a.valid_until>$5), pools AS (SELECT user_id,0 priority FROM eligible WHERE iam_role_allows(role,company_id,'workflow.finance_approver') UNION ALL SELECT user_id,1 priority FROM eligible WHERE iam_role_allows(role,company_id,'workflow.holding_owner')), first_pool AS (SELECT user_id,min(priority) priority FROM pools GROUP BY user_id) SELECT user_id FROM first_pool ORDER BY priority,user_id").bind(company.as_str()).bind(active_staff).bind(creator).bind(preparer).bind(now).fetch_all(c).await?)
}
/// Identities allowed as explicitly named owner/preparer even without HR.
pub async fn service_count_flow_active_members(
    c: &mut PgConnection,
    company: &CompanyId,
    now: i64,
) -> Result<Vec<String>, Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    // Legacy reads all qualifying members; retain the complete result without
    // treating UTF-8 byte length as a user identifier's UTF-16 length.
    let ids:Vec<String>=sqlx::query_scalar("SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=$1 AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=$2 AND a.valid_until>$2 ORDER BY a.user_id").bind(company.as_str()).bind(now).fetch_all(c).await?;
    Ok(ids)
}

type FlowIdentityUnit = (
    String,
    i32,
    String,
    String,
    String,
    String,
    String,
    Vec<i64>,
);
#[derive(PartialEq, Eq)]
struct FlowAssignmentImage {
    assignment: String,
    assignment_active: bool,
    principal_active: bool,
    finance: bool,
    owner: bool,
}
#[derive(PartialEq, Eq)]
struct FlowIdentityImage {
    assignments: Vec<FlowAssignmentImage>,
    roles: Vec<String>,
    revisions: Vec<String>,
    permissions: Vec<String>,
}
/// Assignment projection and evaluated IAM predicates, not an actor grant.
/// The caller must separately witness People and Organization source data.
pub struct CurrentStockCountFlowAssignments {
    unit: FlowIdentityUnit,
    company: CompanyId,
    now: i64,
    image: FlowIdentityImage,
}
impl CurrentStockCountFlowAssignments {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if flow_identity_unit(c).await? != self.unit
            || flow_identity_image(c, &self.company, self.now).await? != self.image
            || flow_identity_unit(c).await? != self.unit
        {
            return Err(PersistenceError::new("STOCK_COUNT_FLOW_IDENTITY_CHANGED").into());
        }
        Ok(())
    }
    pub async fn bind_projection<'a>(
        &'a self,
        c: &'a mut PgConnection,
    ) -> Result<BoundStockCountFlowAssignments<'a>, Error> {
        self.require_current(c).await?;
        Ok(BoundStockCountFlowAssignments {
            source: self,
            connection: c,
        })
    }
}
/// A fixed owner projection bound to the same witnessed connection.
#[must_use = "finish the projection to revalidate the Identity witness"]
pub struct BoundStockCountFlowAssignments<'a> {
    source: &'a CurrentStockCountFlowAssignments,
    connection: &'a mut PgConnection,
}
impl BoundStockCountFlowAssignments<'_> {
    pub fn assignments_sql(&self) -> &'static str {
        r#"SELECT "id","user_id","role","modules_json","company_id","branch_id","valid_from","valid_until","revoked_at","version","scope_kind","scope_node_id","parent_id","source","source_id" FROM public.assignments"#
    }
    pub fn connection(&mut self) -> &mut PgConnection {
        self.connection
    }
    /// The caller must finish after its composed query and before using facts.
    pub async fn finish(self) -> Result<(), Error> {
        self.source.require_current(self.connection).await
    }
}
/// Caller must acquire the complete Identity and permission catalog fences,
/// together with other owners' fences, before the first protected snapshot.
pub async fn inspect_stock_count_flow_assignments(
    c: &mut PgConnection,
    company: &CompanyId,
    now: i64,
) -> Result<CurrentStockCountFlowAssignments, Error> {
    let unit = flow_identity_unit(c).await?;
    let image = flow_identity_image(c, company, now).await?;
    if flow_identity_unit(c).await? != unit {
        return Err(invalid());
    }
    Ok(CurrentStockCountFlowAssignments {
        unit,
        company: company.clone(),
        now,
        image,
    })
}
async fn flow_identity_image(
    c: &mut PgConnection,
    company: &CompanyId,
    now: i64,
) -> Result<FlowIdentityImage, Error> {
    use sqlx::Row;
    let rows=sqlx::query("SELECT to_jsonb(a)::text image,iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) assignment_active,iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) principal_active,iam_role_allows(a.role,a.company_id,'workflow.finance_approver') finance,iam_role_allows(a.role,a.company_id,'workflow.holding_owner') owner FROM public.assignments a WHERE a.company_id=$1 ORDER BY a.id")
        .bind(company.as_str()).fetch_all(&mut *c).await?;
    let assignments = rows
        .into_iter()
        .map(|r| {
            Ok(FlowAssignmentImage {
                assignment: r.try_get("image")?,
                assignment_active: r.try_get("assignment_active")?,
                principal_active: r.try_get("principal_active")?,
                finance: r.try_get("finance")?,
                owner: r.try_get("owner")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let roles=sqlx::query_scalar("SELECT to_jsonb(r)::text FROM public.iam_roles r WHERE company_id IN ('*',$1) ORDER BY company_id,id").bind(company.as_str()).fetch_all(&mut *c).await?;
    let revisions=sqlx::query_scalar("SELECT to_jsonb(r)::text FROM public.iam_role_revisions r WHERE company_id IN ('*',$1) ORDER BY company_id,role_id,revision").bind(company.as_str()).fetch_all(&mut *c).await?;
    let permissions=sqlx::query_scalar("SELECT to_jsonb(p)::text FROM public.iam_permissions p WHERE id LIKE 'workflow.%' ORDER BY id").fetch_all(&mut *c).await?;
    // Preserve the caller's policy time and reevaluate valid_from/until at the
    // same value; full assignment bytes already retain those fields.
    let _ = now;
    Ok(FlowIdentityImage {
        assignments,
        roles,
        revisions,
        permissions,
    })
}
async fn flow_identity_unit(c: &mut PgConnection) -> Result<FlowIdentityUnit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    super::postgres_effective_authority::require_effective_authority_sources_held(&mut *c).await?;
    super::permission_catalog_lock::require_permission_catalog_lock(&mut *c).await?;
    let functions:bool=sqlx::query_scalar("SELECT to_regprocedure('iam_assignment_active(text,bigint)')=to_regprocedure('public.iam_assignment_active(text,bigint)') AND to_regprocedure('iam_principal_active(text,text,bigint)')=to_regprocedure('public.iam_principal_active(text,text,bigint)') AND to_regprocedure('iam_role_allows(text,text,text)')=to_regprocedure('public.iam_role_allows(text,text,text)')").fetch_one(&mut *c).await?;
    if !functions {
        return Err(PersistenceError::new("STOCK_COUNT_FLOW_IDENTITY_FUNCTION_INVALID").into());
    }
    let (valid,oids):(bool,Vec<i64>)=sqlx::query_as("WITH names(name) AS (SELECT unnest(ARRAY['assignments','iam_roles','iam_role_revisions','iam_members','iam_identities','iam_connections','iam_revoked_sessions','iam_emergency_sessions','active_work_contexts','iam_access_subjects','iam_access_invitations','um_attributes','um_values','um_groups','um_memberships','um_policies','um_attachments','um_candidates','iam_directory_source_selections','iam_directory_observation_sources','iam_permissions']::text[])), rels AS (SELECT name,to_regclass('public.'||name) oid FROM names) SELECT count(*)=21 AND bool_and(c.oid IS NOT NULL AND c.relkind IN ('r','p') AND c.relpersistence='p' AND c.relnamespace='public'::regnamespace AND to_regclass(r.name)=c.oid AND NOT row_security_active(c.oid)),array_agg(c.oid::bigint ORDER BY r.name) FROM rels r LEFT JOIN pg_class c ON c.oid=r.oid").fetch_one(&mut *c).await?;
    if !valid {
        return Err(PersistenceError::new("STOCK_COUNT_FLOW_IDENTITY_SOURCE_INVALID").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),$1::bigint[]").bind(oids).fetch_one(c).await?)
}
