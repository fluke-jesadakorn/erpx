//! PostgreSQL adapter for the native People access grant slice.
//!
//! SQL is Identity-owned (`iam_access_subjects`, `iam_access_invitations`,
//! `iam_access_previews`, `iam_roles`, `iam_role_revisions`, `iam_members`,
//! `assignments`, `iam_audit`) plus the shared command receipt, audit and
//! outbox tables. `hr_people`, `hr_engagements` and `organization_nodes` are
//! read-only source facts. The caller's serializable Holding transaction is
//! required and no commit is performed here.
//!
//! Invitation *delivery* is a Control-database effect. It runs after the
//! Holding unit commits and is exposed separately
//! ([`read_direct_invitation`] + [`stage_direct_invitation_delivery`]) so the
//! composition root can supply the Control connection and the invitation token
//! secret.
use crate::application::people_grants::{PeopleGrantFuture, PeopleGrantRepository};
use crate::domain::people_grants::{
    GrantApplyPlan, GrantAssignmentRow, GrantPreviewPlan,
    GrantPreviewRow, GrantReceipt, GrantRoleRow, GrantSnapshot, GrantSubjectRow, GrantUnitRow,
    GRANT_APPLY_OPERATION, PeopleGrantActor, PeopleGrantChange, access_object_changed,
    account_already_linked, directory_account_link_required, directory_identity_review_required,
    grant_apply_result, identity_inactive, idempotency_conflict, readback_invalid, role_changed,
    scope_unavailable,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgPeopleGrants<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPeopleGrants<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PEOPLE_ACCESS_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("PEOPLE_ACCESS_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("people-access-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn write_snapshot(&mut self, actor: &PeopleGrantActor) -> Result<GrantSnapshot, Error> {
        let company = actor.company_id.as_str();
        let mut subjects: Vec<GrantSubjectRow> = sqlx::query(
            "SELECT id,name,email,kind,user_id,person_id,organization_id,sponsor_person_id,\
             expires_at::bigint AS expires_at,status,version::bigint AS version \
             FROM iam_access_subjects \
             WHERE company_id=$1 AND kind='external' ORDER BY name,id",
        )
        .bind(company)
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|row| GrantSubjectRow {
            id: row.get("id"),
            name: row.get("name"),
            email: row.get("email"),
            kind: row.get("kind"),
            user_id: row.get("user_id"),
            person_id: row.get("person_id"),
            organization_id: row.get("organization_id"),
            sponsor_person_id: row.get("sponsor_person_id"),
            expires_at: row.get("expires_at"),
            status: row.get("status"),
            version: row.get("version"),
        })
        .collect();
        subjects.extend(
            sqlx::query(
                "SELECT 'employee:'||p.id AS id,p.legal_name AS name,COALESCE(p.email,'') AS email,\
                 p.user_id,p.id AS person_id,p.status,p.version::bigint AS version \
                 FROM hr_people p WHERE p.company_id=$1 AND p.merged_into_id IS NULL \
                 AND p.status='active' AND NOT EXISTS(SELECT 1 FROM iam_access_subjects s \
                   WHERE s.company_id=p.company_id AND s.kind='external' AND s.person_id=p.id) \
                 ORDER BY p.legal_name,p.id",
            )
            .bind(company)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| GrantSubjectRow {
                id: row.get("id"),
                name: row.get("name"),
                email: row.get("email"),
                kind: "internal".into(),
                user_id: row.get("user_id"),
                person_id: row.get("person_id"),
                organization_id: None,
                sponsor_person_id: None,
                expires_at: None,
                status: row.get("status"),
                version: row.get("version"),
            }),
        );
        subjects.extend(
            sqlx::query(
                "SELECT id,name,email,user_id,status,version::bigint AS version \
                 FROM iam_access_subjects WHERE company_id=$1 AND kind='internal' \
                 AND person_id IS NULL ORDER BY name,id",
            )
            .bind(company)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| GrantSubjectRow {
                id: row.get("id"),
                name: row.get("name"),
                email: row.get("email"),
                kind: "unclassified".into(),
                user_id: row.get("user_id"),
                person_id: None,
                organization_id: None,
                sponsor_person_id: None,
                expires_at: None,
                status: row.get("status"),
                version: row.get("version"),
            }),
        );
        let roles = sqlx::query(
            "SELECT r.id,r.name,r.built_in,r.published_revision::bigint AS revision,\
             v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r \
             JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
             AND v.revision=r.published_revision \
             WHERE r.company_id IN ('*',$1) AND r.status='active' ORDER BY r.name,r.id",
        )
        .bind(company)
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|row| GrantRoleRow {
            id: row.get("id"),
            name: row.get("name"),
            revision: row.get("revision"),
            built_in: row.get("built_in"),
            permissions: json_list(row.get("permissions_json")),
            grantable_permissions: json_list(row.get("grantable_json")),
            scopes: json_list(row.get("scopes_json")),
        })
        .collect();
        let assignments = sqlx::query(
            "SELECT id,user_id,role AS role_id,scope_kind,scope_node_id,\
             valid_until::bigint AS valid_until,version::bigint AS version,\
             iam_assignment_active(id,$2) AS active FROM assignments \
             WHERE company_id=$1 AND revoked_at IS NULL ORDER BY user_id,id",
        )
        .bind(company)
        .bind(actor.expires_at.min(i64::MAX))
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|row| GrantAssignmentRow {
            id: row.get("id"),
            user_id: row.get("user_id"),
            role_id: row.get("role_id"),
            scope_kind: row.get("scope_kind"),
            scope_node_id: row.get("scope_node_id"),
            valid_until: row.get("valid_until"),
            version: row.get("version"),
            active: row.get("active"),
        })
        .collect();
        let units = sqlx::query(
            "SELECT id,kind,branch_id FROM organization_nodes \
             WHERE company_id=$1 AND status='active' AND kind IN ('branch','department') \
             ORDER BY id",
        )
        .bind(company)
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|row| GrantUnitRow {
            id: row.get("id"),
            kind: row.get("kind"),
            branch_id: row.get("branch_id"),
        })
        .collect();
        let authorizing = sqlx::query(
            "SELECT a.scope_kind,a.scope_node_id,a.valid_until::bigint AS valid_until,\
             COALESCE(v.grantable_json,'[]'::jsonb) AS grantable FROM assignments a \
             LEFT JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) \
             LEFT JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
             AND v.revision=r.published_revision \
             WHERE a.id=$1 AND a.company_id=$2",
        )
        .bind(actor.assignment_id.as_str())
        .bind(company)
        .fetch_optional(&mut *self.connection)
        .await?;
        let (authorizing_until, authorizing_scope_kind, authorizing_scope_node_id, grantable) =
            match authorizing {
                Some(row) => (
                    row.get::<i64, _>("valid_until"),
                    row.get::<String, _>("scope_kind"),
                    row.get::<Option<String>, _>("scope_node_id"),
                    json_list(row.get("grantable")),
                ),
                None => (0, String::new(), None, Vec::new()),
            };
        Ok(GrantSnapshot {
            subjects,
            roles,
            assignments,
            units,
            authorizing_until,
            authorizing_scope_kind,
            authorizing_scope_node_id,
            grantable_permissions: grantable,
        })
    }

    async fn role_permissions(
        &mut self,
        company: &str,
        role_id: &str,
        revision: i64,
    ) -> Result<Vec<String>, Error> {
        let row = sqlx::query(
            "SELECT permissions_json FROM iam_role_revisions \
             WHERE company_id IN ('*',$1) AND role_id=$2 AND revision=$3",
        )
        .bind(company)
        .bind(role_id)
        .bind(revision)
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(row.map(|row| json_list(row.get("permissions_json"))).unwrap_or_default())
    }

    async fn published_revision(
        &mut self,
        company: &str,
        role_id: &str,
    ) -> Result<Option<i64>, Error> {
        Ok(sqlx::query_scalar(
            "SELECT published_revision::bigint FROM iam_roles \
             WHERE company_id IN ('*',$1) AND id=$2 AND status='active'",
        )
        .bind(company)
        .bind(role_id)
        .fetch_optional(&mut *self.connection)
        .await?)
    }

    async fn scope_branch(
        &mut self,
        company: &str,
        scope_kind: &str,
        scope_node_id: Option<&str>,
    ) -> Result<Option<String>, Error> {
        let Some(node) = scope_node_id else {
            return Ok(None);
        };
        let branch: Option<String> = sqlx::query_scalar(
            "SELECT CASE WHEN $2='branch' THEN id WHEN $2='department' THEN branch_id ELSE NULL END \
             FROM organization_nodes WHERE company_id=$1 AND id=$3 AND status='active'",
        )
        .bind(company)
        .bind(scope_kind)
        .bind(node)
        .fetch_optional(&mut *self.connection)
        .await?;
        Ok(branch)
    }

    async fn write_account(
        &mut self,
        actor: &PeopleGrantActor,
        plan: &GrantApplyPlan,
    ) -> Result<(), Error> {
        let PeopleGrantChange::Account { name, email, .. } = &plan.change else {
            return Err(readback_invalid().into());
        };
        sqlx::query(
            "INSERT INTO iam_access_subjects(id,company_id,kind,name,email) \
             VALUES($1,$2,'internal',$3,$4) ON CONFLICT(company_id,id) DO NOTHING",
        )
        .bind(&plan.id)
        .bind(actor.company_id.as_str())
        .bind(name)
        .bind(email)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn write_role(
        &mut self,
        actor: &PeopleGrantActor,
        plan: &GrantApplyPlan,
    ) -> Result<(), Error> {
        let PeopleGrantChange::Role {
            name,
            permissions,
            scopes,
            ..
        } = &plan.change
        else {
            return Err(readback_invalid().into());
        };
        let company = actor.company_id.as_str();
        if plan.expected_version == 0 {
            let created = sqlx::query(
                "INSERT INTO iam_roles(company_id,id,name,built_in) VALUES($1,$2,$3,false) \
                 ON CONFLICT(company_id,id) DO NOTHING",
            )
            .bind(company)
            .bind(&plan.id)
            .bind(name)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if created != 1 {
                return Err(role_changed().into());
            }
        } else {
            let renamed = sqlx::query(
                "UPDATE iam_roles SET name=$3 WHERE company_id=$1 AND id=$2 \
                 AND published_revision=$4 AND built_in=false",
            )
            .bind(company)
            .bind(&plan.id)
            .bind(name)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if renamed != 1 {
                return Err(role_changed().into());
            }
        }
        // A published custom role keeps its delegation ceiling: publishing new
        // permissions never widens what the role may itself grant.
        let retained = sqlx::query(
            "SELECT grantable_json FROM iam_role_revisions WHERE company_id=$1 AND role_id=$2 \
             AND revision=(SELECT published_revision FROM iam_roles WHERE company_id=$1 AND id=$2)",
        )
        .bind(company)
        .bind(&plan.id)
        .fetch_optional(&mut *self.connection)
        .await?
        .map(|row| json_list(row.get("grantable_json")))
        .unwrap_or_default();
        let revision: i64 = sqlx::query_scalar(
            "SELECT (COALESCE(max(revision),0)+1)::bigint FROM iam_role_revisions \
             WHERE company_id=$1 AND role_id=$2",
        )
        .bind(company)
        .bind(&plan.id)
        .fetch_one(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,\
             grantable_json,scopes_json,created_by,created_at) \
             VALUES($1,$2,$3,$4::jsonb,$5::jsonb,$6::jsonb,$7,$8)",
        )
        .bind(company)
        .bind(&plan.id)
        .bind(revision)
        .bind(serde_json::to_string(permissions).map_err(|_| readback_invalid())?)
        .bind(serde_json::to_string(&retained).map_err(|_| readback_invalid())?)
        .bind(serde_json::to_string(scopes).map_err(|_| readback_invalid())?)
        .bind(actor.user_id.as_str())
        .bind(plan.now)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query("UPDATE iam_roles SET published_revision=$3 WHERE company_id=$1 AND id=$2")
            .bind(company)
            .bind(&plan.id)
            .bind(revision)
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn write_revoke(
        &mut self,
        actor: &PeopleGrantActor,
        plan: &GrantApplyPlan,
    ) -> Result<(), Error> {
        let Some(assignment_id) = plan.assignment_id.as_deref() else {
            return Err(readback_invalid().into());
        };
        let changed = sqlx::query(
            "UPDATE assignments SET revoked_at=$3,version=version+1 \
             WHERE id=$1 AND company_id=$2 AND version=$4 AND revoked_at IS NULL",
        )
        .bind(assignment_id)
        .bind(actor.company_id.as_str())
        .bind(plan.now)
        .bind(plan.expected_version)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(access_object_changed().into());
        }
        Ok(())
    }

    async fn write_grant(
        &mut self,
        actor: &PeopleGrantActor,
        key: &str,
        plan: &GrantApplyPlan,
    ) -> Result<String, Error> {
        let company = actor.company_id.as_str();
        let Some(subject) = plan.subject.as_ref() else {
            return Err(readback_invalid().into());
        };
        let Some(role_id) = plan.change.role_id() else {
            return Err(readback_invalid().into());
        };
        let Some(role_revision) = plan.role_revision else {
            return Err(readback_invalid().into());
        };
        // The reviewed role revision is the compare-and-swap guard: a role
        // republished between preview and apply cannot widen this grant.
        if self.published_revision(company, role_id).await? != Some(role_revision) {
            return Err(role_changed().into());
        }
        let scope_kind = plan.scope_kind.clone().unwrap_or_default();
        let scope_node_id = plan.scope_node_id.clone();
        if scope_kind != "holding" && scope_node_id.is_none() {
            return Err(scope_unavailable().into());
        }
        let valid_until = plan.valid_until.unwrap_or(0);
        let branch_id = self
            .scope_branch(company, &scope_kind, scope_node_id.as_deref())
            .await?;
        let permissions = self
            .role_permissions(company, role_id, role_revision)
            .await?;
        let modules = module_names(&permissions);
        let invitation = matches!(plan.change, PeopleGrantChange::Invite { .. });
        let user_id = if invitation {
            let Some(invitation_id) = plan.invitation_id.as_deref() else {
                return Err(readback_invalid().into());
            };
            let user_id = format!("usr_invite_{invitation_id}");
            if subject.kind == "external" {
                let changed = sqlx::query(
                    "UPDATE iam_access_subjects SET user_id=$3,status='invited',\
                     version=version+1 WHERE id=$1 AND company_id=$2 AND user_id IS NULL",
                )
                .bind(&subject.id)
                .bind(company)
                .bind(&user_id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(access_object_changed().into());
                }
            } else {
                sqlx::query(
                    "INSERT INTO iam_access_subjects(id,company_id,kind,name,email,user_id,\
                     person_id,status) VALUES($1,$2,'internal',$3,$4,$5,$6,'invited') \
                     ON CONFLICT(company_id,id) DO UPDATE SET user_id=$5,status='invited',\
                     version=iam_access_subjects.version+1 WHERE \
                     iam_access_subjects.user_id IS NULL",
                )
                .bind(&subject.id)
                .bind(company)
                .bind(&subject.name)
                .bind(&subject.email)
                .bind(&user_id)
                .bind(&subject.person_id)
                .execute(&mut *self.connection)
                .await?;
            }
            sqlx::query(
                "INSERT INTO iam_members(company_id,user_id,principal_id,person_id) \
                 VALUES($1,$2,$3,$4) ON CONFLICT(company_id,user_id) DO NOTHING",
            )
            .bind(company)
            .bind(&user_id)
            .bind(format!("principal-{invitation_id}"))
            .bind(&subject.person_id)
            .execute(&mut *self.connection)
            .await?;
            user_id
        } else {
            let Some(user_id) = subject.user_id.clone() else {
                return Err(crate::domain::people_grants::invite_required().into());
            };
            user_id
        };
        let member = sqlx::query(
            "SELECT status,login_policy FROM iam_members WHERE company_id=$1 AND user_id=$2",
        )
        .bind(company)
        .bind(&user_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        if let Some(member) = &member {
            let status: String = member.get("status");
            let login_policy: String = member.get("login_policy");
            if status != "active" {
                return Err(identity_inactive().into());
            }
            if user_id.starts_with("dir_") && login_policy != "directory" {
                return Err(directory_identity_review_required().into());
            }
        }
        let assignment_id = format!("iam-assignment-{}", &digest_binding(key, &plan.id));
        sqlx::query(
            "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,\
             valid_until,scope_kind,scope_node_id,parent_id,source,version) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'manual',1)",
        )
        .bind(&assignment_id)
        .bind(&user_id)
        .bind(role_id)
        .bind(serde_json::to_string(&modules).map_err(|_| readback_invalid())?)
        .bind(company)
        .bind(&branch_id)
        .bind(plan.now)
        .bind(valid_until)
        .bind(&scope_kind)
        .bind(&scope_node_id)
        .bind(actor.assignment_id.as_str())
        .execute(&mut *self.connection)
        .await?;
        if let Some(invitation_id) = plan.invitation_id.as_deref() {
            let definition = json!({
                "userId": user_id,
                "roleId": role_id,
                "expectedRoleRevision": role_revision,
                "scopeKind": scope_kind,
                "scopeNodeId": scope_node_id,
                "validUntil": valid_until,
                "reason": match &plan.change {
                    PeopleGrantChange::Invite { reason, .. } => reason.clone(),
                    _ => String::new(),
                },
                "assignmentId": assignment_id,
                "existingUserId": null,
                "email": subject.email,
                "name": subject.name,
            });
            sqlx::query(
                "INSERT INTO iam_access_invitations(id,company_id,subject_id,definition,author_id,\
                 parent_id,role_revision) VALUES($1,$2,$3,$4::jsonb,$5,$6,$7)",
            )
            .bind(invitation_id)
            .bind(company)
            .bind(&subject.id)
            .bind(definition.to_string())
            .bind(actor.user_id.as_str())
            .bind(actor.assignment_id.as_str())
            .bind(role_revision)
            .execute(&mut *self.connection)
            .await?;
        }
        Ok(assignment_id)
    }

    #[allow(clippy::too_many_arguments)]
    async fn effects(
        &mut self,
        actor: &PeopleGrantActor,
        key: &str,
        request_hash: &str,
        plan: &GrantApplyPlan,
        result: &Value,
    ) -> Result<(), Error> {
        let authority = actor.authority_json();
        let receipt = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(GRANT_APPLY_OPERATION)
        .bind(actor.user_id.as_str())
        .bind(request_hash)
        .bind(&plan.id)
        .bind(result.to_string())
        .bind(plan.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if receipt != 1 {
            return Err(idempotency_conflict().into());
        }
        let evidence = json!({
            "input": {"operation": plan.change.operation()},
            "result": result,
            "scope": plan.scope_kind,
            "authority": serde_json::from_str::<Value>(&authority)
                .map_err(|_| readback_invalid())?,
        });
        sqlx::query(
            "INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,\
             created_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6::jsonb,$7)",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(GRANT_APPLY_OPERATION)
        .bind(key)
        .bind(evidence.to_string())
        .bind(plan.now)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(GRANT_APPLY_OPERATION)
        .bind(format!("iam.people-access.{}", plan.change.operation()))
        .bind(&plan.id)
        .bind(1_i64)
        .bind(plan.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        let payload = json!({
            "operation": plan.change.operation(),
            "id": plan.id,
            "subjectId": plan.subject_id,
            "assignmentId": plan.assignment_id,
            "invitationId": plan.invitation_id,
            "authority": serde_json::from_str::<Value>(&authority)
                .map_err(|_| readback_invalid())?,
        });
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(key)
        .bind(format!("iam.people-access.{}", plan.change.operation()))
        .bind("iam.people-access")
        .bind(&plan.id)
        .bind(1_i64)
        .bind(payload.to_string())
        .bind(plan.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

fn json_list(value: Value) -> Vec<String> {
    serde_json::from_value::<Vec<String>>(value).unwrap_or_default()
}

/// A module permission contributes its module name to `assignments.modules_json`.
fn module_names(permissions: &[String]) -> Vec<String> {
    let mut modules: Vec<String> = Vec::new();
    for permission in permissions {
        if let Some(module) = permission.strip_prefix("module.") {
            let name = module.split('.').next().unwrap_or(module);
            if !name.is_empty() && !modules.iter().any(|seen| seen == name) {
                modules.push(name.to_owned());
            }
        }
    }
    modules
}

/// Deterministic assignment identity for a replayed command.
fn digest_binding(key: &str, record: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(key.as_bytes());
    digest.update(b":");
    digest.update(record.as_bytes());
    hex::encode(digest.finalize())[..32].to_owned()
}

impl PeopleGrantRepository for PgPeopleGrants<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        permission: &'a str,
        now: i64,
    ) -> PeopleGrantFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM assignments a \
                 JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id \
                 JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) \
                  AND r.status='active' \
                 JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
                  AND v.revision=r.published_revision \
                 WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
                 AND iam_assignment_active(a.id,$4) AND iam_role_allows(a.role,a.company_id,$5) \
                 AND m.status='active' \
                 AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions x WHERE x.session_id=$6 \
                  AND x.company_id=$3 AND x.user_id=$2))",
            )
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .bind(permission)
            .bind(actor.session_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> PeopleGrantFuture<'a, String, Self::Error> {
        Box::pin(async move {
            Ok(sqlx::query_scalar("SELECT encode(sha256($1::bytea),'hex')")
                .bind(value.as_bytes())
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn snapshot<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
    ) -> PeopleGrantFuture<'a, GrantSnapshot, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.write_snapshot(actor).await
        })
    }

    fn same_person<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        user_id: &'a str,
    ) -> PeopleGrantFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            Ok(sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
                .bind(user_id)
                .bind(actor.user_id.as_str())
                .bind(actor.company_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PeopleGrantFuture<'a, Option<GrantReceipt>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT request_hash,operation,actor_id,record_id,result_json,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| GrantReceipt {
                request_hash: row.get("request_hash"),
                operation: row.get("operation"),
                actor_id: row.get("actor_id"),
                record_id: row.get("record_id"),
                result_json: row
                    .try_get::<Option<String>, _>("result_json")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                authority_json: row
                    .try_get::<Option<String>, _>("authority_json")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
            }))
        })
    }

    fn commit_preview<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        plan: &'a GrantPreviewPlan,
    ) -> PeopleGrantFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            if plan.company_id != actor.company_id
                || plan.actor_id != actor.user_id
                || plan.assignment_id != actor.assignment_id
                || plan.session_id != actor.session_id
            {
                return Err(readback_invalid().into());
            }
            Ok(sqlx::query_scalar(
                "INSERT INTO iam_access_previews(token,company_id,actor_id,assignment_id,\
                 session_id,change,fingerprint,expires_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5::jsonb,$6,$7) RETURNING token",
            )
            .bind(&plan.company_id)
            .bind(&plan.actor_id)
            .bind(&plan.assignment_id)
            .bind(&plan.session_id)
            .bind(&plan.change_json)
            .bind(&plan.fingerprint)
            .bind(plan.expires_at)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn preview_for_update<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        token: &'a str,
        now: i64,
    ) -> PeopleGrantFuture<'a, Option<GrantPreviewRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT change::text AS change_json,fingerprint FROM iam_access_previews \
                 WHERE token=$1 AND company_id=$2 AND actor_id=$3 AND assignment_id=$4 \
                 AND session_id=$5 AND expires_at>$6 FOR UPDATE",
            )
            .bind(token)
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.assignment_id.as_str())
            .bind(actor.session_id.as_str())
            .bind(now)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| GrantPreviewRow {
                change_json: row.get("change_json"),
                fingerprint: row.get("fingerprint"),
            }))
        })
    }

    fn consume_preview<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        token: &'a str,
    ) -> PeopleGrantFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let consumed = sqlx::query(
                "DELETE FROM iam_access_previews WHERE token=$1 AND company_id=$2 AND actor_id=$3",
            )
            .bind(token)
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if consumed != 1 {
                return Err(readback_invalid().into());
            }
            Ok(())
        })
    }

    fn commit_apply<'a>(
        &'a mut self,
        actor: &'a PeopleGrantActor,
        key: &'a str,
        plan: &'a GrantApplyPlan,
        request_hash: &'a str,
    ) -> PeopleGrantFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let mut assignment_id = None;
            match &plan.change {
                PeopleGrantChange::Account { .. } => self.write_account(actor, plan).await?,
                PeopleGrantChange::Role { .. } => self.write_role(actor, plan).await?,
                PeopleGrantChange::Revoke { .. } => self.write_revoke(actor, plan).await?,
                PeopleGrantChange::Grant { .. } | PeopleGrantChange::Invite { .. } => {
                    assignment_id = Some(self.write_grant(actor, key, plan).await?)
                }
            }
            let change = serde_json::to_value(&plan.change).map_err(|_| readback_invalid())?;
            let mut result = grant_apply_result(plan, &change);
            if let (Some(assignment_id), Some(object)) =
                (assignment_id.as_deref(), result.as_object_mut())
            {
                object.insert("assignmentId".into(), json!(assignment_id));
            }
            self.effects(actor, key, request_hash, plan, &result).await?;
            Ok(result)
        })
    }
}

/// One authorized direct invitation, as committed in the Holding database.
///
/// The composition root reads it after the Holding unit commits and stages the
/// Control-database delivery rows. `existing_user_id` is resolved by the staging
/// step because the Control account table is not part of this transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectInvitationDelivery {
    pub invitation_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub subject_id: String,
    pub email: String,
    pub name: String,
    pub role_id: String,
    pub valid_until: i64,
    pub scope_kind: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub parent_id: String,
    pub authorizer_id: String,
    pub existing_user_id: Option<String>,
}

/// Read the committed review row for a direct invitation.
pub async fn read_direct_invitation(
    connection: &mut PgConnection,
    holding_id: &str,
    company_id: &str,
    invitation_id: &str,
) -> Result<DirectInvitationDelivery, PgTransactionError<BusinessError>> {
    let row = sqlx::query(
        "SELECT i.subject_id,i.definition,i.parent_id,i.author_id,s.email,s.name AS subject_name \
         FROM iam_access_invitations i \
         JOIN iam_access_subjects s ON s.company_id=i.company_id AND s.id=i.subject_id \
         WHERE i.id=$1 AND i.company_id=$2 AND i.state='authorized'",
    )
    .bind(invitation_id)
    .bind(company_id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(crate::domain::people_grants::access_object_unavailable)?;
    let definition: Value = row.get("definition");
    let scope_kind: String = text(&definition, "scopeKind")?;
    let scope_node_id = optional_text(&definition, "scopeNodeId");
    let (branch_id, department_id) = if scope_kind == "department" {
        (None, scope_node_id)
    } else if scope_kind == "branch" {
        (scope_node_id, None)
    } else {
        (None, None)
    };
    Ok(DirectInvitationDelivery {
        invitation_id: invitation_id.to_owned(),
        holding_id: holding_id.to_owned(),
        company_id: company_id.to_owned(),
        subject_id: row.get("subject_id"),
        email: row.get("email"),
        name: row.get("subject_name"),
        role_id: text(&definition, "roleId")?,
        valid_until: definition["validUntil"]
            .as_i64()
            .ok_or_else(readback_invalid)?,
        scope_kind,
        branch_id,
        department_id,
        parent_id: row.get("parent_id"),
        authorizer_id: row.get("author_id"),
        existing_user_id: optional_text(&definition, "existingUserId"),
    })
}

fn text(value: &Value, key: &str) -> Result<String, PgTransactionError<BusinessError>> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| readback_invalid().into())
}

fn optional_text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Stage the Control-database invitation and delivery job after the Holding
/// transaction commits. `token_hash` is the caller's HMAC of the invitation
/// secret; this adapter never holds the secret.
pub async fn stage_direct_invitation_delivery(
    connection: &mut PgConnection,
    delivery: &DirectInvitationDelivery,
    token_hash: &str,
    now: i64,
) -> Result<(), PgTransactionError<BusinessError>> {
    let existing = sqlx::query(
        "SELECT u.id, EXISTS(SELECT 1 FROM account a WHERE a.\"userId\"=u.id \
         AND a.\"providerId\"='credential') AS credential \
         FROM \"user\" u WHERE lower(u.email)=lower($1) LIMIT 1",
    )
    .bind(&delivery.email)
    .fetch_optional(&mut *connection)
    .await?;
    let existing_user_id = match existing {
        Some(row) => {
            if !row.get::<bool, _>("credential") {
                return Err(directory_account_link_required().into());
            }
            let linked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2)",
            )
            .bind(&delivery.company_id)
            .bind(row.get::<String, _>("id"))
            .fetch_one(&mut *connection)
            .await?;
            if linked {
                return Err(account_already_linked().into());
            }
            Some(row.get::<String, _>("id"))
        }
        None => None,
    };
    let role = json!({
        "role": delivery.role_id,
        "expires": delivery.valid_until,
        "direct": true,
        "existingUserId": existing_user_id,
    });
    sqlx::query(
        "INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,\
         name,role,scope_kind,branch_id,department_id,created_by,assignment_id,expires_at,\
         created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) \
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&delivery.invitation_id)
    .bind(token_hash)
    .bind(&delivery.holding_id)
    .bind(&delivery.company_id)
    .bind(&delivery.subject_id)
    .bind(&delivery.email)
    .bind(&delivery.name)
    .bind(role.to_string())
    .bind(&delivery.scope_kind)
    .bind(&delivery.branch_id)
    .bind(&delivery.department_id)
    .bind(&delivery.authorizer_id)
    .bind(&delivery.parent_id)
    .bind((now + 48 * 3_600_000).min(delivery.valid_until))
    .bind(now)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO account_invitation_delivery_jobs(id,invitation_id,holding_id,company_id,\
         state,next_attempt_at,created_at,updated_at) \
         VALUES($1,$2,$3,$4,'queued',$5,$5,$5) ON CONFLICT(invitation_id) DO NOTHING",
    )
    .bind(format!("account-invite-delivery-{}", delivery.invitation_id))
    .bind(&delivery.invitation_id)
    .bind(&delivery.holding_id)
    .bind(&delivery.company_id)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Isolated PostgreSQL proof for the native People access grant producer.
///
/// Runs only when `ERPX_TEST_ADMIN_URL` points at the authorized local
/// PostgreSQL 18 test administrator, so the default `cargo test --lib` run
/// stays hermetic. The serializable owner unit is opened by the test exactly as
/// composition does; no commit happens inside the adapter.
#[cfg(test)]
mod postgres_tests {
    use super::{
        DirectInvitationDelivery, PgPeopleGrants, read_direct_invitation,
    };
    use crate::adapters::permission_catalog::permission_catalog;
    use crate::application::people_grants::{apply_grant_change, preview_grant_change};
    use crate::domain::people_grants::PeopleGrantActor;
    use crate::people_fixture as fixture;
    use erpx_server_runtime::public::postgres::{
        DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
    };
    use erpx_server_shared::public::{BusinessError, CommandKey};
    use serde_json::{Value, json};
    use sqlx::Row;

    const NOW: i64 = 5_000;

    fn actor(permissions: &[&str]) -> PeopleGrantActor {
        PeopleGrantActor {
            user_id: "actor".into(),
            session_id: "session-actor".into(),
            company_id: "company".into(),
            assignment_id: "assignment-actor".into(),
            scope_kind: "holding".into(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            platform: false,
            expires_at: 1_000_000,
        }
    }

    fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
        match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected an owner business failure, got {other:?}"),
        }
    }

    async fn setup(pool: &DatabasePool) {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(pool, MigrationTrack::HoldingWrite, &files)
            .await
            .unwrap();
        sqlx::raw_sql(
            "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
             ('*','it_admin','IT administrator',true,99,'active') \
             ON CONFLICT(company_id,id) DO UPDATE SET published_revision=99,status='active';\
             INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,\
             grantable_json,scopes_json,created_by,created_at) VALUES \
             ('*','it_admin',99,'[\"iam.read\",\"iam.access.apply\",\"iam.roles.manage\",\
             \"people.relationships.read\"]','[\"support.read\",\"support.manage\"]',\
             '[\"holding\",\"branch\",\"department\"]','system',0) \
             ON CONFLICT(company_id,role_id,revision) DO NOTHING;\
             INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
             ('company','custom_published','Published reader',false,3,'active') \
             ON CONFLICT(company_id,id) DO UPDATE SET published_revision=3,status='active';\
             INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,\
             grantable_json,scopes_json,created_by,created_at) VALUES \
             ('company','custom_published',3,'[\"support.read\"]','[\"support.manage\"]',\
             '[\"holding\",\"department\"]','system',0) \
             ON CONFLICT(company_id,role_id,revision) DO NOTHING;\
             INSERT INTO organization_nodes(id,company_id,parent_id,branch_id,kind,display_name,\
             slug,status,hierarchy_revision) VALUES \
             ('company','company',NULL,NULL,'company','Acme','acme','active',1),\
             ('branch-1','company','company','branch-1','branch','HQ','hq','active',1),\
             ('department-1','company','branch-1','branch-1','department','Operations',\
             'operations','active',1);\
             INSERT INTO hr_people(id,company_id,legal_name,country_code,status,version,created_by,\
             created_at,updated_at) VALUES \
             ('person-actor','company','Ada Actor','GB','active',1,'system',0,0),\
             ('person-sponsor','company','Sam Sponsor','GB','active',1,'system',0,0);\
             INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,\
             version) VALUES \
             ('company','actor','local:actor','person-actor','active','local',1);\
             INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,\
             valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) \
             VALUES('assignment-actor','actor','it_admin','[]','company',NULL,0,9000000000000000,\
             NULL,1,'holding',NULL,NULL,'manual',NULL);\
             INSERT INTO iam_external_organizations(id,company_id,name,kind) VALUES \
             ('org-1','company','Visitor company','organization');\
             INSERT INTO iam_access_subjects(id,company_id,kind,name,email,organization_id,\
             sponsor_person_id,expires_at,status,version) VALUES \
             ('subject-external','company','external','Visitor','visitor@example.invalid','org-1',\
             'person-sponsor',950000,'active',1);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
    }

    async fn preview(
        pool: &DatabasePool,
        actor: &PeopleGrantActor,
        change: Value,
    ) -> Result<Value, PgTransactionError<BusinessError>> {
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let result = {
            let mut repository = PgPeopleGrants::new(&mut tx);
            preview_grant_change(
                &mut repository,
                actor,
                permission_catalog().unwrap(),
                change,
                NOW,
            )
            .await
        };
        match result {
            Ok(value) => {
                tx.commit().await.unwrap();
                Ok(value)
            }
            Err(error) => {
                tx.rollback().await.unwrap();
                Err(error)
            }
        }
    }

    async fn apply(
        pool: &DatabasePool,
        actor: &PeopleGrantActor,
        token: &str,
        key: &str,
    ) -> Result<Value, PgTransactionError<BusinessError>> {
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let command = CommandKey::new(key).unwrap();
        let result = {
            let mut repository = PgPeopleGrants::new(&mut tx);
            apply_grant_change(
                &mut repository,
                actor,
                permission_catalog().unwrap(),
                &command,
                token,
                NOW,
            )
            .await
        };
        match result {
            Ok(value) => {
                tx.commit().await.unwrap();
                Ok(value)
            }
            Err(error) => {
                tx.rollback().await.unwrap();
                Err(error)
            }
        }
    }

    async fn reviewed(
        pool: &DatabasePool,
        actor: &PeopleGrantActor,
        change: Value,
        key: &str,
    ) -> Value {
        let preview = preview(pool, actor, change).await.unwrap();
        let token = preview["token"].as_str().unwrap().to_owned();
        apply(pool, actor, &token, key).await.unwrap()
    }

    fn invite(valid_until: i64) -> Value {
        json!({
            "operation": "invite",
            "subjectId": "subject-external",
            "roleId": "custom_published",
            "scopeKind": "department",
            "scopeNodeId": "department-1",
            "validUntil": valid_until,
            "reason": "Invite the visitor",
        })
    }

    fn permissions() -> Vec<&'static str> {
        vec![
            "iam.read",
            "iam.access.apply",
            "iam.roles.manage",
            "people.relationships.read",
        ]
    }

    #[tokio::test]
    #[ignore = "requires ERPX_TEST_ADMIN_URL for the isolated local PostgreSQL fixture"]
    async fn publishing_a_custom_role_is_reviewed_cas_guarded_and_replay_safe() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let actor = actor(&permissions());
            let change = json!({
                "operation": "role",
                "expectedVersion": 0,
                "name": "Support reader",
                "permissions": ["support.read"],
                "scopes": ["holding"],
                "reason": "Publish a bounded role",
            });
            let reviewed_change = preview(&pool, &actor, change.clone()).await.unwrap();
            let token = reviewed_change["token"].as_str().unwrap().to_owned();
            let applied = apply(&pool, &actor, &token, "command-role").await.unwrap();
            let id = applied["id"].as_str().unwrap().to_owned();
            assert!(id.starts_with("custom_"));

            let row = sqlx::query(
                "SELECT built_in,published_revision FROM iam_roles \
                 WHERE company_id='company' AND id=$1",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert!(!row.get::<bool, _>("built_in"));
            assert_eq!(row.get::<i32, _>("published_revision"), 1);
            let revision = sqlx::query(
                "SELECT permissions_json,grantable_json,scopes_json FROM iam_role_revisions \
                 WHERE company_id='company' AND role_id=$1 AND revision=1",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                revision.get::<Value, _>("permissions_json"),
                json!(["support.read"])
            );
            assert_eq!(revision.get::<Value, _>("grantable_json"), json!([]));

            // Same key replays the recorded receipt; a new key cannot reuse the token.
            assert_eq!(
                apply(&pool, &actor, &token, "command-role").await.unwrap(),
                applied
            );
            assert_eq!(
                business(apply(&pool, &actor, &token, "command-role-2").await.unwrap_err()).code,
                "PREVIEW_EXPIRED"
            );

            // Publishing a new revision keeps the retained delegation ceiling.
            let role = reviewed(
                &pool,
                &actor,
                json!({
                    "operation": "role",
                    "id": "custom_published",
                    "expectedVersion": 3,
                    "name": "Published reader",
                    "permissions": ["support.read"],
                    "scopes": ["holding"],
                    "reason": "Republish the role",
                }),
                "command-role-republish",
            )
            .await;
            assert_eq!(role["id"], "custom_published");
            let retained: Value = sqlx::query_scalar(
                "SELECT grantable_json FROM iam_role_revisions WHERE company_id='company' \
                 AND role_id='custom_published' AND revision=4",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(retained, json!(["support.manage"]));
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires ERPX_TEST_ADMIN_URL for the isolated local PostgreSQL fixture"]
    async fn invite_writes_assignment_member_invitation_receipt_audit_and_outbox() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let actor = actor(&permissions());
            let reviewed = preview(&pool, &actor, invite(900_000)).await.unwrap();
            assert_eq!(
                reviewed["summary"],
                "Visitor → Published reader · department · 1970-01-01T00:15:00.000Z"
            );
            let token = reviewed["token"].as_str().unwrap().to_owned();
            let applied = apply(&pool, &actor, &token, "command-invite").await.unwrap();
            let invitation = applied["invitationId"].as_str().unwrap().to_owned();
            let assignment = applied["assignmentId"].as_str().unwrap().to_owned();
            assert_eq!(applied["id"], "subject-external");

            let subject = sqlx::query(
                "SELECT kind,user_id,status FROM iam_access_subjects \
                 WHERE company_id='company' AND id='subject-external'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(subject.get::<String, _>("kind"), "external");
            assert_eq!(
                subject.get::<String, _>("user_id"),
                format!("usr_invite_{invitation}")
            );
            assert_eq!(subject.get::<String, _>("status"), "invited");

            let grant = sqlx::query(
                "SELECT user_id,role,scope_kind,scope_node_id,valid_until,parent_id,source,\
                 modules_json FROM assignments WHERE id=$1",
            )
            .bind(&assignment)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(grant.get::<String, _>("role"), "custom_published");
            assert_eq!(grant.get::<String, _>("scope_kind"), "department");
            assert_eq!(grant.get::<String, _>("scope_node_id"), "department-1");
            assert_eq!(grant.get::<i64, _>("valid_until"), 900_000);
            assert_eq!(grant.get::<String, _>("parent_id"), "assignment-actor");
            assert_eq!(grant.get::<String, _>("source"), "manual");
            assert_eq!(grant.get::<String, _>("modules_json"), "[]");

            let member: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM iam_members WHERE company_id='company' \
                 AND user_id=(SELECT user_id FROM iam_access_subjects WHERE id='subject-external')",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(member, 1);
            let receipts: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM command_receipts WHERE command_id='command-invite' \
                 AND operation='people.access.apply'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(receipts, 1);
            let audits: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM iam_audit WHERE action='people.access.apply' \
                 AND target_id='command-invite'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(audits, 1);
            let outbox: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM outbox_events WHERE event_type='iam.people-access.invite' \
                 AND aggregate_id='subject-external'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(outbox, 1);

            // The committed review row is the Control-database delivery source.
            let pool_ref = pool.clone();
            let invitation_id = invitation.clone();
            let delivery =
                read_direct_invitation(&mut *pool_ref.raw().acquire().await.unwrap(), "h001", "company", &invitation_id)
                    .await
                    .unwrap();
            assert_eq!(
                delivery,
                DirectInvitationDelivery {
                    invitation_id,
                    holding_id: "h001".into(),
                    company_id: "company".into(),
                    subject_id: "subject-external".into(),
                    email: "visitor@example.invalid".into(),
                    name: "Visitor".into(),
                    role_id: "custom_published".into(),
                    valid_until: 900_000,
                    scope_kind: "department".into(),
                    branch_id: None,
                    department_id: Some("department-1".into()),
                    parent_id: "assignment-actor".into(),
                    authorizer_id: "actor".into(),
                    existing_user_id: None,
                }
            );
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires ERPX_TEST_ADMIN_URL for the isolated local PostgreSQL fixture"]
    async fn grant_and_revoke_enforce_cas_revision_and_self_scope() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let actor = actor(&permissions());
            sqlx::raw_sql(
                "UPDATE iam_access_subjects SET user_id='visitor',status='active' WHERE \
                 company_id='company' AND id='subject-external';\
                 INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) \
                 VALUES('company','visitor','local:visitor','active','local',1);",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let grant = reviewed(
                &pool,
                &actor,
                json!({
                    "operation": "grant",
                    "subjectId": "subject-external",
                    "roleId": "custom_published",
                    "scopeKind": "holding",
                    "scopeNodeId": null,
                    "validUntil": 900_000,
                    "reason": "Assign the published role",
                }),
                "command-grant",
            )
            .await;
            let assignment = grant["assignmentId"].as_str().unwrap().to_owned();
            assert_eq!(grant["id"], "subject-external");

            // The reviewed snapshot fingerprint changes when the role republishes.
            let stale = preview(
                &pool,
                &actor,
                json!({
                    "operation": "grant",
                    "subjectId": "subject-external",
                    "roleId": "custom_published",
                    "scopeKind": "holding",
                    "scopeNodeId": null,
                    "validUntil": 900_000,
                    "reason": "Assign again",
                }),
            )
            .await
            .unwrap();
            sqlx::raw_sql(
                "UPDATE iam_roles SET published_revision=4 WHERE id='custom_published'",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                business(
                    apply(
                        &pool,
                        &actor,
                        stale["token"].as_str().unwrap(),
                        "command-grant-stale"
                    )
                    .await
                    .unwrap_err()
                )
                .code,
                "PREVIEW_CHANGED"
            );
            sqlx::raw_sql(
                "UPDATE iam_roles SET published_revision=3 WHERE id='custom_published'",
            )
            .execute(pool.raw())
            .await
            .unwrap();

            // Revocation is CAS and refuses the actor's own assignment.
            assert_eq!(
                business(
                    preview(
                        &pool,
                        &actor,
                        json!({
                            "operation": "revoke",
                            "assignmentId": assignment,
                            "expectedVersion": 2,
                            "reason": "Withdraw access",
                        })
                    )
                    .await
                    .unwrap_err()
                )
                .code,
                "ASSIGNMENT_CHANGED"
            );
            let applied = reviewed(
                &pool,
                &actor,
                json!({
                    "operation": "revoke",
                    "assignmentId": assignment,
                    "expectedVersion": 1,
                    "reason": "Withdraw access",
                }),
                "command-revoke",
            )
            .await;
            assert_eq!(applied["id"], assignment);
            let row = sqlx::query("SELECT revoked_at,version FROM assignments WHERE id=$1")
                .bind(&assignment)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            assert_eq!(row.get::<i64, _>("revoked_at"), NOW);
            assert_eq!(row.get::<i64, _>("version"), 2);
            assert_eq!(
                business(
                    preview(
                        &pool,
                        &actor,
                        json!({
                            "operation": "revoke",
                            "assignmentId": "assignment-actor",
                            "expectedVersion": 1,
                            "reason": "Withdraw own access",
                        })
                    )
                    .await
                    .unwrap_err()
                )
                .code,
                "SELF_ACCESS_CHANGE"
            );
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires ERPX_TEST_ADMIN_URL for the isolated local PostgreSQL fixture"]
    async fn preview_rejects_undelegated_roles_expired_authority_and_unlinked_subjects() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let actor = actor(&permissions());
            // A permission outside the authorizing role's grantable ceiling.
            assert_eq!(
                business(
                    preview(
                        &pool,
                        &actor,
                        json!({
                            "operation": "role",
                            "expectedVersion": 0,
                            "name": "Wide role",
                            "permissions": ["support.create"],
                            "scopes": ["holding"],
                            "reason": "Exceed the ceiling",
                        })
                    )
                    .await
                    .unwrap_err()
                )
                .code,
                "PERMISSION_UNAVAILABLE"
            );
            // A grant before the subject is linked must be invited first.
            assert_eq!(
                business(
                    preview(
                        &pool,
                        &actor,
                        json!({
                            "operation": "grant",
                            "subjectId": "subject-external",
                            "roleId": "custom_published",
                            "scopeKind": "holding",
                            "scopeNodeId": null,
                            "validUntil": 900_000,
                            "reason": "Grant before linking",
                        })
                    )
                    .await
                    .unwrap_err()
                )
                .code,
                "INVITE_REQUIRED"
            );
            let mut earliest = invite(900_000);
            earliest["validUntil"] = json!(2_000_000_000);
            assert_eq!(
                business(preview(&pool, &actor, earliest).await.unwrap_err()).code,
                "EXPIRY_INVALID"
            );
            // A revoked authorizing assignment fails the live authority read.
            sqlx::raw_sql("UPDATE assignments SET revoked_at=1 WHERE id='assignment-actor'")
                .execute(pool.raw())
                .await
                .unwrap();
            assert_eq!(
                business(preview(&pool, &actor, invite(900_000)).await.unwrap_err()).code,
                "ACCESS_OBJECT_CHANGED"
            );
        })
        .await;
    }
}
