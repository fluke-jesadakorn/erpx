//! Scalable native owner-write executor for identity_access.
//!
//! This is the single authoritative kind registry for the remaining TypeScript
//! write literals in this owner. A `kind` is an allow-listed discriminator; the
//! SQL and its positional argument order live here, in the owner crate. The
//! caller supplies only a full-parity `values` array, never SQL text. This
//! module executes the statement on the caller-owned serializable transaction
//! and commits a receipt, audit and outbox event in the same unit (CQRS command
//! side). There is no TypeScript SQL copy and no fallback writer.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, query::Query};

type Error = PgTransactionError<BusinessError>;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "IDENTITY_OWNER_WRITE_INVALID",
        "Review the identity write request.",
    )
}

// ---------------------------------------------------------------------------
// Account / invitation family
// ---------------------------------------------------------------------------
pub const INVITATION_SUPERSEDE: &str = "invitation_supersede";
pub const INVITATION_DELIVERY_CANCEL_SUPERSEDED: &str = "invitation_delivery_cancel_superseded";
pub const INVITATION_INSERT: &str = "invitation_insert";
pub const INVITATION_DELIVERY_INSERT: &str = "invitation_delivery_insert";
pub const INVITATION_DELIVERY_REQUEUE: &str = "invitation_delivery_requeue";
pub const INVITATION_REVOKE: &str = "invitation_revoke";
pub const INVITATION_DELIVERY_CANCEL: &str = "invitation_delivery_cancel";
pub const INVITATION_DELIVERY_PROMOTE: &str = "invitation_delivery_promote";
pub const INVITATION_DELIVERY_REJECT: &str = "invitation_delivery_reject";
pub const INVITATION_DELIVERY_RECOVER: &str = "invitation_delivery_recover";
pub const INVITATION_DELIVERY_CLAIM: &str = "invitation_delivery_claim";
pub const INVITATION_DELIVERY_CANCEL_UNAVAILABLE: &str = "invitation_delivery_cancel_unavailable";
pub const INVITATION_DELIVERY_SENT: &str = "invitation_delivery_sent";
pub const INVITATION_DELIVERY_FAILED: &str = "invitation_delivery_failed";
pub const ACCOUNT_PREFERENCE_INSERT: &str = "account_preference_insert";
pub const ACCOUNT_PREFERENCE_UPDATE: &str = "account_preference_update";
pub const ACCOUNT_USER_NAME_UPDATE: &str = "account_user_name_update";
pub const TIME_PREFERENCE_INSERT: &str = "time_preference_insert";
pub const TIME_PREFERENCE_UPDATE: &str = "time_preference_update";
pub const IAM_THROTTLE_INCREMENT: &str = "iam_throttle_increment";

// ---------------------------------------------------------------------------
// IAM family (grants, roles, sessions, audit, migrations)
// ---------------------------------------------------------------------------
pub const IAM_GRANT_INSERT: &str = "iam_grant_insert";
pub const IAM_REVIEW_INSERT: &str = "iam_review_insert";
pub const IAM_RECEIPT_INSERT: &str = "iam_receipt_insert";
pub const IAM_AUDIT_INSERT: &str = "iam_audit_insert";
pub const IAM_ASSIGNMENT_REVOKE: &str = "iam_assignment_revoke";
pub const IAM_MEMBER_UPDATE: &str = "iam_member_update";
pub const IAM_ROLE_UPDATE: &str = "iam_role_update";
pub const IAM_SESSION_DELETE: &str = "iam_session_delete";
pub const IAM_REVOKED_SESSION_INSERT: &str = "iam_revoked_session_insert";
pub const IAM_MIGRATION_MARK: &str = "iam_migration_mark";
pub const IAM_MIGRATION_RECORD: &str = "iam_migration_record";

// ---------------------------------------------------------------------------
// Technology / directory / provider / beta / people facts family
// ---------------------------------------------------------------------------
pub const DIRECTORY_OPERATION_INSERT: &str = "directory_operation_insert";
pub const DIRECTORY_MEMBERSHIP_REVOKE: &str = "directory_membership_revoke";
pub const DIRECTORY_MEMBERSHIP_DELETE: &str = "directory_membership_delete";
pub const DIRECTORY_MEMBERSHIP_TRACK: &str = "directory_membership_track";
pub const SSO_BINDING_INSERT: &str = "sso_binding_insert";
pub const SSO_TRANSACTION_INSERT: &str = "sso_transaction_insert";
pub const SSO_GRANT_INSERT: &str = "sso_grant_insert";
pub const SSO_SESSION_INSERT: &str = "sso_session_insert";
pub const SSO_TRANSACTION_CONSUME: &str = "sso_transaction_consume";
pub const SSO_GRANT_CONSUME: &str = "sso_grant_consume";
pub const PROVIDER_CONTEXT_INSERT: &str = "provider_context_insert";
pub const PROVIDER_CONTEXT_UPDATE: &str = "provider_context_update";
pub const WORKSPACE_PREFERENCE_UPSERT: &str = "workspace_preference_upsert";
pub const BETA_INVITATION_INSERT: &str = "beta_invitation_insert";
pub const BETA_STATE_UPDATE: &str = "beta_state_update";
pub const BETA_ONBOARDING_EFFECT: &str = "beta_onboarding_effect";
pub const PEOPLE_LIFECYCLE_EFFECT: &str = "people_lifecycle_effect";
pub const PEOPLE_TEMPLATE_ATTRIBUTE_UPDATE: &str = "people_template_attribute_update";
pub const PEOPLE_DIRECTORY_FACT_UPDATE: &str = "people_directory_fact_update";
pub const PAYROLL_OWNER_FACT_UPDATE: &str = "payroll_owner_fact_update";
pub const DEPARTMENT_RELOCATION_UPDATE: &str = "department_relocation_update";

/// Every write kind this owner module accepts. Anything else is fail-closed.
pub const KINDS: &[&str] = &[
    INVITATION_SUPERSEDE,
    INVITATION_DELIVERY_CANCEL_SUPERSEDED,
    INVITATION_INSERT,
    INVITATION_DELIVERY_INSERT,
    INVITATION_DELIVERY_REQUEUE,
    INVITATION_REVOKE,
    INVITATION_DELIVERY_CANCEL,
    INVITATION_DELIVERY_PROMOTE,
    INVITATION_DELIVERY_REJECT,
    INVITATION_DELIVERY_RECOVER,
    INVITATION_DELIVERY_CLAIM,
    INVITATION_DELIVERY_CANCEL_UNAVAILABLE,
    INVITATION_DELIVERY_SENT,
    INVITATION_DELIVERY_FAILED,
    ACCOUNT_PREFERENCE_INSERT,
    ACCOUNT_PREFERENCE_UPDATE,
    ACCOUNT_USER_NAME_UPDATE,
    TIME_PREFERENCE_INSERT,
    TIME_PREFERENCE_UPDATE,
    IAM_THROTTLE_INCREMENT,
    IAM_GRANT_INSERT,
    IAM_REVIEW_INSERT,
    IAM_RECEIPT_INSERT,
    IAM_AUDIT_INSERT,
    IAM_ASSIGNMENT_REVOKE,
    IAM_MEMBER_UPDATE,
    IAM_ROLE_UPDATE,
    IAM_SESSION_DELETE,
    IAM_REVOKED_SESSION_INSERT,
    IAM_MIGRATION_MARK,
    IAM_MIGRATION_RECORD,
    DIRECTORY_OPERATION_INSERT,
    DIRECTORY_MEMBERSHIP_REVOKE,
    DIRECTORY_MEMBERSHIP_DELETE,
    DIRECTORY_MEMBERSHIP_TRACK,
    SSO_BINDING_INSERT,
    SSO_TRANSACTION_INSERT,
    SSO_GRANT_INSERT,
    SSO_SESSION_INSERT,
    SSO_TRANSACTION_CONSUME,
    SSO_GRANT_CONSUME,
    PROVIDER_CONTEXT_INSERT,
    PROVIDER_CONTEXT_UPDATE,
    WORKSPACE_PREFERENCE_UPSERT,
    BETA_INVITATION_INSERT,
    BETA_STATE_UPDATE,
    BETA_ONBOARDING_EFFECT,
    PEOPLE_LIFECYCLE_EFFECT,
    PEOPLE_TEMPLATE_ATTRIBUTE_UPDATE,
    PEOPLE_DIRECTORY_FACT_UPDATE,
    PAYROLL_OWNER_FACT_UPDATE,
    DEPARTMENT_RELOCATION_UPDATE,
];

/// Whether a write kind belongs to this owner module.
pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind) || super::identity_write_generated::generated_supported(kind)
}

/// Authoritative SQL for one owner write kind. `?` placeholders are replaced
/// with positional `$n` parameters at execution; only values come from callers.
/// A kind is only ever mapped to a literal SQL statement held in this crate.
fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        INVITATION_SUPERSEDE => {
            "UPDATE account_invitations SET status='revoked' WHERE holding_id=? AND person_id=? AND status='pending' AND id!=?"
        }
        INVITATION_DELIVERY_CANCEL_SUPERSEDED => {
            "UPDATE account_invitation_delivery_jobs d SET state='cancelled',error='INVITATION_SUPERSEDED',lease_token=NULL,lease_until=NULL,updated_at=? FROM account_invitations i WHERE d.invitation_id=i.id AND i.holding_id=? AND i.person_id=? AND i.status='revoked' AND i.id!=? AND d.state!='sent'"
        }
        INVITATION_INSERT => {
            "INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,branch_id,created_by,assignment_id,expires_at,created_at,department_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING"
        }
        INVITATION_DELIVERY_INSERT => {
            "INSERT INTO account_invitation_delivery_jobs(id,invitation_id,holding_id,company_id,state,next_attempt_at,created_at,updated_at) VALUES(?,?,?,?,'awaiting_approval',?,?,?) ON CONFLICT(invitation_id) DO NOTHING"
        }
        INVITATION_DELIVERY_REQUEUE => {
            "UPDATE account_invitation_delivery_jobs SET state='queued',attempts=0,next_attempt_at=?,error=NULL,lease_token=NULL,lease_until=NULL,updated_at=? WHERE invitation_id=? AND holding_id=? AND company_id=? AND state='failed' AND EXISTS(SELECT 1 FROM account_invitations WHERE id=? AND person_id=? AND status='pending' AND expires_at>?) RETURNING id"
        }
        INVITATION_REVOKE => {
            "UPDATE account_invitations SET status='revoked' WHERE id=? AND holding_id=? AND company_id=? AND person_id=? AND status='pending' RETURNING id"
        }
        INVITATION_DELIVERY_CANCEL => {
            "UPDATE account_invitation_delivery_jobs SET state='cancelled',error='INVITATION_REVOKED',lease_token=NULL,lease_until=NULL,updated_at=? WHERE invitation_id=? AND state!='sent'"
        }
        INVITATION_DELIVERY_PROMOTE => {
            "UPDATE account_invitation_delivery_jobs SET state='queued',next_attempt_at=?,updated_at=? WHERE id=? AND state='awaiting_approval'"
        }
        INVITATION_DELIVERY_REJECT => {
            "UPDATE account_invitation_delivery_jobs SET state='cancelled',error='INVITATION_REJECTED',updated_at=? WHERE id=? AND state='awaiting_approval'"
        }
        INVITATION_DELIVERY_RECOVER => {
            "UPDATE account_invitation_delivery_jobs SET state=CASE WHEN attempts<6 THEN 'queued' ELSE 'failed' END,lease_token=NULL,lease_until=NULL,error='DELIVERY_INTERRUPTED',next_attempt_at=?,updated_at=? WHERE state='running' AND lease_until<?"
        }
        INVITATION_DELIVERY_CLAIM => {
            "UPDATE account_invitation_delivery_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=?,updated_at=? WHERE id=(SELECT id FROM account_invitation_delivery_jobs WHERE state='queued' AND next_attempt_at<=? ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING id,invitation_id,attempts,lease_token"
        }
        INVITATION_DELIVERY_CANCEL_UNAVAILABLE => {
            "UPDATE account_invitation_delivery_jobs SET state='cancelled',lease_token=NULL,lease_until=NULL,error='INVITATION_UNAVAILABLE',updated_at=? WHERE id=? AND lease_token=?"
        }
        INVITATION_DELIVERY_SENT => {
            "UPDATE account_invitation_delivery_jobs SET state='sent',sent_at=?,provider_id=?,lease_token=NULL,lease_until=NULL,error=NULL,updated_at=? WHERE id=? AND lease_token=?"
        }
        INVITATION_DELIVERY_FAILED => {
            "UPDATE account_invitation_delivery_jobs SET state=?,error=?,next_attempt_at=?,lease_token=NULL,lease_until=NULL,updated_at=? WHERE id=? AND lease_token=?"
        }
        ACCOUNT_PREFERENCE_INSERT => {
            "INSERT INTO account_preferences(user_id,language,appearance,version,updated_at) VALUES(?,?,?,1,?) ON CONFLICT(user_id) DO NOTHING RETURNING version"
        }
        ACCOUNT_PREFERENCE_UPDATE => {
            "UPDATE account_preferences SET language=?,appearance=?,version=version+1,updated_at=? WHERE user_id=? AND version=? RETURNING version"
        }
        ACCOUNT_USER_NAME_UPDATE => "UPDATE \"user\" SET name=?,\"updatedAt\"=? WHERE id=?",
        TIME_PREFERENCE_INSERT => {
            "INSERT INTO account_time_preferences(user_id,mode,time_zone,hour_cycle,version,updated_at) VALUES(?,?,?,?,1,?) ON CONFLICT(user_id) DO NOTHING RETURNING mode,time_zone,hour_cycle,version"
        }
        TIME_PREFERENCE_UPDATE => {
            "UPDATE account_time_preferences SET mode=?,time_zone=?,hour_cycle=?,version=version+1,updated_at=? WHERE user_id=? AND version=? RETURNING mode,time_zone,hour_cycle,version"
        }
        IAM_THROTTLE_INCREMENT => {
            "INSERT INTO iam_throttles(key,window_start,attempts) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET window_start=CASE WHEN iam_throttles.window_start<? THEN excluded.window_start ELSE iam_throttles.window_start END,attempts=CASE WHEN iam_throttles.window_start<? THEN 1 ELSE iam_throttles.attempts+1 END RETURNING attempts"
        }
        IAM_GRANT_INSERT => {
            "INSERT INTO iam_grants(id,company_id,user_id,role_id,scope_kind,branch_id,department_id,source,source_id,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,1,?,?,?) ON CONFLICT DO NOTHING"
        }
        IAM_REVIEW_INSERT => {
            "INSERT INTO iam_reviews(id,company_id,kind,payload,state,created_by,created_at,updated_at) VALUES(?,?,?,?::jsonb,'pending',?,?,?) ON CONFLICT DO NOTHING"
        }
        IAM_RECEIPT_INSERT => {
            "INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING"
        }
        IAM_AUDIT_INSERT => {
            "INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING"
        }
        IAM_ASSIGNMENT_REVOKE => {
            "UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=? AND company_id=?"
        }
        IAM_MEMBER_UPDATE => {
            "UPDATE iam_members SET status=?,version=version+1,updated_at=? WHERE company_id=? AND user_id=?"
        }
        IAM_ROLE_UPDATE => {
            "UPDATE iam_roles SET published_revision=?,updated_at=? WHERE id=? AND company_id IN ('*',?)"
        }
        IAM_SESSION_DELETE => "DELETE FROM session WHERE id=?",
        IAM_REVOKED_SESSION_INSERT => {
            "INSERT INTO iam_revoked_sessions(id,company_id,user_id,session_id,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING"
        }
        IAM_MIGRATION_MARK => "UPDATE iam_migrations SET state=?,updated_at=? WHERE id=?",
        IAM_MIGRATION_RECORD => {
            "INSERT INTO iam_migrations(id,company_id,name,state,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING"
        }
        DIRECTORY_OPERATION_INSERT => {
            "INSERT INTO people_directory_operations(id,company_id,connection_id,connection_version,state_version,status,definition,evidence,created_by,assignment_id,created_at,updated_at,approval_mode,mfa_session_id,provision_id) VALUES(?,?,?,?,?,'queued',?,?,?,?,?,?,'direct',?,?)"
        }
        DIRECTORY_MEMBERSHIP_REVOKE => {
            "UPDATE people_directory_membership_sources SET revoked_at=? WHERE application_id=?"
        }
        DIRECTORY_MEMBERSHIP_DELETE => {
            "DELETE FROM people_directory_membership_sources WHERE application_id=ANY(?::text[])"
        }
        DIRECTORY_MEMBERSHIP_TRACK => {
            "INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,application_id,created_at) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING"
        }
        SSO_BINDING_INSERT => {
            "INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES(?,?,?,?,?,?)"
        }
        SSO_TRANSACTION_INSERT => {
            "INSERT INTO sso_browser_transactions(id,browser_hash,state_hash,challenge,audience,environment,callback,return_path,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?,?,?)"
        }
        SSO_GRANT_INSERT => {
            "INSERT INTO sso_authorization_grants(code_hash,transaction_id,parent_session_id,audience,environment,callback,state_hash,challenge,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?,?,?)"
        }
        SSO_SESSION_INSERT => {
            "INSERT INTO session(id,token,\"userId\",\"createdAt\",\"updatedAt\",\"expiresAt\") VALUES(?,?,?,?,?,?)"
        }
        SSO_TRANSACTION_CONSUME => "UPDATE sso_browser_transactions SET consumed_at=? WHERE id=?",
        SSO_GRANT_CONSUME => "UPDATE sso_authorization_grants SET consumed_at=? WHERE code_hash=?",
        PROVIDER_CONTEXT_INSERT => {
            "INSERT INTO admin_provider_contexts(session_id,user_id,assignment_id,revision,updated_at) VALUES(?,?,?,1,?) ON CONFLICT(session_id) DO NOTHING RETURNING revision"
        }
        PROVIDER_CONTEXT_UPDATE => {
            "UPDATE admin_provider_contexts SET assignment_id=?,revision=revision+1,updated_at=? WHERE session_id=? AND user_id=? AND revision=? RETURNING revision"
        }
        WORKSPACE_PREFERENCE_UPSERT => {
            "INSERT INTO account_workspace_preferences(user_id,platform_assignment_id,updated_at) VALUES(?,?,?) ON CONFLICT(user_id) DO UPDATE SET platform_assignment_id=excluded.platform_assignment_id,updated_at=excluded.updated_at"
        }
        BETA_INVITATION_INSERT => {
            "INSERT INTO beta_invitations(id,email,holding_slug,state,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING"
        }
        BETA_STATE_UPDATE => "UPDATE beta_requests SET state=?,updated_at=? WHERE id=?",
        BETA_ONBOARDING_EFFECT => {
            "INSERT INTO beta_onboarding_effects(id,request_id,kind,payload,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING"
        }
        PEOPLE_LIFECYCLE_EFFECT => {
            "INSERT INTO people_lifecycle_effects(id,person_id,kind,payload,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING"
        }
        PEOPLE_TEMPLATE_ATTRIBUTE_UPDATE => {
            "UPDATE people_template_attributes SET value=?,version=version+1,updated_at=? WHERE id=? AND version=?"
        }
        PEOPLE_DIRECTORY_FACT_UPDATE => {
            "UPDATE people_directory_facts SET value=?,updated_at=? WHERE id=?"
        }
        PAYROLL_OWNER_FACT_UPDATE => {
            "UPDATE payroll_owner_facts SET value=?,updated_at=? WHERE id=?"
        }
        DEPARTMENT_RELOCATION_UPDATE => {
            "UPDATE organization_departments SET parent_id=?,version=version+1,updated_at=? WHERE id=? AND version=?"
        }
        _ => return super::identity_write_generated::generated_sql(kind),
    })
}

/// Replace `?` markers with positional `$n` parameters.
fn positional(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 16);
    let mut index = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            index += 1;
            out.push('$');
            out.push_str(&index.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

fn bind_value<'q>(
    mut query: Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: &'q Value,
) -> Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match value {
        Value::String(text) => query = query.bind(text.as_str()),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                query = query.bind(integer);
            } else {
                query = query.bind(number.as_f64().unwrap_or_default());
            }
        }
        Value::Bool(flag) => query = query.bind(*flag),
        Value::Null => query = query.bind(Option::<&str>::None),
        object => query = query.bind(object.clone()),
    }
    query
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("IDENTITY_OWNER_WRITE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("IDENTITY_OWNER_WRITE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// What the caller needs to know after a committed owner write.
#[derive(Clone, Debug, serde::Serialize)]
pub struct OwnerWriteOutcome {
    pub kind: String,
    pub rows: u64,
}

/// Execute one owner write kind on the caller-owned serializable unit and
/// commit its receipt/audit/outbox ledger rows in the same transaction.
pub async fn execute(
    connection: &mut PgConnection,
    scope: &str,
    actor: &str,
    kind: &str,
    values: &[Value],
    now: i64,
    key: &str,
) -> Result<OwnerWriteOutcome, Error> {
    ready(connection).await?;
    if !supported(kind) || values.is_empty() || values.len() > 64 {
        return Err(invalid().into());
    }
    let statement = sql(kind).ok_or_else(invalid)?;
    // Legacy statements use `?` markers; PostgreSQL-native kinds already carry
    // `$n`. Both are owner-owned literals; only values arrive from the caller.
    let dollar = statement.contains("$1");
    let expected = if dollar {
        statement
            .match_indices('$')
            .filter_map(|(index, _)| {
                statement[index + 1..]
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit())
                    .collect::<String>()
                    .parse::<usize>()
                    .ok()
            })
            .max()
            .unwrap_or(0)
    } else {
        statement.matches('?').count()
    };
    if values.len() != expected {
        return Err(invalid().into());
    }
    let rendered = if dollar {
        statement.to_owned()
    } else {
        positional(statement)
    };
    let mut query = sqlx::query(sqlx::AssertSqlSafe(rendered));
    for value in values {
        query = bind_value(query, value);
    }
    let rows = query
        .execute(&mut *connection)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .and_then(|value| value.code())
                .as_deref()
                == Some("23505")
            {
                BusinessError::new(
                    FailureKind::Conflict,
                    "IDENTITY_OWNER_WRITE_CONFLICT",
                    "The identity record changed. Reload and review again.",
                )
                .into()
            } else {
                Error::from(error)
            }
        })?
        .rows_affected();

    // CQRS command ledger: receipt + audit + outbox in this same unit.
    let result = serde_json::json!({"kind": kind, "rows": rows});
    let changed = sqlx::query(
        "INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("identity-write-{key}"))
    .bind(scope)
    .bind(actor)
    .bind(format!("identity-write:{kind}"))
    .bind(&result)
    .bind(now)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "IDEMPOTENCY_CONFLICT",
            "This command was already used for a different write.",
        )
        .into());
    }
    let audit = serde_json::json!({"kind": kind, "rows": rows});
    sqlx::query(
        "INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("identity-write-audit-{key}"))
    .bind(scope)
    .bind(actor)
    .bind(actor)
    .bind(kind)
    .bind(key)
    .bind(&audit)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("identity-write-outbox-{key}"))
    .bind(scope)
    .bind(actor)
    .bind(actor)
    .bind(format!("identity.write.{kind}"))
    .bind(key)
    .bind(&result)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(OwnerWriteOutcome {
        kind: kind.to_owned(),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_allow_listed_and_parameterized() {
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            let statement = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                statement.contains('?'),
                "write must be parameterized: {kind}"
            );
        }
    }

    #[test]
    fn unknown_kinds_are_rejected() {
        assert!(!supported("not_an_identity_write"));
        assert!(sql("not_an_identity_write").is_none());
    }

    #[test]
    fn placeholders_become_positional_parameters() {
        assert_eq!(
            positional("INSERT INTO t(a,b) VALUES(?,?)"),
            "INSERT INTO t(a,b) VALUES($1,$2)"
        );
        assert_eq!(
            positional("UPDATE t SET a=? WHERE b=? AND c=?"),
            "UPDATE t SET a=$1 WHERE b=$2 AND c=$3"
        );
    }

    #[test]
    fn invitation_insert_has_fifteen_parameters() {
        assert_eq!(sql(INVITATION_INSERT).unwrap().matches('?').count(), 15);
    }
}
