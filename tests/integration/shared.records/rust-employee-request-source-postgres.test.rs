//! Native Records employee-request purchase-source witness over migrated
//! PostgreSQL. These prove the Records-owned guarded row, Company/scope
//! isolation and writer-first freshness for Procurement consumers. The
//! witness grants no `commerce.purchase` authority, reads no allocation
//! or order-link facts and claims no owner completion.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_records::public::employee_request_source::{
    EmployeeRequestScope, PgEmployeeRequestSource, lock_employee_request_sources,
    prepare_order_handoff_source, prepare_quotation_source,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};

fn holding_scope(company: &str) -> EmployeeRequestScope {
    EmployeeRequestScope {
        company_id: company.into(),
        branch_id: None,
        department_id: None,
    }
}

fn business(error: PgTransactionError<erpx_server_shared::public::BusinessError>) -> &'static str {
    match error {
        PgTransactionError::Business(error) => {
            assert!(
                error.kind == erpx_server_shared::public::FailureKind::NotFound
                    || error.kind == erpx_server_shared::public::FailureKind::Forbidden
                    || error.kind == erpx_server_shared::public::FailureKind::Conflict
                    || error.kind == erpx_server_shared::public::FailureKind::InvalidInput,
                "unexpected error kind {:?}",
                error.kind
            );
            error.code
        }
        other => panic!("expected an owner business failure, got {other:?}"),
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
        "INSERT INTO employee_requests(id,company_id,branch_id,department_id,kind,category,title,description,location,impact,needed_date,items_json,currency,requester_user_id,created_by,parent_request_id,state,team_id,assignee_user_id,routing_json,approval_json,blocked_reason,resolution,purchase_order_id,handover_json,version,evidence_version,created_at,updated_at) VALUES \
         ('request-quoted','company','branch-1',NULL,'purchase','purchasing','Cables','Stock up','Warehouse','normal',NULL,'[{\"description\":\"Cable\",\"quantity\":\"10\"}]','THB','requester','creator',NULL,'in_progress',NULL,'handler','{}','{\"decision\":\"approved\"}',NULL,'',NULL,'{}',3,2,100,500),\
         ('request-unassigned','company',NULL,NULL,'purchase','purchasing','Bolts','Stock up','Warehouse','normal',NULL,'[]','THB','requester','creator',NULL,'queued',NULL,NULL,'{}','{\"decision\":\"approved\"}',NULL,'',NULL,'{}',1,1,100,500),\
         ('request-draft','company',NULL,NULL,'purchase','purchasing','Draft','New','Warehouse','normal',NULL,'[]','THB','requester','creator',NULL,'draft',NULL,'handler','{}','{\"decision\":\"pending\"}',NULL,'',NULL,'{}',1,1,100,500),\
         ('request-service','company',NULL,NULL,'service','it','Help','Fix','Office','normal',NULL,'[]','THB','requester','creator',NULL,'in_progress',NULL,'handler','{}','{\"decision\":\"approved\"}',NULL,'',NULL,'{}',2,1,100,500),\
         ('request-foreign','foreign',NULL,NULL,'purchase','purchasing','Foreign','Outside','Warehouse','normal',NULL,'[]','THB','requester','creator',NULL,'in_progress',NULL,'handler','{}','{\"decision\":\"approved\"}',NULL,'',NULL,'{}',1,1,100,500);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn quotation(
    pool: &DatabasePool,
    scope: &EmployeeRequestScope,
    actor: &str,
    id: &str,
) -> Result<serde_json::Value, PgTransactionError<erpx_server_shared::public::BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    lock_employee_request_sources(&mut tx).await?;
    let mut source = PgEmployeeRequestSource::new(&mut tx);
    let prepared = prepare_quotation_source(&mut source, scope, actor, id)
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => PgTransactionError::Business(error),
            other => other,
        });
    match prepared {
        Ok(prepared) => {
            let value = serde_json::json!({
                "id": prepared.request.id,
                "kind": prepared.request.kind,
                "state": prepared.request.state,
                "version": prepared.request.version,
                "evidence_version": prepared.request.evidence_version,
                "branch_id": prepared.request.branch_id,
                "title": prepared.request.title,
            });
            prepared.require_current(&mut source).await?;
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            tx.rollback().await.unwrap();
            Err(error)
        }
    }
}

#[tokio::test]
async fn quotation_source_reads_the_guarded_purchase_row() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let value = quotation(
            &pool,
            &holding_scope("company"),
            "handler",
            "request-quoted",
        )
        .await
        .unwrap();
        assert_eq!(value["id"], "request-quoted");
        assert_eq!(value["kind"], "purchase");
        assert_eq!(value["state"], "in_progress");
        assert_eq!(value["version"], 3);
        assert_eq!(value["evidence_version"], 2);
        assert_eq!(value["branch_id"], "branch-1");
        assert_eq!(value["title"], "Cables");
    })
    .await;
}

#[tokio::test]
async fn foreign_out_of_scope_and_non_quotation_rows_are_denied() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        // Foreign company: the row is invisible, not reported present.
        assert_eq!(
            business(
                quotation(
                    &pool,
                    &holding_scope("foreign"),
                    "handler",
                    "request-quoted"
                )
                .await
                .unwrap_err()
            ),
            "REQUEST_NOT_FOUND"
        );
        // Branch-containing row is outside a different branch selection.
        let branch = EmployeeRequestScope {
            company_id: "company".into(),
            branch_id: Some("branch-2".into()),
            department_id: None,
        };
        assert_eq!(
            business(
                quotation(&pool, &branch, "handler", "request-quoted")
                    .await
                    .unwrap_err()
            ),
            "REQUEST_NOT_FOUND"
        );
        // Missing id.
        assert_eq!(
            business(
                quotation(
                    &pool,
                    &holding_scope("company"),
                    "handler",
                    "request-missing"
                )
                .await
                .unwrap_err()
            ),
            "REQUEST_NOT_FOUND"
        );
        // Wrong assignee.
        assert_eq!(
            business(
                quotation(&pool, &holding_scope("company"), "other", "request-quoted")
                    .await
                    .unwrap_err()
            ),
            "PURCHASE_REQUEST_REQUIRED"
        );
        // Service request is never a purchase quotation source.
        assert_eq!(
            business(
                quotation(
                    &pool,
                    &holding_scope("company"),
                    "handler",
                    "request-service"
                )
                .await
                .unwrap_err()
            ),
            "PURCHASE_REQUEST_REQUIRED"
        );
        // Draft purchase request is not yet approved.
        assert_eq!(
            business(
                quotation(&pool, &holding_scope("company"), "handler", "request-draft")
                    .await
                    .unwrap_err()
            ),
            "PURCHASE_REQUEST_REQUIRED"
        );
    })
    .await;
}

#[tokio::test]
async fn order_handoff_source_allows_unassigned_approved_purchase() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let mut source = PgEmployeeRequestSource::new(&mut tx);
        let prepared = prepare_order_handoff_source(
            &mut source,
            &holding_scope("company"),
            "handler",
            "request-unassigned",
            false,
        )
        .await
        .unwrap();
        assert_eq!(prepared.request.id, "request-unassigned");
        prepared.require_current(&mut source).await.unwrap();
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn guard_detects_version_and_items_drift_in_the_same_unit() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        // Version drift: prepare, bump the row in the same unit, revalidate.
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let prepared = {
            let mut source = PgEmployeeRequestSource::new(&mut tx);
            prepare_quotation_source(&mut source, &holding_scope("company"), "handler", "request-quoted")
                .await
                .unwrap()
        };
        sqlx::query("UPDATE employee_requests SET version=version+1, updated_at=600 WHERE id='request-quoted'")
            .execute(&mut *tx)
            .await
            .unwrap();
        {
            let mut source = PgEmployeeRequestSource::new(&mut tx);
            assert_eq!(
                business(prepared.require_current(&mut source).await.unwrap_err()),
                "VERSION_CONFLICT"
            );
        }
        tx.rollback().await.unwrap();
        // Items drift: same unit, only items_json changes.
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let prepared = {
            let mut source = PgEmployeeRequestSource::new(&mut tx);
            prepare_quotation_source(&mut source, &holding_scope("company"), "handler", "request-quoted")
                .await
                .unwrap()
        };
        sqlx::query("UPDATE employee_requests SET items_json='[]', updated_at=600 WHERE id='request-quoted'")
            .execute(&mut *tx)
            .await
            .unwrap();
        {
            let mut source = PgEmployeeRequestSource::new(&mut tx);
            assert_eq!(
                business(prepared.require_current(&mut source).await.unwrap_err()),
                "VERSION_CONFLICT"
            );
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn writer_first_reader_sees_the_fresh_committed_row() {
    use sqlx::Connection;
    fixture::with_database(6, |pool| async move {
        setup(&pool).await;
        // Writer holds an uncommitted row update.
        let mut writer = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let writer_pid: i32 =
            sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE employee_requests SET version=version+1, updated_at=600 WHERE id='request-quoted'")
            .execute(&mut *writer)
            .await
            .unwrap();
        let other_pool = pool.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let reader_task = tokio::spawn(async move {
            // Own connection, pid before BEGIN so the guarded FOR SHARE
            // read is the first transactional statement.
            let mut connection = other_pool.raw().acquire().await.unwrap();
            let pid: i32 =
                sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut reader = connection.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
            send.send(pid).unwrap();
            // Table lock first: SHARE ROW EXCLUSIVE conflicts with the
            // writer's ROW EXCLUSIVE, so the lock queues behind the
            // writer and the snapshot starts after the writer commits.
            lock_employee_request_sources(&mut reader).await.unwrap();
            let mut source = PgEmployeeRequestSource::new(&mut reader);
            prepare_quotation_source(&mut source, &holding_scope("company"), "handler", "request-quoted")
                .await
                .map(|prepared| prepared.request.version)
        });
        let reader_pid = receive.await.unwrap();
        let mut blocked = false;
        for _ in 0..200 {
            blocked = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))")
                .bind(writer_pid)
                .bind(reader_pid)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(blocked, "guarded read must wait on the writer before its snapshot");
        writer.commit().await.unwrap();
        let version = reader_task
            .await
            .unwrap()
            .expect("reader sees the committed row");
        assert_eq!(version, 4);
    })
    .await;
}

#[tokio::test]
async fn commerce_request_cte_matches_actual_typescript_direct_link_query() {
    use erpx_server_records::public::commerce_request_links::{
        inspect_commerce_request_links, lock_commerce_request_links,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let commerce = root.join("packages/server-commerce/src/adapters/commerce.ts");
    let records = root
        .join("packages/server-records/src/adapters/commerce-final-owner-projection-sources.ts");
    let procurement = root.join(
        "packages/server-procurement/src/adapters/commerce-final-owner-projection-sources.ts",
    );
    let script = r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const fs=await import('node:fs/promises'),r=await import(process.argv[2]),p=await import(process.argv[3]),s=await fs.readFile(process.argv[1],'utf8');const m=s.match(/readEmployeeRequests1:\s*\(facts:[\s\S]*?\)\s*=>\s*db\.prepare\(`([^`]+)`\)\.bind\(([^)]+)\)/);if(!m||m[2].replace(/\s/g,'')!=='facts.companyId,facts.id,facts.id2')throw Error('SOURCE_BINDS_CHANGED');const app=await fs.readFile('packages/server-commerce/src/application/commerce.ts','utf8');if(!/readEmployeeRequests1\(\{\s*companyId:\s*actor\.companyId,\s*id:\s*id,\s*id2:\s*id\s*\}\)/.test(app))throw Error('CALLER_ID2_CHANGED');let sql=m[1].replace('${recordsProjectionSources.employee_requests}',r.commerceFinalOwnerProjectionSources.employee_requests).replace('${procurementProjectionSources.procurement_request_orders}',p.commerceFinalOwnerProjectionSources.procurement_request_orders);if(sql.includes('${'))throw Error('UNRESOLVED_SOURCE');let n=0;sql=sql.replace(/\?/g,()=>'$'+(++n));if(n!==3)throw Error('BINDS_CHANGED');process.stdout.write(JSON.stringify({sql,records:r.commerceFinalOwnerProjectionSources.employee_requests}));"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(root)
            .args(["--conditions=react-server", "-e", script])
            .arg(commerce)
            .arg(records)
            .arg(procurement)
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "actual TS oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let query: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let original = query["sql"].as_str().unwrap().to_owned();
    let projection = query["records"].as_str().unwrap().to_owned();
    fixture::with_database(2,|pool|async move{
        setup(&pool).await;
        sqlx::raw_sql("UPDATE employee_requests SET purchase_order_id='order-direct' WHERE id='request-quoted'; UPDATE employee_requests SET purchase_order_id='order-foreign' WHERE id='request-foreign'").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();assert!(inspect_commerce_request_links(&mut tx).await.is_err());lock_commerce_request_links(&mut tx).await.unwrap();let witness=inspect_commerce_request_links(&mut tx).await.unwrap();
        let needle=format!("({projection})");assert_eq!(original.matches(&needle).count(),1);let projected=format!("WITH {} {}",witness.cte_sql(),original.replacen(&needle,"commerce_request_links",1));
        for (company,id,expected) in [("company","order-direct",Some("request-quoted")),("company","missing",None),("company","order-foreign",None),("foreign","order-foreign",Some("request-foreign"))] {
            let a:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(original.as_str())).bind(company).bind(id).bind(id).fetch_optional(&mut *tx).await.unwrap();let b:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind(company).bind(id).bind(id).fetch_optional(&mut *tx).await.unwrap();assert_eq!(a.as_deref(),expected);assert_eq!(b,a);
        }
        // The raw boundary deliberately does not claim content stability.
        sqlx::query("SAVEPOINT change_link").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.employee_requests SET purchase_order_id='order-new' WHERE id='request-quoted'").execute(&mut *tx).await.unwrap();witness.require_current(&mut tx).await.unwrap();let a:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(original.as_str())).bind("company").bind("order-direct").bind("order-direct").fetch_optional(&mut *tx).await.unwrap();let b:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind("company").bind("order-direct").bind("order-direct").fetch_optional(&mut *tx).await.unwrap();assert_eq!(a,None);assert_eq!(a,b);sqlx::query("ROLLBACK TO SAVEPOINT change_link").execute(&mut *tx).await.unwrap();tx.rollback().await.unwrap();
        let mut fresh=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_commerce_request_links(&mut fresh).await.unwrap();assert!(witness.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut saved=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_lock").execute(&mut *saved).await.unwrap();lock_commerce_request_links(&mut saved).await.unwrap();let held=inspect_commerce_request_links(&mut saved).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_lock").execute(&mut *saved).await.unwrap();assert!(held.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commerce_request_source_fences_writers_shadow_rls_and_resource_limits() {
    use erpx_server_records::public::commerce_request_links::{
        inspect_commerce_request_links, lock_commerce_request_links,
    };
    fixture::with_database(2,|pool|async move{
        setup(&pool).await;
        let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_commerce_request_links(&mut tx).await.unwrap();let source=inspect_commerce_request_links(&mut tx).await.unwrap();
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();let(send,receive)=tokio::sync::oneshot::channel();let other=pool.clone();
        let writer=tokio::spawn(async move{let mut w=other.raw().begin().await.unwrap();sqlx::query("SET LOCAL lock_timeout='3s'").execute(&mut *w).await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *w).await.unwrap();send.send(pid).unwrap();let error=sqlx::query("UPDATE public.employee_requests SET title='writer' WHERE id='request-draft'").execute(&mut *w).await.unwrap_err();assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("55P03"));w.rollback().await.unwrap();});
        let pid=receive.await.unwrap();tokio::time::timeout(std::time::Duration::from_secs(2),async{loop{let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2)) AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=$2 AND relation='public.employee_requests'::regclass AND mode='RowExclusiveLock' AND NOT granted)").bind(reader_pid).bind(pid).fetch_one(&mut *tx).await.unwrap();if blocked{break}tokio::task::yield_now().await;}}).await.expect("exact writer waits on the Records table fence");writer.await.unwrap();source.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
        let mut shadow=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();sqlx::query("CREATE TEMP TABLE employee_requests(id text)").execute(&mut *shadow).await.unwrap();lock_commerce_request_links(&mut shadow).await.unwrap();assert!(inspect_commerce_request_links(&mut shadow).await.is_err());shadow.rollback().await.unwrap();
        let mut rls=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();let bypass:bool=sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *rls).await.unwrap();assert!(!bypass);sqlx::raw_sql("ALTER TABLE public.employee_requests ENABLE ROW LEVEL SECURITY; ALTER TABLE public.employee_requests FORCE ROW LEVEL SECURITY").execute(&mut *rls).await.unwrap();let active:bool=sqlx::query_scalar("SELECT row_security_active('public.employee_requests'::regclass)").fetch_one(&mut *rls).await.unwrap();assert!(active);lock_commerce_request_links(&mut rls).await.unwrap();assert!(inspect_commerce_request_links(&mut rls).await.is_err());rls.rollback().await.unwrap();
        let mut bounded=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_commerce_request_links(&mut bounded).await.unwrap();let before=inspect_commerce_request_links(&mut bounded).await.unwrap();sqlx::query("SAVEPOINT byte_budget").execute(&mut *bounded).await.unwrap();
        sqlx::query("INSERT INTO public.employee_requests(id,company_id,kind,category,requester_user_id,created_by,created_at,updated_at) SELECT repeat('x',1800)||n::text,'company','purchase','fixture','requester','creator',0,0 FROM generate_series(1,1200) n").execute(&mut *bounded).await.unwrap();let measured:(i64,i64)=sqlx::query_as("SELECT count(*)::bigint,COALESCE(sum(octet_length(to_jsonb(s)::text)::bigint),0)::bigint FROM (SELECT id,company_id,purchase_order_id FROM public.employee_requests) s").fetch_one(&mut *bounded).await.unwrap();assert_eq!(measured.0,1205);assert!(measured.1>2097152,"byte-only excess: {measured:?}");let error=inspect_commerce_request_links(&mut bounded).await.err().expect("byte limit");assert!(matches!(error,PgTransactionError::Business(ref e) if e.code=="RECORDS_COMMERCE_LINK_LIMIT"));assert!(before.require_current(&mut bounded).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT byte_budget").execute(&mut *bounded).await.unwrap();before.require_current(&mut bounded).await.unwrap();
        sqlx::query("INSERT INTO public.employee_requests(id,company_id,kind,category,requester_user_id,created_by,created_at,updated_at) SELECT 'bounded-request-'||n::text,'company','purchase','fixture','requester','creator',0,0 FROM generate_series(1,9996) n").execute(&mut *bounded).await.unwrap();let measured:(i64,i64)=sqlx::query_as("SELECT count(*)::bigint,COALESCE(sum(octet_length(to_jsonb(s)::text)::bigint),0)::bigint FROM (SELECT id,company_id,purchase_order_id FROM public.employee_requests) s").fetch_one(&mut *bounded).await.unwrap();assert_eq!(measured.0,10001);assert!(measured.1<=2097152,"row-only excess: {measured:?}");let error=inspect_commerce_request_links(&mut bounded).await.err().expect("row limit");assert!(matches!(error,PgTransactionError::Business(ref e) if e.code=="RECORDS_COMMERCE_LINK_LIMIT"));bounded.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commerce_selected_request_matches_actual_typescript_read_and_retains_exact_row() {
    use erpx_server_records::public::commerce_selected_request::{
        lock_commerce_selected_request, observe_commerce_selected_request,
    };
    use erpx_server_shared::public::CompanyId;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ts = root.join("packages/server-records/src/adapters/employee-requests-read.ts");
    let script = r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const db={prepare(sql){return{bind(...values){return{first:async()=>({sql,values})}}}}};const q=await m.nativeEmployeeRequestReadRepository(db).readReadEmployeeRequestRow('request-quoted','company');if(JSON.stringify(q.values)!==JSON.stringify(['request-quoted','company']))throw Error('SOURCE_BINDS_CHANGED');let n=0;process.stdout.write(q.sql.replace(/\?/g,()=>'$'+(++n)));"#;
    let oracle =
        std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(root)
            .args(["--conditions=react-server", "-e", script])
            .arg(ts)
            .output()
            .unwrap();
    assert!(
        oracle.status.success(),
        "actual Records TS oracle: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let raw = String::from_utf8(oracle.stdout).unwrap();
    let query = format!("SELECT to_jsonb(r)::text FROM ({raw}) r");
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;let company=CompanyId::new("company").unwrap();let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();assert!(observe_commerce_selected_request(&mut tx,&company,"request-quoted").await.is_err());lock_commerce_selected_request(&mut tx).await.unwrap();
        for (id,expected) in [("request-quoted",true),("request-foreign",false),("missing",false)] {
            let selected=observe_commerce_selected_request(&mut tx,&company,id).await.unwrap();let actual:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str())).bind(id).bind(company.as_str()).fetch_optional(&mut *tx).await.unwrap();assert_eq!(selected.image(),actual.as_deref());assert_eq!(selected.image().is_some(),expected);selected.require_current(&mut tx).await.unwrap();
        }
        let selected=observe_commerce_selected_request(&mut tx,&company,"request-quoted").await.unwrap();sqlx::query("SAVEPOINT request_change").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.employee_requests SET title='changed',approval_json='{broken' WHERE id='request-quoted'").execute(&mut *tx).await.unwrap();assert!(selected.require_current(&mut tx).await.is_err());let malformed=observe_commerce_selected_request(&mut tx,&company,"request-quoted").await.unwrap();let decoded:serde_json::Value=serde_json::from_str(malformed.image().unwrap()).unwrap();assert_eq!(decoded["approval_json"],"{broken");sqlx::query("ROLLBACK TO SAVEPOINT request_change").execute(&mut *tx).await.unwrap();selected.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT request_size").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.employee_requests SET description=repeat('x',2097153) WHERE id='request-quoted'").execute(&mut *tx).await.unwrap();let error=observe_commerce_selected_request(&mut tx,&company,"request-quoted").await.err().expect("bounded row");assert!(matches!(error,PgTransactionError::Business(ref e) if e.code=="RECORDS_SELECTED_REQUEST_LIMIT"));sqlx::query("ROLLBACK TO SAVEPOINT request_size").execute(&mut *tx).await.unwrap();selected.require_current(&mut tx).await.unwrap();
        assert!(observe_commerce_selected_request(&mut tx,&company,&"x".repeat(8193)).await.is_err());tx.rollback().await.unwrap();let mut fresh=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_commerce_selected_request(&mut fresh).await.unwrap();assert!(selected.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut save=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *save).await.unwrap();sqlx::query("SAVEPOINT request_lock").execute(&mut *save).await.unwrap();lock_commerce_selected_request(&mut save).await.unwrap();let retained=observe_commerce_selected_request(&mut save,&company,"request-quoted").await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT request_lock").execute(&mut *save).await.unwrap();assert!(retained.require_current(&mut save).await.is_err());save.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commerce_selected_request_fences_real_writer_shadow_and_rls() {
    use erpx_server_records::public::commerce_selected_request::{
        lock_commerce_selected_request, observe_commerce_selected_request,
    };
    use erpx_server_shared::public::CompanyId;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let company=CompanyId::new("company").unwrap();
        let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_commerce_selected_request(&mut tx).await.unwrap();
        let selected=observe_commerce_selected_request(&mut tx,&company,"request-draft").await.unwrap();
        assert!(selected.image().is_some());
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();
        let (send,receive)=tokio::sync::oneshot::channel(); let other=pool.clone();
        let writer=tokio::spawn(async move {
            let mut w=other.raw().begin().await.unwrap();
            sqlx::query("SET LOCAL lock_timeout='3s'").execute(&mut *w).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *w).await.unwrap(); send.send(pid).unwrap();
            let error=sqlx::query("UPDATE public.employee_requests SET title='writer' WHERE id='request-draft'").execute(&mut *w).await.unwrap_err();
            assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("55P03")); w.rollback().await.unwrap();
        });
        let pid=receive.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2),async { loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2)) AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=$2 AND relation='public.employee_requests'::regclass AND mode='RowExclusiveLock' AND NOT granted)")
                .bind(reader_pid).bind(pid).fetch_one(&mut *tx).await.unwrap();
            if blocked { break } tokio::task::yield_now().await;
        }}).await.expect("actual writer waits on selected request fence");
        writer.await.unwrap(); selected.require_current(&mut tx).await.unwrap(); tx.rollback().await.unwrap();

        let mut shadow=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        sqlx::query("CREATE TEMP TABLE employee_requests(id text)").execute(&mut *shadow).await.unwrap();
        lock_commerce_selected_request(&mut shadow).await.unwrap();
        let error=observe_commerce_selected_request(&mut shadow,&company,"request-draft").await.err().expect("temp shadow rejected");
        assert!(matches!(error,PgTransactionError::Persistence(ref e) if e.code=="RECORDS_SELECTED_REQUEST_FENCE_REQUIRED")); shadow.rollback().await.unwrap();

        let mut rls=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        let bypass:bool=sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *rls).await.unwrap(); assert!(!bypass);
        sqlx::raw_sql("ALTER TABLE public.employee_requests ENABLE ROW LEVEL SECURITY; ALTER TABLE public.employee_requests FORCE ROW LEVEL SECURITY").execute(&mut *rls).await.unwrap();
        let active:bool=sqlx::query_scalar("SELECT row_security_active('public.employee_requests'::regclass)").fetch_one(&mut *rls).await.unwrap(); assert!(active);
        lock_commerce_selected_request(&mut rls).await.unwrap();
        let error=observe_commerce_selected_request(&mut rls,&company,"request-draft").await.err().expect("active RLS rejected");
        assert!(matches!(error,PgTransactionError::Persistence(ref e) if e.code=="RECORDS_SELECTED_REQUEST_FENCE_REQUIRED")); rls.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commerce_selected_request_writer_first_retry_reads_committed_row_and_insert() {
    use erpx_server_records::public::commerce_selected_request::{
        lock_commerce_selected_request, observe_commerce_selected_request,
    };
    use erpx_server_shared::public::CompanyId;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut writer=pool.raw().begin().await.unwrap();
        let updated=sqlx::query("UPDATE public.employee_requests SET title='Committed writer',version=version+1 WHERE id='request-draft'")
            .execute(&mut *writer).await.unwrap(); assert_eq!(updated.rows_affected(),1);
        sqlx::query("INSERT INTO public.employee_requests(id,company_id,kind,category,title,requester_user_id,created_by,created_at,updated_at) VALUES('writer-inserted','company','purchase','purchasing','Committed insert','requester','creator',1,1)")
            .execute(&mut *writer).await.unwrap();
        let mut attempt=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        // The first protected operation is the lock; it must fail without a snapshot.
        let error=lock_commerce_selected_request(&mut attempt).await.unwrap_err();
        assert!(matches!(error,PgTransactionError::Persistence(ref e) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        attempt.rollback().await.unwrap(); writer.commit().await.unwrap();
        let mut retry=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_commerce_selected_request(&mut retry).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let updated=observe_commerce_selected_request(&mut retry,&company,"request-draft").await.unwrap();
        let value:serde_json::Value=serde_json::from_str(updated.image().expect("committed row")).unwrap();
        assert_eq!(value["title"],"Committed writer"); assert_eq!(value["version"],2);
        let inserted=observe_commerce_selected_request(&mut retry,&company,"writer-inserted").await.unwrap();
        let value:serde_json::Value=serde_json::from_str(inserted.image().expect("committed insert")).unwrap();
        assert_eq!(value["title"],"Committed insert");
        updated.require_current(&mut retry).await.unwrap(); inserted.require_current(&mut retry).await.unwrap(); retry.rollback().await.unwrap();
    }).await;
}
