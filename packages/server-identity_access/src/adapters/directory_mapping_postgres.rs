//! Standing reviewed mappings use actual current IAM and owner structural facts.
use super::directory_postgres::{DirectoryRoute, Tx, unavailable};
use crate::domain::{directory::DirectoryConnectionDescriptor, directory_grants::*};
use erpx_server_organization::public::{NodeKind, PgOrganizationReader, lock_hierarchy_sources};
use erpx_server_people_payroll::public::identity_facts::lock_people_authority_sources;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{CompanyId, RecordId};
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, PgConnection, Row};

pub(super) async fn lock_sources(c: &mut PgConnection, platform: bool) -> Tx<()> {
    // Never lock tombstones/context tables: Account retirement must commit while
    // this read proof is retained. SHARE also fences legacy/raw policy writers.
    sqlx::query("LOCK TABLE iam_connections,iam_members,iam_identities,iam_mappings,iam_roles,iam_role_revisions IN SHARE MODE").execute(&mut *c).await?;
    if platform {
        sqlx::query("LOCK TABLE platform_assignments IN SHARE MODE")
            .execute(c)
            .await?;
    } else {
        super::directory_source_postgres::lock_sources(c).await?;
        sqlx::query("LOCK TABLE assignments IN SHARE MODE")
            .execute(&mut *c)
            .await?;
        lock_people_authority_sources(c).await?;
        lock_hierarchy_sources(c).await?;
    }
    Ok(())
}
/// Bounded readset includes exact mapping/parent versions, role publication and
/// current lineage predicate. Time is sampled again at the final boundary.
pub(super) async fn policy_witness(
    c: &mut PgConnection,
    d: &DirectoryConnectionDescriptor,
    now: i64,
) -> Tx<String> {
    let table = if d.company_id == "platform" {
        "platform_assignments"
    } else {
        "assignments"
    };
    let query = format!(
        "SELECT CASE WHEN octet_length(e::text)<=262144 THEN e::text END evidence FROM (SELECT jsonb_build_object('mapping',to_jsonb(m),'parent',to_jsonb(p),'current',iam_assignment_active(m.parent_id,$3),'roles',(SELECT jsonb_agg(jsonb_build_object('role',to_jsonb(r),'revision',to_jsonb(v)) ORDER BY r.company_id,r.id) FROM iam_roles r LEFT JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$2) AND r.id IN (m.role_id,p.role))) e,m.id FROM iam_mappings m LEFT JOIN {table} p ON p.id=m.parent_id WHERE m.connection_id=$1 AND m.company_id=$2 ORDER BY m.id LIMIT 10001) q"
    );
    let mut rows = sqlx::query(AssertSqlSafe(query.as_str()))
        .bind(&d.connection_id)
        .bind(&d.company_id)
        .bind(now)
        .fetch(c);
    let (mut count, mut bytes) = (0usize, 0usize);
    let mut digest = Sha256::new();
    while let Some(row) = rows.try_next().await? {
        count += 1;
        let text: Option<String> = row.try_get("evidence")?;
        let text = text.ok_or_else(unavailable)?;
        bytes = bytes.checked_add(text.len()).ok_or_else(unavailable)?;
        if count > 10000 || bytes > 16 * 1024 * 1024 {
            return Err(unavailable().into());
        }
        digest.update((text.len() as u64).to_be_bytes());
        digest.update(text.as_bytes());
    }
    Ok(hex::encode(digest.finalize()))
}
#[derive(serde::Deserialize)]
struct Candidate {
    id: String,
    user_id: String,
    role_id: String,
    scope_kind: String,
    scope_node_id: Option<String>,
    valid_until: i64,
    parent_id: String,
}
struct Role {
    revision: i64,
    permissions: Vec<String>,
    grantable: Vec<String>,
    scopes: Vec<String>,
}
async fn role(c: &mut PgConnection, company: &str, id: &str) -> Tx<Option<Role>> {
    let rows=sqlx::query("SELECT r.published_revision::bigint revision,CASE WHEN octet_length(v.permissions_json::text)<=65536 THEN v.permissions_json::text END permissions,CASE WHEN octet_length(v.grantable_json::text)<=65536 THEN v.grantable_json::text END grantable,CASE WHEN octet_length(v.scopes_json::text)<=4096 THEN v.scopes_json::text END scopes FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2) AND r.status='active' LIMIT 2 FOR SHARE OF r,v").bind(id).bind(company).fetch_all(c).await?;
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() != 1 {
        return Err(unavailable().into());
    }
    let row = &rows[0];
    let list = |field| -> Tx<Vec<String>> {
        let raw: Option<String> = row.try_get(field)?;
        let values: Vec<String> =
            serde_json::from_str(&raw.ok_or_else(unavailable)?).map_err(|_| unavailable())?;
        if values.len() > 4096 || values.iter().any(|v| v.len() > 500) {
            return Err(unavailable().into());
        }
        Ok(values)
    };
    Ok(Some(Role {
        revision: row.try_get("revision")?,
        permissions: list("permissions")?,
        grantable: list("grantable")?,
        scopes: list("scopes")?,
    }))
}

/// Exact intended own rows accompany the upstream source witness. A trigger may
/// not rewrite a granted identity/scope/role, suppress a revocation, or replace
/// the source baseline after the operation has already used it.
pub(super) struct DirectoryReconciliation {
    platform: bool,
    source: String,
    expected: std::collections::BTreeMap<String, serde_json::Value>,
    bytes: usize,
}
impl DirectoryReconciliation {
    fn remember(&mut self, value: serde_json::Value) -> Tx<()> {
        let id = value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| id.len() <= 200)
            .ok_or_else(unavailable)?
            .to_owned();
        let bytes = serde_json::to_vec(&value).map_err(|_| unavailable())?.len();
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(unavailable)?;
        if bytes > 65536
            || self.bytes > 32 * 1024 * 1024
            || self.expected.len() >= 20000
            || self.expected.insert(id, value).is_some()
        {
            return Err(unavailable().into());
        }
        Ok(())
    }
    async fn require_effects(&self, c: &mut PgConnection) -> Tx<()> {
        let table = if self.platform {
            "platform_assignments"
        } else {
            "assignments"
        };
        // Only the static owner table name is interpolated; IDs remain bound.
        let query = format!(
            "SELECT CASE WHEN octet_length(to_jsonb(a)::text)<=65536 THEN to_jsonb(a)::text END evidence FROM {table} a WHERE a.id=ANY($1::text[])"
        );
        let ids: Vec<&str> = self.expected.keys().map(String::as_str).collect();
        let mut rows = sqlx::query(AssertSqlSafe(query.as_str()))
            .bind(&ids)
            .fetch(c);
        let (mut count, mut bytes) = (0usize, 0usize);
        while let Some(row) = rows.try_next().await? {
            let raw = row
                .try_get::<Option<String>, _>("evidence")?
                .ok_or_else(unavailable)?;
            bytes = bytes.checked_add(raw.len()).ok_or_else(unavailable)?;
            if bytes > 32 * 1024 * 1024 {
                return Err(unavailable().into());
            }
            let actual: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| unavailable())?;
            let id = actual
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(unavailable)?;
            if self.expected.get(id) != Some(&actual) {
                return Err(unavailable().into());
            }
            count += 1;
        }
        if count != self.expected.len() {
            return Err(unavailable().into());
        }
        Ok(())
    }
    pub(super) async fn require_current(
        &self,
        c: &mut PgConnection,
        d: &DirectoryConnectionDescriptor,
        now: i64,
    ) -> Tx<()> {
        if policy_witness(c, d, now).await? != self.source {
            return Err(unavailable().into());
        }
        self.require_effects(c).await
    }
}
pub(super) async fn reconcile(
    c: &mut PgConnection,
    d: &DirectoryConnectionDescriptor,
    route: &DirectoryRoute,
    now: i64,
    ids: &[String],
) -> Tx<DirectoryReconciliation> {
    let table = if d.company_id == "platform" {
        "platform_assignments"
    } else {
        "assignments"
    };
    let predicate = "a.source='directory' AND a.revoked_at IS NULL AND a.source_id IN (SELECT id FROM iam_mappings WHERE connection_id=$2 AND company_id=$3) AND NOT EXISTS(SELECT 1 FROM iam_mappings m JOIN iam_identities i ON i.connection_id=m.connection_id AND i.user_id=a.user_id AND i.company_id=m.company_id WHERE m.id=a.source_id AND m.status='active' AND m.valid_until>$1 AND i.enabled AND i.groups_json @> jsonb_build_array(m.group_id) AND iam_assignment_active(m.parent_id,$1))";
    let select = format!(
        "SELECT CASE WHEN octet_length(to_jsonb(a)::text)<=65536 THEN to_jsonb(a)::text END evidence FROM {table} a WHERE {predicate} ORDER BY a.id LIMIT 10001 FOR UPDATE"
    );
    let mut rows = sqlx::query(AssertSqlSafe(select.as_str()))
        .bind(now)
        .bind(&d.connection_id)
        .bind(&d.company_id)
        .fetch(&mut *c);
    let mut retained = DirectoryReconciliation {
        platform: d.company_id == "platform",
        source: String::new(),
        expected: Default::default(),
        bytes: 0,
    };
    while let Some(row) = rows.try_next().await? {
        if retained.expected.len() >= 10000 {
            return Err(unavailable().into());
        }
        let raw = row
            .try_get::<Option<String>, _>("evidence")?
            .ok_or_else(unavailable)?;
        let mut value: serde_json::Value = serde_json::from_str(&raw).map_err(|_| unavailable())?;
        if d.company_id != "platform"
            && value.get("company_id").and_then(serde_json::Value::as_str)
                != Some(d.company_id.as_str())
        {
            return Err(unavailable().into());
        }
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_i64)
            .and_then(|version| version.checked_add(1))
            .ok_or_else(unavailable)?;
        let object = value.as_object_mut().ok_or_else(unavailable)?;
        object.insert("version".into(), version.into());
        object.insert("revoked_at".into(), now.into());
        retained.remember(value)?;
    }
    drop(rows);
    let revoke = format!("UPDATE {table} a SET revoked_at=$1,version=version+1 WHERE {predicate}");
    let changed = sqlx::query(AssertSqlSafe(revoke.as_str()))
        .bind(now)
        .bind(&d.connection_id)
        .bind(&d.company_id)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if changed != retained.expected.len() as u64 {
        return Err(unavailable().into());
    }
    retained.require_effects(c).await?;
    // Expected removals are now fixed. Capture upstream source BEFORE any new
    // grant INSERT can invoke immediate owner/source mutation triggers.
    retained.source = policy_witness(c, d, now).await?;
    let query = format!(
        "SELECT CASE WHEN octet_length(e::text)<=8192 THEN e::text END evidence FROM (SELECT jsonb_build_object('id',m.id,'user_id',i.user_id,'role_id',m.role_id,'scope_kind',m.scope_kind,'scope_node_id',m.scope_node_id,'valid_until',m.valid_until,'parent_id',m.parent_id) e FROM iam_mappings m JOIN iam_identities i ON i.connection_id=m.connection_id AND i.company_id=m.company_id JOIN iam_members u ON u.identity_id=i.id AND u.company_id=m.company_id AND u.user_id=i.user_id AND u.connection_id=m.connection_id WHERE m.connection_id=$1 AND m.company_id=$2 AND m.status='active' AND m.valid_until>$3 AND i.enabled AND i.groups_json @> jsonb_build_array(m.group_id) AND u.status='active' AND u.login_policy='directory' AND iam_assignment_active(m.parent_id,$3) AND NOT EXISTS(SELECT 1 FROM {table} a WHERE a.user_id=i.user_id AND a.source='directory' AND a.source_id=m.id) ORDER BY m.id,i.id LIMIT 10001) q"
    );
    let mut stream = sqlx::query(AssertSqlSafe(query.as_str()))
        .bind(&d.connection_id)
        .bind(&d.company_id)
        .bind(now)
        .fetch(&mut *c);
    let mut candidates = Vec::new();
    let mut bytes = 0usize;
    while let Some(row) = stream.try_next().await? {
        let text: Option<String> = row.try_get("evidence")?;
        let text = text.ok_or_else(unavailable)?;
        bytes += text.len();
        if candidates.len() >= 10000 || bytes > 8 * 1024 * 1024 {
            return Err(unavailable().into());
        }
        candidates.push(serde_json::from_str::<Candidate>(&text).map_err(|_| unavailable())?);
    }
    drop(stream);
    if candidates.len() > ids.len() {
        return Err(unavailable().into());
    }
    for (m, identifier) in candidates.into_iter().zip(ids) {
        let id = format!("iam-{identifier}");
        let platform = d.company_id == "platform";
        let parent_query = if platform {
            "SELECT CASE WHEN octet_length(user_id)<=200 THEN user_id END user_id,CASE WHEN octet_length(role)<=200 THEN role END role,'platform' scope_kind,false node_present,NULL::text scope_node_id,valid_until,true company_matches,iam_assignment_active(id,$2) current FROM platform_assignments WHERE id=$1 FOR SHARE"
        } else {
            "SELECT CASE WHEN octet_length(user_id)<=200 THEN user_id END user_id,CASE WHEN octet_length(role)<=200 THEN role END role,scope_kind,scope_node_id IS NOT NULL node_present,CASE WHEN octet_length(scope_node_id)<=200 THEN scope_node_id END scope_node_id,valid_until,company_id=$3 company_matches,iam_assignment_active(id,$2) current FROM assignments WHERE id=$1 FOR SHARE"
        };
        let mut parent_query = sqlx::query(parent_query).bind(&m.parent_id).bind(now);
        if !platform {
            parent_query = parent_query.bind(&d.company_id);
        }
        let Some(parent) = parent_query.fetch_optional(&mut *c).await? else {
            continue;
        };
        let parent_user: String = parent
            .try_get::<Option<String>, _>("user_id")?
            .ok_or_else(unavailable)?;
        let parent_role: String = parent
            .try_get::<Option<String>, _>("role")?
            .ok_or_else(unavailable)?;
        if parent_user.len() > 200 || parent_role.len() > 200 {
            return Err(unavailable().into());
        }
        let published = role(c, &d.company_id, &m.role_id).await?;
        let parent_policy = role(c, &d.company_id, &parent_role).await?;
        let same: bool = sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
            .bind(&m.user_id)
            .bind(&parent_user)
            .bind(&d.company_id)
            .fetch_one(&mut *c)
            .await?;
        let parent_scope: String = parent.try_get("scope_kind")?;
        let parent_node: Option<String> = parent.try_get("scope_node_id")?;
        if parent.try_get::<bool, _>("node_present")? && parent_node.is_none() {
            return Err(unavailable().into());
        }
        let scope = if matches!(m.scope_kind.as_str(), "branch" | "department") && !platform {
            if let (Some(holding), Some(node)) = (&route.holding_id, &m.scope_node_id) {
                let mut org = PgOrganizationReader::new(holding.clone(), c);
                match org
                    .scope_node_context(
                        &CompanyId::new(d.company_id.clone())?,
                        if m.scope_kind == "branch" {
                            NodeKind::Branch
                        } else {
                            NodeKind::Department
                        },
                        &RecordId::new(node.clone())?,
                    )
                    .await
                {
                    Ok(scope) => Some(scope),
                    Err(PgTransactionError::Business(error))
                        if error.code == "WORKSPACE_NOT_FOUND" =>
                    {
                        None
                    }
                    Err(error) => return Err(error),
                }
            } else {
                None
            }
        } else {
            None
        };
        let facts = DirectoryGrantFacts {
            same_person: same,
            parent: Some(DirectoryParentFact {
                current: parent.try_get("current")?,
                company_matches: parent.try_get("company_matches")?,
                scope: &parent_scope,
                node: parent_node.as_deref(),
                valid_until: parent.try_get("valid_until")?,
                grantable: parent_policy.as_ref().map(|p| p.grantable.as_slice()),
            }),
            member: Some(DirectoryMemberFact {
                active: true,
                directory: true,
            }),
            role: published.as_ref().map(|r| DirectoryRoleFact {
                revision: r.revision,
                permissions: &r.permissions,
                grantable: &r.grantable,
                scopes: &r.scopes,
            }),
            requested_scope: &m.scope_kind,
            requested_node: m.scope_node_id.as_deref(),
            requested_until: m.valid_until,
            scope: scope.as_ref().map(|s| DirectoryScopeFact {
                branch: s.branch().map(|n| n.id.as_str()),
                department: s.department().map(|n| n.id.as_str()),
            }),
            platform,
        };
        if validate_mapping_grant(&facts, now).is_err() {
            continue;
        }
        let published = published.ok_or_else(unavailable)?;
        let mut modules = Vec::new();
        if m.role_id == "holding_owner" {
            for p in super::permission_catalog::permission_catalog()?.definitions() {
                if p.id.starts_with("module.") && !modules.contains(&p.module) {
                    modules.push(p.module.clone());
                }
            }
        } else {
            for p in &published.permissions {
                if p.starts_with("module.") {
                    let value = p.split('.').nth(1).ok_or_else(unavailable)?.to_owned();
                    if !modules.contains(&value) {
                        modules.push(value);
                    }
                }
            }
        }
        let modules = serde_json::to_string(&modules).map_err(|_| unavailable())?;
        let mut expected = serde_json::json!({"id":id,"user_id":m.user_id,"role":m.role_id,"modules_json":modules,"valid_from":now,"valid_until":m.valid_until,"revoked_at":null,"version":1,"parent_id":m.parent_id,"source":"directory","source_id":m.id});
        if !platform {
            let object = expected.as_object_mut().ok_or_else(unavailable)?;
            object.insert("company_id".into(), d.company_id.clone().into());
            object.insert(
                "branch_id".into(),
                serde_json::to_value(
                    scope
                        .as_ref()
                        .and_then(|s| s.branch())
                        .map(|n| n.id.as_str()),
                )
                .map_err(|_| unavailable())?,
            );
            object.insert("scope_kind".into(), m.scope_kind.clone().into());
            object.insert(
                "scope_node_id".into(),
                serde_json::to_value(&m.scope_node_id).map_err(|_| unavailable())?,
            );
        }
        retained.remember(expected)?;
        if platform {
            sqlx::query("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,parent_id,source,source_id) VALUES($1,$2,$3,$4,$5,$6,$7,'directory',$8)").bind(&id).bind(&m.user_id).bind(&m.role_id).bind(&modules).bind(now).bind(m.valid_until).bind(&m.parent_id).bind(&m.id).execute(&mut *c).await?;
        } else {
            let branch = scope
                .as_ref()
                .and_then(|s| s.branch())
                .map(|n| n.id.as_str());
            sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source,source_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'directory',$12)").bind(&id).bind(&m.user_id).bind(&m.role_id).bind(&modules).bind(&d.company_id).bind(branch).bind(now).bind(m.valid_until).bind(&m.scope_kind).bind(&m.scope_node_id).bind(&m.parent_id).bind(&m.id).execute(&mut *c).await?;
        }
    }
    retained.require_effects(c).await?;
    Ok(retained)
}
