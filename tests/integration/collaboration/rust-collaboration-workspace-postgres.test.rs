//! PostgreSQL proof for the legacy Collaboration workspace read: company,
//! branch and source-port visibility filtering, eligible people, authority flags
//! and the retained source witnesses. Skipped when `ERPX_TEST_ADMIN_URL` is not
//! configured.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::collaboration_work::{
    CollaborationScope, CollaborationSourcePort, SourcePresentation, WORKSPACE_SOURCE_TYPES,
    read_scope_workspace,
};
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

type Error = PgTransactionError<BusinessError>;

fn configured() -> bool {
    std::env::var("ERPX_TEST_ADMIN_URL").is_ok()
}

async fn setup(pool: &erpx_server_runtime::public::postgres::DatabasePool, now: i64) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    for (role, permissions) in [
        (
            "collaboration_workspace",
            "[\"records.read\",\"records.write\",\"records.review\"]",
        ),
        (
            "collaboration_workspace_reader",
            "[\"records.read\",\"records.write\"]",
        ),
        ("collaboration_workspace_viewer", "[\"records.read\"]"),
    ] {
        sqlx::query(
            "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
             VALUES('*',$1,$1,true,1,'active')",
        )
        .bind(role)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
             VALUES('*',$1,1,$2::jsonb,'[]','[\"holding\",\"branch\",\"department\"]','fixture',0)",
        )
        .bind(role)
        .bind(permissions)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    for (assignment, user, role, branch, name) in [
        (
            "workspace-grant-owner",
            "owner",
            "collaboration_workspace",
            "branch",
            "Alice Owner",
        ),
        (
            "workspace-grant-reader",
            "reader",
            "collaboration_workspace_reader",
            "branch",
            "Bob Reader",
        ),
        (
            "workspace-grant-viewer",
            "viewer",
            "collaboration_workspace_viewer",
            "branch",
            "Zoe Viewer",
        ),
        (
            "workspace-grant-outsider",
            "outsider",
            "collaboration_workspace_viewer",
            "other-branch",
            "Oscar Outsider",
        ),
    ] {
        sqlx::query(
            "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) \
             VALUES($1,$2,$3,'[\"collaboration\"]','company',$4,$5,$6,1,'branch',$4)",
        )
        .bind(assignment)
        .bind(user)
        .bind(role)
        .bind(branch)
        .bind(now - 60_000)
        .bind(now + 3_600_000)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) \
             VALUES($1,$2,'company',$3,'TH','active','fixture',$4,$4)",
        )
        .bind(format!("person-{user}"))
        .bind(user)
        .bind(name)
        .bind(now)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) \
             VALUES($1,$2,'company','branch','employee','salaried','active','2025-01-01','Member','THB','fixture',$3,$3)",
        )
        .bind(format!("engagement-{user}"))
        .bind(format!("person-{user}"))
        .bind(now)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    // A person with no assignment is never eligible.
    sqlx::query(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) \
         VALUES('person-no-assignment',NULL,'company','No Assignment','TH','active','fixture',$1,$1)",
    )
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();

    for (id, company, branch, source, source_id, target, target_id, status, at) in [
        (
            "relation-visible",
            "company",
            "branch",
            "support_ticket",
            "ticket-visible",
            "staff_task",
            "task-visible",
            "active",
            now + 40,
        ),
        (
            "relation-missing",
            "company",
            "branch",
            "support_ticket",
            "ticket-visible",
            "support_ticket",
            "missing",
            "active",
            now + 30,
        ),
        (
            "relation-forbidden",
            "company",
            "branch",
            "support_ticket",
            "ticket-visible",
            "support_ticket",
            "forbidden",
            "active",
            now + 20,
        ),
        (
            "relation-foreign",
            "company",
            "branch",
            "support_ticket",
            "ticket-visible",
            "support_ticket",
            "foreign",
            "active",
            now + 10,
        ),
        (
            "relation-field",
            "company",
            "branch",
            "field_service_order",
            "field-1",
            "support_ticket",
            "ticket-visible",
            "active",
            now + 50,
        ),
        (
            "relation-removed",
            "company",
            "branch",
            "support_ticket",
            "ticket-visible",
            "staff_task",
            "task-removed",
            "removed",
            now + 60,
        ),
        (
            "relation-other-company",
            "other-company",
            "branch",
            "support_ticket",
            "ticket-visible",
            "staff_task",
            "task-visible",
            "active",
            now + 70,
        ),
    ] {
        sqlx::query(
            "INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,status,created_by,created_at,updated_at) \
             VALUES($1,$2,$3,NULL,$4,$5,3,$6,$7,3,'related','Fixture relation',$8,'owner',$9,$9)",
        )
        .bind(id)
        .bind(company)
        .bind(branch)
        .bind(source)
        .bind(source_id)
        .bind(target)
        .bind(target_id)
        .bind(status)
        .bind(at)
        .execute(pool.raw())
        .await
        .unwrap();
    }

    for (id, branch, source_type, source_id, owner, at) in [
        (
            "card-visible",
            "branch",
            "support_ticket",
            "ticket-visible",
            "owner",
            now + 40,
        ),
        (
            "card-other-branch",
            "other-branch",
            "support_ticket",
            "ticket-visible",
            "owner",
            now + 30,
        ),
        (
            "card-missing",
            "branch",
            "support_ticket",
            "missing",
            "owner",
            now + 20,
        ),
        (
            "card-forbidden",
            "branch",
            "support_ticket",
            "forbidden",
            "owner",
            now + 10,
        ),
        (
            "card-foreign",
            "branch",
            "support_ticket",
            "foreign",
            "owner",
            now + 5,
        ),
        (
            "card-field",
            "branch",
            "field_service_order",
            "field-1",
            "owner",
            now + 50,
        ),
    ] {
        sqlx::query(
            "INSERT INTO collaboration_action_cards(id,company_id,branch_id,department_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) \
             VALUES($1,'company',$2,NULL,$3,$4,3,$1,$5,$6,'owner',$7,$7)",
        )
        .bind(id)
        .bind(branch)
        .bind(source_type)
        .bind(source_id)
        .bind(owner)
        .bind(now + 60_000)
        .bind(at)
        .execute(pool.raw())
        .await
        .unwrap();
    }
}

#[derive(Default)]
struct Sources {
    prepare_calls: Vec<(SourceKind, String)>,
    require_calls: usize,
    drift: bool,
}

impl CollaborationSourcePort for Sources {
    type Error = Error;
    type Witness = String;

    async fn prepare(
        &mut self,
        reference: &SourceReference,
    ) -> Result<(SourceRecord, Self::Witness), Error> {
        self.prepare_calls
            .push((reference.kind, reference.id.clone()));
        match reference.id.as_str() {
            "missing" => Err(BusinessError::new(
                FailureKind::NotFound,
                "SOURCE_RECORD_NOT_FOUND",
                "Source record is unavailable in this work scope.",
            )
            .into()),
            "forbidden" => Err(BusinessError::new(
                FailureKind::Forbidden,
                "SOURCE_RECORD_FORBIDDEN",
                "The source is not visible in this work scope.",
            )
            .into()),
            id => Ok((
                SourceRecord {
                    kind: reference.kind,
                    id: id.into(),
                    company_id: if id == "foreign" {
                        "other-company".into()
                    } else {
                        "company".into()
                    },
                    branch_id: Some("branch".into()),
                    department_id: None,
                    version: 3,
                    title: format!("Title {id}"),
                    status: "open".into(),
                },
                format!("witness:{id}"),
            )),
        }
    }

    async fn require_current(&mut self, witness: &Self::Witness) -> Result<(), Error> {
        self.require_calls += 1;
        if self.drift {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COLLABORATION_SOURCE_CHANGED",
                "The collaboration source changed.",
            )
            .into());
        }
        assert!(witness.starts_with("witness:"));
        Ok(())
    }

    async fn present(&mut self, reference: &SourceReference) -> Result<SourcePresentation, Error> {
        Ok(SourcePresentation {
            title: None,
            href: Some(format!(
                "/records/{}/{}",
                reference.kind.as_str(),
                reference.id
            )),
        })
    }
}

#[tokio::test]
async fn pg_workspace_hides_unresolved_rows_and_retains_source_witnesses() {
    if !configured() {
        eprintln!(
            "skipping collaboration_workspace_postgres: ERPX_TEST_ADMIN_URL is not configured"
        );
        return;
    }
    fixture::with_database(2, |pool| async move {
        let now: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        setup(&pool, now).await;
        let scope = CollaborationScope::new("company", "owner", "workspace-grant-owner", now)
            .unwrap()
            .branch(Some("branch"));
        let (workspace, mut sources) = {
            let pool = pool.clone();
            let scope = scope.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let scope = scope.clone();
                    Box::pin(async move {
                        let mut sources = Sources::default();
                        let workspace = read_scope_workspace(c, scope, &mut sources).await?;
                        Ok::<_, Error>((workspace, sources))
                    })
                })
                .await
                .unwrap()
        };

        let relation_ids: Vec<&str> = workspace
            .relations
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert_eq!(relation_ids, ["relation-visible"]);

        let card_ids: Vec<&str> = workspace
            .cards
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert_eq!(card_ids, ["card-visible"]);
        let card = &workspace.cards[0];
        assert_eq!(card["source_title"], "Title ticket-visible");
        assert_eq!(
            card["source_href"],
            "/records/support_ticket/ticket-visible"
        );
        assert_eq!(card["owner_eligible"], 1);
        assert_eq!(card["owner_name"], "Alice Owner");

        let people: Vec<&str> = workspace
            .people
            .iter()
            .map(|person| person["id"].as_str().unwrap())
            .collect();
        assert_eq!(people, ["owner", "reader"]);
        assert_eq!(workspace.people[0]["name"], "Alice Owner");

        assert_eq!(workspace.source_types, WORKSPACE_SOURCE_TYPES);
        assert!(workspace.can_write);
        assert!(workspace.can_review);

        // relation-visible resolves both sides and card-visible resolves one.
        assert_eq!(workspace.witness_count(), 3);
        assert!(
            sources
                .prepare_calls
                .iter()
                .any(|(_, id)| id.as_str() == "task-visible")
        );
        for hidden in ["missing", "forbidden", "foreign"] {
            assert!(
                sources
                    .prepare_calls
                    .iter()
                    .any(|(_, id)| id.as_str() == hidden),
                "the unresolved source {hidden} was never inspected"
            );
        }
        assert!(
            !sources
                .prepare_calls
                .iter()
                .any(|(_, id)| id.as_str() == "field-1"),
            "field service sources belong to the native Field surface"
        );
        workspace.require_current(&mut sources).await.unwrap();
        assert_eq!(sources.require_calls, 3);

        let mut drift = Sources {
            drift: true,
            ..Default::default()
        };
        assert!(workspace.require_current(&mut drift).await.is_err());

        // The read-only viewer sees the same rows but no write/review actions.
        let viewer = CollaborationScope::new("company", "viewer", "workspace-grant-viewer", now)
            .unwrap()
            .branch(Some("branch"));
        let viewer_workspace = {
            let pool = pool.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let viewer = viewer.clone();
                    Box::pin(async move {
                        let mut sources = Sources::default();
                        read_scope_workspace(c, viewer, &mut sources).await
                    })
                })
                .await
                .unwrap()
        };
        assert!(!viewer_workspace.can_write);
        assert!(!viewer_workspace.can_review);
        assert_eq!(viewer_workspace.cards.len(), 1);

        // Branch selection hides the other branch card while company-wide
        // relations stay visible.
        let other = CollaborationScope::new("company", "owner", "workspace-grant-owner", now)
            .unwrap()
            .branch(Some("other-branch"));
        let other_workspace = {
            let pool = pool.clone();
            PgUnitOfWork::new(pool)
                .run(TransactionOptions::default(), move |c| {
                    let other = other.clone();
                    Box::pin(async move {
                        let mut sources = Sources::default();
                        read_scope_workspace(c, other, &mut sources).await
                    })
                })
                .await
                .unwrap()
        };
        let other_cards: Vec<&str> = other_workspace
            .cards
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert_eq!(other_cards, ["card-other-branch"]);
        assert_eq!(other_workspace.relations.len(), 1);
    })
    .await;
}
