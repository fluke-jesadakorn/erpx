//! Identity half of the retained Sales recipient predicate. People employment
//! facts and the actual owner source are independently retained by composition.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

pub struct SalesRecipientRequest {
    pub company_id: CompanyId,
    pub user_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
}

pub struct CurrentSalesRecipient {
    request: SalesRecipientRequest,
    eligible: bool,
    unit: Unit,
}

/// Opaque proof that this transaction acquired the source locks before this
/// port's read. Composition must call the lock function before any snapshot.
pub struct SalesRecipientSourceLocks {
    unit: Unit,
}

/// Holds the original connection while composition prepares its other sources.
/// Caller must enter before the first transaction snapshot and perform only
/// source-locking utility operations until `bind`.
pub struct SalesRecipientLockPreparation<'a> {
    connection: &'a mut PgConnection,
}
impl SalesRecipientLockPreparation<'_> {
    pub fn connection(&mut self) -> &mut PgConnection {
        self.connection
    }
    pub async fn bind(self) -> Result<SalesRecipientSourceLocks, Error> {
        Ok(SalesRecipientSourceLocks {
            unit: unit(self.connection).await?,
        })
    }
}

impl CurrentSalesRecipient {
    pub fn is_eligible(&self) -> bool {
        self.eligible
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || read(c, &self.request).await? != self.eligible {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IAM_SALES_RECIPIENT_CHANGED",
                "The current Sales recipient assignment changed.",
            )
            .into());
        }
        Ok(())
    }
}

/// Call before the first snapshot in the owning transaction.
///
/// This locks every Identity, organization, People and policy source used by the
/// recipient predicate. Composition may acquire additional owner locks through
/// the returned preparation's `connection()` before `bind()`; `bind()` must
/// occur before any snapshot or read.
pub async fn lock_sales_recipient_sources(
    c: &mut PgConnection,
) -> Result<SalesRecipientLockPreparation<'_>, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("IAM_SALES_RECIPIENT_TRANSACTION_REQUIRED").into());
    }
    super::postgres_effective_authority::lock_effective_authority_sources(c).await?;
    Ok(SalesRecipientLockPreparation { connection: c })
}

pub async fn inspect_sales_recipient(
    c: &mut PgConnection,
    locks: &SalesRecipientSourceLocks,
    request: SalesRecipientRequest,
) -> Result<CurrentSalesRecipient, Error> {
    let unit = unit(c).await?;
    if unit != locks.unit {
        return Err(PersistenceError::new("IAM_SALES_RECIPIENT_LOCKS_REQUIRED").into());
    }
    let eligible = read(c, &request).await?;
    Ok(CurrentSalesRecipient {
        request,
        eligible,
        unit,
    })
}

/// Fixed Identity-owned assignment projection for a composed Sales recipient
/// predicate. This attests source identity only, never rows or authorization:
/// the complete joined positive/negative decision must be rerun after effects.
pub struct CurrentSalesAssignmentJoin {
    unit: Unit,
    boundary: (String, String, String, i64),
}
impl CurrentSalesAssignmentJoin {
    pub fn cte_sql(&self) -> &'static str {
        "sales_recipient_assignments AS (SELECT id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id FROM public.assignments)"
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || assignment_join_boundary(c).await? != self.boundary {
            return Err(PersistenceError::new("IAM_SALES_ASSIGNMENT_SOURCE_CHANGED").into());
        }
        Ok(())
    }
}
/// All owner locks must have been acquired before the first snapshot; this
/// checks current lock possession and unit, not lock timing or row contents.
pub async fn inspect_sales_assignment_join(c: &mut PgConnection, locks: &SalesRecipientSourceLocks) -> Result<CurrentSalesAssignmentJoin, Error> {
    let unit = unit(c).await?;
    if unit != locks.unit { return Err(PersistenceError::new("IAM_SALES_RECIPIENT_LOCKS_REQUIRED").into()); }
    let boundary = assignment_join_boundary(c).await?;
    Ok(CurrentSalesAssignmentJoin { unit, boundary })
}
async fn assignment_join_boundary(c: &mut PgConnection) -> Result<(String, String, String, i64), Error> {
    let (held, visible, role, session, path, oid): (bool, bool, String, String, String, i64) = sqlx::query_as("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active(c.oid),current_user::text,session_user::text,current_setting('search_path'),c.oid::bigint FROM pg_class c WHERE c.oid='public.assignments'::regclass").fetch_one(c).await?;
    if !held || !visible { return Err(PersistenceError::new("IAM_SALES_ASSIGNMENT_SOURCE_REQUIRED").into()); }
    Ok((role,session,path,oid))
}

async fn read(c: &mut PgConnection, r: &SalesRecipientRequest) -> Result<bool, Error> {
    // Both permissions must belong to the same active scoped assignment, as
    // in Sales' existing recipient predicate. Use the actual database clock.
    Ok(sqlx::query_scalar::<_, bool>("SELECT true FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$3) OR (a.scope_kind='department' AND a.scope_node_id=$4)) ORDER BY a.id LIMIT 1 FOR SHARE OF a")
        .bind(r.company_id.as_str()).bind(&r.user_id).bind(&r.branch_id).bind(r.department_id.as_deref())
        .fetch_optional(c).await?.unwrap_or(false))
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("IAM_SALES_RECIPIENT_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

/// Identity-owned canonical boundary for the IAM portion of the composed
/// Sales recipient predicate. This is not actor authority, a row-content proof,
/// a pre-snapshot timing proof, or a fence against concurrent function DDL.
/// Organization, People and management-owned authority remain separate ports.
pub struct CurrentSalesIamBoundary {
    unit: Unit,
    boundary: (String, String, Vec<i64>, String),
}
impl CurrentSalesIamBoundary {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || sales_iam_boundary(c).await? != self.boundary {
            return Err(PersistenceError::new("IAM_SALES_AUTHORITY_SOURCE_CHANGED").into());
        }
        Ok(())
    }
}

/// The coordinator must set the canonical search path before taking source
/// locks and acquire all owner locks before the first snapshot. This operation
/// verifies present locks, current role/RLS and function resolution only.
pub async fn inspect_sales_iam_boundary(c: &mut PgConnection, locks: &SalesRecipientSourceLocks) -> Result<CurrentSalesIamBoundary, Error> {
    let unit = unit(c).await?;
    if unit != locks.unit { return Err(PersistenceError::new("IAM_SALES_RECIPIENT_LOCKS_REQUIRED").into()); }
    let boundary = sales_iam_boundary(c).await?;
    Ok(CurrentSalesIamBoundary { unit, boundary })
}

async fn sales_iam_boundary(c: &mut PgConnection) -> Result<(String, String, Vec<i64>, String), Error> {
    // Function signatures are finite, owner-controlled identifiers. Function
    // body fingerprints detect later drift, but do not establish migration
    // provenance or prevent concurrent DDL; those remain coordinator duties.
    let (valid, role, session, oids, functions): (bool, String, String, Vec<i64>, String) = sqlx::query_as(r#"
      WITH names(name) AS (SELECT unnest(ARRAY[
        'assignments','iam_roles','iam_role_revisions','iam_members','iam_identities','iam_connections',
        'iam_revoked_sessions','iam_emergency_sessions','active_work_contexts','iam_access_subjects','iam_access_invitations',
        'um_attributes','um_values','um_groups','um_memberships','um_policies','um_attachments','um_candidates',
        'iam_directory_source_selections','iam_directory_observation_sources','iam_permissions']::text[])),
      rels AS (SELECT n.name,to_regclass('public.'||n.name) AS oid FROM names n),
      sigs(sig) AS (SELECT unnest(ARRAY[
        'iam_assignment_active(text,bigint)','iam_principal_active(text,text,bigint)',
        'iam_principal_active_before_external(text,text,bigint)','iam_directory_observation_current(text,text,bigint)',
        'iam_role_allows(text,text,text)','um_candidate_eligible(text,bigint)',
        'um_attachment_matches(text,text,text)','um_group_member(text,text,text)',
        'um_person_in_unit(text,text,text)','um_descendant(text,text,text)',
        'um_person_facts(text,text)','um_rule_matches(jsonb,jsonb)']::text[])),
      funcs AS (SELECT s.sig,to_regprocedure('public.'||s.sig) AS oid,to_regprocedure(s.sig) AS resolved FROM sigs s)
      SELECT current_setting('search_path')='pg_catalog, public, pg_temp'
       AND (SELECT count(*)=21 AND bool_and(c.oid IS NOT NULL AND c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM rels r LEFT JOIN pg_class c ON c.oid=r.oid)
       AND (SELECT count(*)=12 AND bool_and(p.oid IS NOT NULL AND f.oid=f.resolved AND p.pronamespace='public'::regnamespace AND NOT p.prosecdef AND coalesce(array_length(p.proconfig,1),0)=0 AND has_function_privilege(p.oid,'EXECUTE')) FROM funcs f LEFT JOIN pg_proc p ON p.oid=f.oid),
       current_user::text,session_user::text,
       (SELECT array_agg(r.oid::oid::bigint ORDER BY r.name) FROM rels r),
       (SELECT coalesce(jsonb_agg(jsonb_build_array(f.sig,p.oid::bigint,p.proowner::bigint,p.provolatile,p.prosecdef,p.proconfig,md5(pg_get_functiondef(p.oid))) ORDER BY f.sig),'[]'::jsonb)::text FROM funcs f LEFT JOIN pg_proc p ON p.oid=f.oid)
    "#).fetch_one(c).await?;
    if !valid { return Err(PersistenceError::new("IAM_SALES_AUTHORITY_SOURCE_REQUIRED").into()); }
    Ok((role, session, oids, functions))
}
