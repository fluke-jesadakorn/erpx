//! Same-transaction live-vs-native parity harness for the Records
//! `record_context` read port (`G4 / migrate.records.context-read-port`).
//!
//! For each caller-unit read, the exact retained TypeScript statement (copied
//! verbatim from `packages/server-records/src/adapters/records.ts`) and the
//! owner's registered Postgres adapter are executed on ONE freshly opened
//! serializable transaction over the same fixture rows. The projected rows must
//! match, including an uncommitted write made on that transaction. Nothing
//! commits and no caller is switched by this test: the TypeScript authority is
//! deliberately retained until `retire.records.read-tail-r13`.
//!
//! The native adapter issues one extra row (`LIMIT n+1`) on bounded families so
//! strict row validation can fail closed on overflow; within the bounded fixture
//! that does not change the result set, which is what this harness asserts.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_records::public::record_context::{
    BacklinkEdge, HistoryRow, MentionRow, PgRecordContext, RecordContextActor,
    RecordContextRepository, RecordRow, ReferenceEdge, SuggestionRow,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use sqlx::{AssertSqlSafe, Connection, Row, postgres::PgRow};

/// Live `readGetRecordRow` (holding scope, no actor cross-owner predicates) and
/// the five context sub-reads from `packages/server-records/src/adapters/records.ts`
/// (repository methods `readGetRecordRow`, `readRecordContext*2..5` at lines
/// 78-111). Kept verbatim; `?` is converted to `$n` only for direct execution.
const LIVE_RECORD: &str = "SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND id=? AND company_id=?";
const LIVE_EDGES: &str = "SELECT target_module,target_id,kind,ordinal FROM record_reference_edges WHERE host_module=? AND host_id=? ORDER BY ordinal";
const LIVE_BACKLINKS: &str = "SELECT host_module,host_id,kind FROM record_reference_edges WHERE target_module=? AND target_id=? ORDER BY created_at DESC LIMIT 50";
const LIVE_MENTIONS: &str = "SELECT target_kind,target_id,target_label,created_at FROM record_principal_mentions WHERE host_module=? AND host_id=? ORDER BY created_at";
const LIVE_SUGGESTIONS: &str = "SELECT id,field_key,proposed_value_json,source_versions_json,provenance_json,state,created_at FROM record_field_suggestions WHERE module_id=? AND record_id=? ORDER BY created_at DESC LIMIT 20";
const LIVE_HISTORY: &str = "SELECT from_status,to_status,action,note,actor_id,record_version,created_at FROM workspace_record_transitions WHERE module_id=? AND record_id=? ORDER BY created_at DESC LIMIT 20";

const NOW: i64 = 500;

fn pg(sql: &str) -> String {
    let mut out = String::new();
    let mut chunks = sql.split('?');
    if let Some(first) = chunks.next() {
        out.push_str(first);
        for (index, chunk) in chunks.enumerate() {
            out.push_str(&format!("${}", index + 1));
            out.push_str(chunk);
        }
    }
    out
}

/// Every numeric column in these owner tables is BIGINT; decode it as i64 and
/// render text, otherwise decode the text column. This keeps the TS and native
/// projections byte-comparable without altering the verbatim statements.
fn cell(row: &PgRow, index: usize) -> Option<String> {
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value.map(|value| value.to_string());
    }
    row.get::<Option<String>, _>(index)
}

async fn live(conn: &mut sqlx::PgConnection, sql: &str, values: &[&str]) -> Vec<Vec<Option<String>>> {
    let mut query = sqlx::query(AssertSqlSafe(pg(sql)));
    for value in values {
        query = query.bind(*value);
    }
    query
        .fetch_all(&mut *conn)
        .await
        .unwrap()
        .iter()
        .map(|row| (0..row.columns().len()).map(|index| cell(row, index)).collect())
        .collect()
}

fn native_row(row: &RecordRow) -> Vec<Option<String>> {
    vec![
        Some(row.module_id.clone()),
        Some(row.id.clone()),
        Some(row.section_id.clone()),
        Some(row.title.clone()),
        Some(row.description.clone()),
        Some(row.status.clone()),
        Some(row.fields_json.clone()),
        Some(row.version.to_string()),
        Some(row.created_by.clone()),
        Some(row.updated_at.to_string()),
    ]
}

fn native_edges(edges: Vec<ReferenceEdge>) -> Vec<Vec<Option<String>>> {
    edges
        .into_iter()
        .map(|edge| {
            vec![
                Some(edge.target_module),
                Some(edge.target_id),
                Some(edge.kind),
                Some(edge.ordinal.to_string()),
            ]
        })
        .collect()
}

fn native_backlinks(edges: Vec<BacklinkEdge>) -> Vec<Vec<Option<String>>> {
    edges
        .into_iter()
        .map(|edge| {
            vec![
                Some(edge.host_module),
                Some(edge.host_id),
                Some(edge.kind),
            ]
        })
        .collect()
}

fn native_mentions(rows: Vec<MentionRow>) -> Vec<Vec<Option<String>>> {
    rows.into_iter()
        .map(|row| {
            vec![
                Some(row.target_kind),
                Some(row.target_id),
                Some(row.target_label),
                Some(row.created_at.to_string()),
            ]
        })
        .collect()
}

fn native_suggestions(rows: Vec<SuggestionRow>) -> Vec<Vec<Option<String>>> {
    rows.into_iter()
        .map(|row| {
            vec![
                Some(row.id),
                Some(row.field_key),
                Some(row.proposed_value_json),
                Some(row.source_versions_json),
                Some(row.provenance_json),
                Some(row.state),
                Some(row.created_at.to_string()),
            ]
        })
        .collect()
}

fn native_history(rows: Vec<HistoryRow>) -> Vec<Vec<Option<String>>> {
    rows.into_iter()
        .map(|row| {
            vec![
                Some(row.from_status),
                Some(row.to_status),
                Some(row.action),
                Some(row.note),
                Some(row.actor_id),
                Some(row.record_version.to_string()),
                Some(row.created_at.to_string()),
            ]
        })
        .collect()
}

fn actor() -> RecordContextActor {
    RecordContextActor {
        user_id: "actor".into(),
        holding_id: "h001".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment-actor".into(),
        assignment_version: Some(2),
        authority_revision: Some(9),
        role: "records_reader".into(),
        scope_kind: "holding".into(),
        modules: vec!["operations".into(), "crm".into()],
        permissions: vec!["records.read".into()],
        expires_at: 1_000_000,
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','records_reader','Records reader',false,1,'active') \
         ON CONFLICT(company_id,id) DO UPDATE SET published_revision=excluded.published_revision,status='active';\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','records_reader',1,'[\"records.read\"]','[]','[\"holding\"]','system',0) \
         ON CONFLICT(company_id,role_id,revision) DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES \
         ('company','actor','local:actor',NULL,'active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) VALUES \
         ('assignment-actor','actor','records_reader','[\"operations\",\"crm\"]','company',NULL,0,1000000,NULL,2,'holding',NULL,NULL,'manual',NULL);\
         INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at,archived_at) VALUES \
         ('operations','rec-1','overview','Service visit','Bounded record','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"first\"}]','company',NULL,NULL,3,'maker','maker',100,500,NULL),\
         ('operations','rec-2','overview','Target record','Link target','approved','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"target\"}]','company',NULL,NULL,2,'maker','maker',100,480,NULL),\
         ('operations','rec-3','overview','Backlink host','Backlink source','in_review','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"host\"}]','company',NULL,NULL,3,'maker','maker',100,470,NULL),\
         ('crm','party-1','overview','Foreign owner party','CRM party','draft','[{\"key\":\"name\",\"label\":\"Name\",\"type\":\"text\",\"value\":\"Party\"}]','company',NULL,NULL,1,'maker','maker',100,440,NULL),\
         ('operations','rec-foreign','overview','Foreign record','Foreign company','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"foreign\"}]','foreign',NULL,NULL,1,'maker','maker',100,450,NULL);\
         INSERT INTO record_reference_edges(host_module,host_id,target_module,target_id,kind,ordinal,created_by,created_at) VALUES \
         ('operations','rec-1','operations','rec-2','dependsOn',0,'maker',400),\
         ('operations','rec-1','crm','party-1','authoredWith',1,'maker',410),\
         ('operations','rec-1','operations','rec-foreign','billedBy',2,'maker',420),\
         ('operations','rec-3','operations','rec-1','blocks',0,'maker',430);\
         INSERT INTO record_principal_mentions(host_module,host_id,target_kind,target_id,target_label,created_by,created_at) VALUES \
         ('operations','rec-1','person','user-1','Person One','maker',400),\
         ('operations','rec-1','group','team-1','Team One','maker',410);\
         INSERT INTO record_field_suggestions(id,module_id,record_id,field_key,proposed_value_json,previous_value_digest,source_versions_json,provenance_json,state,created_by,applied_by,created_at,updated_at) VALUES \
         ('suggestion-1','operations','rec-1','note','\"next\"','digest','[{\"record\":\"operations/rec-2\",\"version\":2}]','{\"provider_kind\":\"development\"}','ready','maker',NULL,420,420);\
         INSERT INTO workspace_record_transitions(id,module_id,record_id,from_status,to_status,action,note,actor_id,assignment_id,record_version,created_at) VALUES \
         ('transition-1','operations','rec-1','draft','in_review','submit','Submitted','maker','assignment-maker',2,460),\
         ('transition-2','operations','rec-1','in_review','approved','approve','Approved','reviewer','assignment-reviewer',3,470);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn live_context_sql_matches_native_owner_adapter_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping Records context parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();

        // An uncommitted owner write on this unit must be visible to both sides.
        sqlx::raw_sql(
            "INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at,archived_at) VALUES ('operations','rec-live','overview','Live target','Uncommitted','draft','[{\"key\":\"note\",\"label\":\"Note\",\"type\":\"text\",\"value\":\"live\"}]','company',NULL,NULL,1,'maker','maker',100,490,NULL);\
             INSERT INTO record_reference_edges(host_module,host_id,target_module,target_id,kind,ordinal,created_by,created_at) VALUES ('operations','rec-1','operations','rec-live','fulfills',3,'maker',490)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        let who = actor();
        let module = "operations";
        let id = "rec-1";

        // The host record row.
        let ts = live(&mut tx, LIVE_RECORD, &[module, id, who.company_id.as_str()]).await;
        let host = {
            let mut repository = PgRecordContext::new(&mut tx);
            repository
                .record(&who, module, id)
                .await
                .unwrap()
                .expect("host record must exist")
        };
        let native = vec![native_row(&host)];
        assert!(!ts.is_empty(), "LIVE_RECORD fixture must return a row");
        assert_eq!(ts, native, "record: live SQL and native adapter rows");

        // Outgoing references (includes the uncommitted edge).
        let ts = live(&mut tx, LIVE_EDGES, &[module, id]).await;
        let native = native_edges({
            let mut repository = PgRecordContext::new(&mut tx);
            repository.references(module, id).await.unwrap()
        });
        assert_eq!(ts.len(), 4, "LIVE_EDGES fixture must return four rows");
        assert_eq!(ts, native, "references: live SQL and native adapter rows");

        // Visible backlinks.
        let ts = live(&mut tx, LIVE_BACKLINKS, &[module, id]).await;
        let native = native_backlinks({
            let mut repository = PgRecordContext::new(&mut tx);
            repository.backlinks(module, id).await.unwrap()
        });
        assert_eq!(ts, native, "backlinks: live SQL and native adapter rows");

        // Principal mentions.
        let ts = live(&mut tx, LIVE_MENTIONS, &[module, id]).await;
        let native = native_mentions({
            let mut repository = PgRecordContext::new(&mut tx);
            repository.mentions(module, id).await.unwrap()
        });
        assert!(!ts.is_empty(), "LIVE_MENTIONS fixture must return rows");
        assert_eq!(ts, native, "mentions: live SQL and native adapter rows");

        // Field suggestions.
        let ts = live(&mut tx, LIVE_SUGGESTIONS, &[module, id]).await;
        let native = native_suggestions({
            let mut repository = PgRecordContext::new(&mut tx);
            repository.suggestions(module, id).await.unwrap()
        });
        assert!(!ts.is_empty(), "LIVE_SUGGESTIONS fixture must return rows");
        assert_eq!(ts, native, "suggestions: live SQL and native adapter rows");

        // Transition timeline.
        let ts = live(&mut tx, LIVE_HISTORY, &[module, id]).await;
        let native = native_history({
            let mut repository = PgRecordContext::new(&mut tx);
            repository.history(module, id).await.unwrap()
        });
        assert!(!ts.is_empty(), "LIVE_HISTORY fixture must return rows");
        assert_eq!(ts, native, "history: live SQL and native adapter rows");

        tx.rollback().await.unwrap();
    })
    .await;
}
