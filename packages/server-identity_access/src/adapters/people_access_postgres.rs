//! PostgreSQL adapter for the native People relationship slice.
//!
//! SQL is Identity-owned (`iam_external_organizations`, `iam_external_teams`,
//! `iam_access_subjects`, `iam_access_previews`, `iam_audit`) plus the shared
//! command receipt, audit and outbox tables. `organization_nodes`,
//! `hr_people`/`hr_engagements` and `iam_members` are read-only source facts;
//! no Organization or People owner row is mutated. The caller's serializable
//! Holding transaction is required and no commit is performed here.
use crate::application::people_access::{PeopleAccessFuture, PeopleRelationshipRepository};
use crate::domain::people_access::{
    OrganizationRow, PeopleAccessActor, RELATIONSHIP_APPLY_OPERATION, RelationshipApplyPlan,
    RelationshipChange, RelationshipPreviewPlan, RelationshipPreviewRow, RelationshipReceipt,
    RelationshipSnapshot, SubjectRow, TeamRow, UnitRow, idempotency_conflict, readback_invalid,
    relationship_apply_result,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgPeopleRelationships<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPeopleRelationships<'c> {
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
            .bind(format!("people-relationships-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn write_snapshot(
        &mut self,
        actor: &PeopleAccessActor,
    ) -> Result<RelationshipSnapshot, Error> {
        let company = actor.company_id.as_str();
        let organizations = sqlx::query(
            "SELECT id,name,kind,version::bigint AS version FROM iam_external_organizations \
             WHERE company_id=$1 ORDER BY name,id",
        )
        .bind(company)
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|row| OrganizationRow {
            id: row.get("id"),
            name: row.get("name"),
            kind: row.get("kind"),
            version: row.get("version"),
        })
        .collect();
        let teams = sqlx::query(
            "SELECT id,name,organization_id,department_id,version::bigint AS version \
             FROM iam_external_teams WHERE company_id=$1 ORDER BY name,id",
        )
        .bind(company)
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|row| TeamRow {
            id: row.get("id"),
            name: row.get("name"),
            organization_id: row.get("organization_id"),
            department_id: row.get("department_id"),
            version: row.get("version"),
        })
        .collect();
        let subjects = sqlx::query(
            "SELECT id,name,email,kind,user_id,person_id,organization_id,team_id,\
             sponsor_person_id,expires_at,status,version::bigint AS version \
             FROM iam_access_subjects WHERE company_id=$1 AND kind='external' ORDER BY name,id",
        )
        .bind(company)
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|row| SubjectRow {
            id: row.get("id"),
            name: row.get("name"),
            email: row.get("email"),
            kind: row.get("kind"),
            user_id: row.get("user_id"),
            person_id: row.get("person_id"),
            organization_id: row.get("organization_id"),
            team_id: row.get("team_id"),
            sponsor_person_id: row.get("sponsor_person_id"),
            expires_at: row.get("expires_at"),
            status: row.get("status"),
            version: row.get("version"),
        })
        .collect();
        let units = sqlx::query(
            "SELECT id,display_name AS name,kind,branch_id FROM organization_nodes \
             WHERE company_id=$1 AND status='active' AND kind IN ('branch','department') \
             ORDER BY display_name,id",
        )
        .bind(company)
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|row| UnitRow {
            id: row.get("id"),
            name: row.get("name"),
            kind: row.get("kind"),
            branch_id: row.get("branch_id"),
        })
        .collect();
        let authorizing_until: i64 = sqlx::query_scalar(
            "SELECT COALESCE((SELECT valid_until FROM assignments \
             WHERE id=$1 AND company_id=$2),0)",
        )
        .bind(actor.assignment_id.as_str())
        .bind(company)
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(RelationshipSnapshot {
            organizations,
            teams,
            subjects,
            units,
            authorizing_until,
        })
    }

    async fn upsert(&mut self, actor: &PeopleAccessActor, plan: &RelationshipApplyPlan) -> Result<(), Error> {
        let company = actor.company_id.as_str();
        match &plan.change {
            RelationshipChange::Organization { name, kind, .. } => {
                sqlx::query(
                    "INSERT INTO iam_external_organizations(id,company_id,name,kind) \
                     VALUES($1,$2,$3,$4) \
                     ON CONFLICT(id) DO UPDATE SET name=$3,kind=$4,\
                     version=iam_external_organizations.version+1",
                )
                .bind(&plan.id)
                .bind(company)
                .bind(name)
                .bind(kind)
                .execute(&mut *self.connection)
                .await?;
            }
            RelationshipChange::Team {
                name,
                organization_id,
                department_id,
                ..
            } => {
                sqlx::query(
                    "INSERT INTO iam_external_teams\
                     (id,company_id,organization_id,name,department_id) VALUES($1,$2,$3,$4,$5) \
                     ON CONFLICT(id) DO UPDATE SET organization_id=$3,name=$4,department_id=$5,\
                     version=iam_external_teams.version+1",
                )
                .bind(&plan.id)
                .bind(company)
                .bind(organization_id)
                .bind(name)
                .bind(department_id)
                .execute(&mut *self.connection)
                .await?;
            }
            RelationshipChange::Subject {
                name,
                email,
                organization_id,
                team_id,
                sponsor_person_id,
                expires_at,
                status,
                ..
            } => {
                sqlx::query(
                    "INSERT INTO iam_access_subjects\
                     (id,company_id,kind,name,email,organization_id,team_id,sponsor_person_id,\
                     expires_at,status,user_id,person_id) \
                     VALUES($1,$2,'external',$3,$4,$5,$6,$7,$8,$9,$10,$11) \
                     ON CONFLICT(id) DO UPDATE SET kind='external',name=$3,email=$4,\
                     organization_id=$5,team_id=$6,sponsor_person_id=$7,expires_at=$8,status=$9,\
                     version=iam_access_subjects.version+1",
                )
                .bind(&plan.id)
                .bind(company)
                .bind(name)
                .bind(email)
                .bind(organization_id)
                .bind(team_id)
                .bind(sponsor_person_id)
                .bind(expires_at)
                .bind(status)
                .bind(&plan.retained_user_id)
                .bind(&plan.retained_person_id)
                .execute(&mut *self.connection)
                .await?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn effects(
        &mut self,
        actor: &PeopleAccessActor,
        key: &str,
        request_hash: &str,
        plan: &RelationshipApplyPlan,
        result_json: &str,
        evidence: &Value,
        outbox: &Value,
    ) -> Result<(), Error> {
        let authority = actor.authority_json();
        let receipt = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,\
             authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(RELATIONSHIP_APPLY_OPERATION)
        .bind(actor.user_id.as_str())
        .bind(request_hash)
        .bind(&plan.id)
        .bind(result_json)
        .bind(plan.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if receipt != 1 {
            return Err(idempotency_conflict().into());
        }
        let mut full = evidence.clone();
        if let Some(object) = full.as_object_mut() {
            object.insert(
                "authority".into(),
                serde_json::from_str::<Value>(&authority).map_err(|_| readback_invalid())?,
            );
        }
        sqlx::query(
            "INSERT INTO iam_audit\
             (id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6::jsonb,$7)",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(RELATIONSHIP_APPLY_OPERATION)
        .bind(&plan.id)
        .bind(full.to_string())
        .bind(plan.now)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(RELATIONSHIP_APPLY_OPERATION)
        .bind(format!("iam.relationship.{}", plan.change.operation()))
        .bind(&plan.id)
        .bind(plan.resulting_version)
        .bind(plan.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        let mut payload = outbox.clone();
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "authority".into(),
                serde_json::from_str::<Value>(&authority).map_err(|_| readback_invalid())?,
            );
        }
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(key)
        .bind(format!("iam.relationship.{}", plan.change.operation()))
        .bind(format!("iam.{}", plan.change.operation()))
        .bind(&plan.id)
        .bind(plan.resulting_version)
        .bind(payload.to_string())
        .bind(plan.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl PeopleRelationshipRepository for PgPeopleRelationships<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        permission: &'a str,
        now: i64,
    ) -> PeopleAccessFuture<'a, bool, Self::Error> {
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
                 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
                 AND iam_role_allows(a.role,a.company_id,$5) AND m.status='active' \
                 AND (m.login_policy IS DISTINCT FROM 'emergency' OR ($6::bigint IS NOT NULL \
                  AND $6::bigint>$4::bigint)) \
                 AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions x WHERE x.session_id=$7 \
                  AND x.company_id=$3 AND x.user_id=$2))",
            )
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .bind(permission)
            .bind(actor.expires_at)
            .bind(actor.session_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> PeopleAccessFuture<'a, String, Self::Error> {
        Box::pin(async move {
            Ok(sqlx::query_scalar("SELECT encode(sha256($1::bytea),'hex')")
                .bind(value.as_bytes())
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn snapshot<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
    ) -> PeopleAccessFuture<'a, RelationshipSnapshot, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.write_snapshot(actor).await
        })
    }

    fn active_employee<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        person_id: &'a str,
    ) -> PeopleAccessFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM hr_people p \
                 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 WHERE p.company_id=$1 AND p.id=$2 AND p.merged_into_id IS NULL \
                 AND p.status!='archived' AND e.worker_type='employee' AND e.status='active')",
            )
            .bind(actor.company_id.as_str())
            .bind(person_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn same_person_subject<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        user_id: Option<&'a str>,
        person_id: Option<&'a str>,
    ) -> PeopleAccessFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM (\
                  SELECT $1::text AS account_id \
                  UNION SELECT m.user_id FROM iam_members m \
                   WHERE m.company_id=$2 AND $3::text IS NOT NULL AND m.person_id=$3::text\
                 ) linked WHERE linked.account_id IS NOT NULL \
                 AND iam_same_person(linked.account_id,$4,$2))",
            )
            .bind(user_id)
            .bind(actor.company_id.as_str())
            .bind(person_id)
            .bind(actor.user_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn protected_account<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        user_id: &'a str,
    ) -> PeopleAccessFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 \
                 AND login_policy='emergency')",
            )
            .bind(actor.company_id.as_str())
            .bind(user_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PeopleAccessFuture<'a, Option<RelationshipReceipt>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT request_hash,operation,actor_id,record_id,result_json,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| RelationshipReceipt {
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
        actor: &'a PeopleAccessActor,
        plan: &'a RelationshipPreviewPlan,
    ) -> PeopleAccessFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            if plan.company_id != actor.company_id.as_str()
                || plan.actor_id != actor.user_id.as_str()
                || plan.assignment_id != actor.assignment_id.as_str()
                || plan.session_id != actor.session_id.as_str()
            {
                return Err(readback_invalid().into());
            }
            Ok(sqlx::query_scalar(
                "INSERT INTO iam_access_previews\
                 (token,company_id,actor_id,assignment_id,session_id,change,fingerprint,\
                 expires_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5::jsonb,$6,$7) \
                 RETURNING token",
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
        actor: &'a PeopleAccessActor,
        token: &'a str,
        now: i64,
    ) -> PeopleAccessFuture<'a, Option<RelationshipPreviewRow>, Self::Error> {
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
            Ok(row.map(|row| RelationshipPreviewRow {
                change_json: row.get("change_json"),
                fingerprint: row.get("fingerprint"),
            }))
        })
    }

    fn consume_preview<'a>(
        &'a mut self,
        actor: &'a PeopleAccessActor,
        token: &'a str,
    ) -> PeopleAccessFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let consumed = sqlx::query(
                "DELETE FROM iam_access_previews WHERE token=$1 AND company_id=$2 \
                 AND actor_id=$3",
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
        actor: &'a PeopleAccessActor,
        key: &'a str,
        plan: &'a RelationshipApplyPlan,
        request_hash: &'a str,
    ) -> PeopleAccessFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            self.upsert(actor, plan).await?;
            let result = relationship_apply_result(&plan.id, plan.resulting_version, &plan.change)
                .map_err(|error| -> Error { error.into() })?;
            let mut evidence = json!({
                "operation": "people.relationships.apply",
                "relationship": plan.change.operation(),
                "expectedVersion": plan.expected_version,
                "resultVersion": plan.resulting_version,
                "reason": match &plan.change {
                    RelationshipChange::Organization { reason, .. }
                    | RelationshipChange::Team { reason, .. }
                    | RelationshipChange::Subject { reason, .. } => reason,
                },
            });
            let outbox = json!({
                "relationship": plan.change.operation(),
                "id": plan.id,
                "version": plan.resulting_version,
                "change": plan.change,
            });
            if let Some(object) = evidence.as_object_mut() {
                object.insert("outbox".into(), outbox.clone());
            }
            self.effects(actor, key, request_hash, plan, &result.to_string(), &evidence, &outbox)
                .await?;
            Ok(result)
        })
    }
}

/// Isolated PostgreSQL proof for the native People relationship producer.
///
/// Runs only when `ERPX_TEST_ADMIN_URL` points at the authorized local
/// PostgreSQL 18 test administrator, so the default `cargo test --lib` run
/// stays hermetic. The lead should relocate it to a registered
/// `tests/integration/identity_access/rust-people-relationships-postgres.test.rs`
/// suite once the catalogue/Cargo test target is added.
#[cfg(test)]
mod postgres_tests {
    use crate::application::people_access::{apply_relationship, preview_relationship};
    use crate::domain::people_access::PeopleAccessActor;
    use crate::people_fixture as fixture;
    use super::PgPeopleRelationships;
    use erpx_server_runtime::public::postgres::{
        DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
    };
    use erpx_server_shared::public::{
        AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
    };
    use serde_json::{Value, json};
    use sqlx::Row;

    const NOW: i64 = 5_000;

    fn actor(permissions: &[&str]) -> PeopleAccessActor {
        PeopleAccessActor {
            user_id: UserId::new("actor").unwrap(),
            session_id: RecordId::new("session-actor").unwrap(),
            holding_id: HoldingId::new("h001").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            assignment_id: AssignmentId::new("assignment-actor").unwrap(),
            scope_kind: "holding".into(),
            permissions: permissions
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
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
             ('*','it_admin','IT administrator',true,99,'active'),\
             ('*','hr_admin','HR administrator',true,98,'active') \
             ON CONFLICT(company_id,id) DO UPDATE SET published_revision=EXCLUDED.published_revision,\
             status='active';\
             INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,\
             scopes_json,created_by,created_at) VALUES \
             ('*','it_admin',99,'[\"iam.read\",\"iam.manage\",\"people.relationships.read\",\
             \"people.relationships.manage\"]','[]','[\"holding\"]','system',0),\
             ('*','hr_admin',98,'[\"people.relationships.read\",\"people.relationships.manage\"]',\
             '[]','[\"holding\"]','system',0) \
             ON CONFLICT(company_id,role_id,revision) DO NOTHING;\
             INSERT INTO organization_nodes(id,company_id,parent_id,branch_id,kind,display_name,\
             slug,status,hierarchy_revision) VALUES \
             ('company','company',NULL,NULL,'company','Acme','acme','active',1),\
             ('branch-1','company','company','branch-1','branch','HQ','hq','active',1),\
             ('department-1','company','branch-1','branch-1','department','Operations',\
             'operations','active',1);\
             INSERT INTO hr_people(id,company_id,legal_name,country_code,status,version,created_by,\
             created_at,updated_at) VALUES \
             ('person-sponsor','company','Ada Sponsor','GB','active',1,'system',0,0);\
             INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,\
             start_date,job_title,currency,version,created_by,created_at,updated_at) VALUES \
             ('engagement-sponsor','person-sponsor','company','employee','salaried','active',\
             '2020-01-01','Operations Lead','GBP',1,'system',0,0);\
             INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,\
             version) VALUES \
             ('company','actor','local:actor','person-actor','active','local',1),\
             ('company','other','local:other','person-other','active','local',1);\
             INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,\
             valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) \
             VALUES('assignment-actor','actor','it_admin','[]','company',NULL,0,1000000000,NULL,1,\
             'holding',NULL,NULL,'manual',NULL);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
    }

    async fn preview(
        pool: &DatabasePool,
        actor: &PeopleAccessActor,
        change: Value,
    ) -> Result<Value, PgTransactionError<BusinessError>> {
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let result = {
            let mut repository = PgPeopleRelationships::new(&mut tx);
            preview_relationship(&mut repository, actor, change, NOW).await
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
        actor: &PeopleAccessActor,
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
            let mut repository = PgPeopleRelationships::new(&mut tx);
            apply_relationship(&mut repository, actor, &command, token, NOW).await
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

    fn organization_change(name: &str, expected_version: i64) -> Value {
        json!({
            "operation": "organization",
            "expectedVersion": expected_version,
            "name": name,
            "kind": "independent",
            "reason": "Record the contractor relationship",
        })
    }

    #[tokio::test]
    #[ignore = "requires ERPX_TEST_ADMIN_URL for the isolated local PostgreSQL fixture"]
    async fn preview_then_apply_writes_relationship_receipt_audit_and_outbox() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let actor = actor(&[
                "iam.read",
                "people.relationships.read",
                "people.relationships.manage",
            ]);
            let reviewed = preview(&pool, &actor, organization_change("Partner", 0))
                .await
                .unwrap();
            let token = reviewed["token"].as_str().unwrap().to_owned();
            assert_eq!(reviewed["summary"], "organization: Partner");

            let applied = apply(&pool, &actor, &token, "command-1").await.unwrap();
            assert_eq!(applied["version"], 1);
            let id = applied["id"].as_str().unwrap().to_owned();
            assert!(id.starts_with("iam-organization-"));

            let row = sqlx::query(
                "SELECT company_id,name,kind,version FROM iam_external_organizations WHERE id=$1",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("company_id"), "company");
            assert_eq!(row.get::<String, _>("name"), "Partner");
            assert_eq!(row.get::<String, _>("kind"), "independent");
            assert_eq!(row.get::<i32, _>("version"), 1);

            let receipts: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM command_receipts WHERE command_id='command-1' \
                 AND operation='people.relationships.apply'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(receipts, 1);
            let audits: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM iam_audit WHERE action='people.relationships.apply' \
                 AND target_id=$1",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(audits, 1);
            let shared: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM audit_events WHERE record_id=$1 AND action=\
                 'people.relationships.apply'",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(shared, 1);
            let outbox: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM outbox_events WHERE aggregate_id=$1 AND event_type=\
                 'iam.relationship.organization'",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(outbox, 1);
            let previews: i64 =
                sqlx::query_scalar("SELECT count(*) FROM iam_access_previews WHERE token=$1")
                    .bind(&token)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(previews, 0, "the reviewed token is single use");

            // Same key replays the recorded receipt.
            assert_eq!(
                apply(&pool, &actor, &token, "command-1").await.unwrap(),
                applied
            );
            // A different key cannot reuse the consumed preview.
            assert_eq!(
                business(apply(&pool, &actor, &token, "command-2").await.unwrap_err()).code,
                "PREVIEW_EXPIRED"
            );
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires ERPX_TEST_ADMIN_URL for the isolated local PostgreSQL fixture"]
    async fn preview_rejects_stale_versions_privilege_changes_and_unrelated_authority() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let actor = actor(&[
                "iam.read",
                "people.relationships.read",
                "people.relationships.manage",
            ]);
            let reviewed = preview(&pool, &actor, organization_change("Partner", 0))
                .await
                .unwrap();
            let token = reviewed["token"].as_str().unwrap().to_owned();
            let applied = apply(&pool, &actor, &token, "command-1").await.unwrap();
            let id = applied["id"].as_str().unwrap().to_owned();

            // The reviewed snapshot no longer matches after the write.
            let stale = preview(&pool, &actor, organization_change("Partner", 0))
                .await
                .unwrap();
            let stale_token = stale["token"].as_str().unwrap().to_owned();
            sqlx::raw_sql("UPDATE iam_external_organizations SET name='Changed elsewhere'")
                .execute(pool.raw())
                .await
                .unwrap();
            assert_eq!(
                business(apply(&pool, &actor, &stale_token, "command-2").await.unwrap_err()).code,
                "PREVIEW_CHANGED"
            );

            // Optimistic version is checked against the reviewed row.
            let mut stale_version = organization_change("Partner", 0);
            stale_version["id"] = json!(id);
            assert_eq!(
                business(
                    preview(&pool, &actor, stale_version)
                        .await
                        .unwrap_err()
                )
                .code,
                "ACCESS_OBJECT_CHANGED"
            );
            let mut current_version = organization_change("Partner", 1);
            current_version["id"] = json!(id);
            assert!(
                preview(&pool, &actor, current_version).await.is_ok(),
                "the current version must remain previewable"
            );

            // Privilege operations never reach the relationship producer.
            assert_eq!(
                business(
                    preview(
                        &pool,
                        &actor,
                        json!({
                            "operation": "grant",
                            "subjectId": "subject-1",
                            "roleId": "it_admin",
                            "scopeKind": "holding",
                            "scopeNodeId": null,
                            "validUntil": 9_000,
                            "reason": "Escalate through relationships",
                        })
                    )
                    .await
                    .unwrap_err()
                )
                .code,
                "PEOPLE_ACCESS_REQUEST_INVALID"
            );

            // The serializable authority read rechecks the live assignment:
            // a revoked authorizing assignment fails even when the passed
            // actor facts still carry the permission.
            sqlx::raw_sql("UPDATE assignments SET revoked_at=1 WHERE id='assignment-actor'")
                .execute(pool.raw())
                .await
                .unwrap();
            assert_eq!(
                business(
                    preview(&pool, &actor, organization_change("Denied", 0))
                        .await
                        .unwrap_err()
                )
                .code,
                "ACCESS_OBJECT_CHANGED"
            );
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires ERPX_TEST_ADMIN_URL for the isolated local PostgreSQL fixture"]
    async fn subject_relationships_require_a_team_in_scope_and_an_active_sponsor() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let actor = actor(&[
                "iam.read",
                "people.relationships.read",
                "people.relationships.manage",
            ]);
            let organization = preview(&pool, &actor, organization_change("Partner", 0))
                .await
                .unwrap();
            let organization_token = organization["token"].as_str().unwrap().to_owned();
            let organization = apply(&pool, &actor, &organization_token, "command-org")
                .await
                .unwrap();
            let organization_id = organization["id"].as_str().unwrap().to_owned();

            let subject = json!({
                "operation": "subject",
                "expectedVersion": 0,
                "name": "Contractor One",
                "email": "contractor@example.invalid",
                "organizationId": organization_id,
                "teamId": null,
                "sponsorPersonId": "person-sponsor",
                "expiresAt": 9_000_000,
                "status": "active",
                "reason": "Record the contractor relationship",
            });
            let reviewed = preview(&pool, &actor, subject.clone()).await.unwrap();
            let token = reviewed["token"].as_str().unwrap().to_owned();
            let applied = apply(&pool, &actor, &token, "command-subject")
                .await
                .unwrap();
            let id = applied["id"].as_str().unwrap().to_owned();
            let row = sqlx::query(
                "SELECT kind,status,user_id,person_id,expires_at,version FROM iam_access_subjects \
                 WHERE id=$1 AND company_id='company'",
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row.get::<String, _>("kind"), "external");
            assert_eq!(row.get::<String, _>("status"), "active");
            assert_eq!(row.get::<i64, _>("expires_at"), 9_000_000);
            assert_eq!(row.get::<i32, _>("version"), 1);

            let mut unknown_sponsor = subject.clone();
            unknown_sponsor["sponsorPersonId"] = json!("person-missing");
            assert_eq!(
                business(preview(&pool, &actor, unknown_sponsor).await.unwrap_err()).code,
                "SPONSOR_UNAVAILABLE"
            );

            let mut self_change = subject.clone();
            self_change["id"] = json!(id);
            self_change["expectedVersion"] = json!(1);
            sqlx::query("UPDATE iam_access_subjects SET user_id='actor' WHERE id=$1")
                .bind(&id)
                .execute(pool.raw())
                .await
                .unwrap();
            assert_eq!(
                business(preview(&pool, &actor, self_change).await.unwrap_err()).code,
                "SELF_ACCESS_CHANGE"
            );
        })
        .await;
    }
}
