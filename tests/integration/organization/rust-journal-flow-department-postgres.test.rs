#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::journal_flow_department::{
    journal_flow_team_name, lock_journal_flow_department_sources,
};
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId};

#[tokio::test]
async fn journal_flow_team_name_uses_active_same_company_node_or_company() {
    fixture::with_database(1, |pool| async move {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(root, MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &files).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
             VALUES('company','company','acme','Acme',NULL,'company',NULL,'active'), \
                   ('department','department','service','Service','company','company',NULL,'active'), \
                   ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active'), \
                   ('archived','department','archived','Archived','company','company',NULL,'suspended'), \
                   ('empty','department','empty','','company','company',NULL,'active'), \
                   ('space','department','space','   ','company','company',NULL,'active')",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let uow = PgUnitOfWork::new(pool.clone());

        // No department selected.
        let no_department = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    lock_journal_flow_department_sources(connection).await?;
                    journal_flow_team_name(connection, &CompanyId::new("company").unwrap(), None).await
                })
            })
            .await
            .unwrap();
        assert_eq!(no_department, "Company");

        // Active same-company node.
        let active = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    lock_journal_flow_department_sources(connection).await?;
                    journal_flow_team_name(
                        connection,
                        &CompanyId::new("company").unwrap(),
                        Some("department"),
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(active, "Service");

        // Evaluate the actual TS Flow expression under pinned Bun rather than
        // substituting a Rust copy of its nullish fallback.
        let script = r#"import {readFileSync} from 'node:fs';if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const source=readFileSync('packages/server-workflow/src/application/flow-routing.ts','utf8');const match=source.match(/teamName:\s*(p\.team_name\s*\?\?\s*"Company")/);if(!match)throw Error('FLOW_TEAM_EXPRESSION_CHANGED');const evaluate=new Function('p',`return (${match[1]})`);process.stdout.write(JSON.stringify([null,'','   ','Service'].map(team_name=>evaluate({team_name}))));"#;
        let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["-e",script]).output().unwrap();
        assert!(oracle.status.success(),"actual TS Flow expression unavailable");
        let expected:Vec<String>=serde_json::from_slice(&oracle.stdout).unwrap();
        for (index, department) in [None,Some("empty"),Some("space"),Some("department")].into_iter().enumerate() {
            let result=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|connection|Box::pin(async move {
                lock_journal_flow_department_sources(connection).await?;
                journal_flow_team_name(connection,&CompanyId::new("company").unwrap(),department).await
            })).await.unwrap();
            assert_eq!(result,expected[index],"department {department:?}");
        }

        // Missing, archived, or foreign node falls back to Company.
        for department in ["missing", "archived"] {
            let result = uow
                .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                    let department = department.to_owned();
                    Box::pin(async move {
                        lock_journal_flow_department_sources(connection).await?;
                        journal_flow_team_name(
                            connection,
                            &CompanyId::new("company").unwrap(),
                            Some(&department),
                        )
                        .await
                    })
                })
                .await
                .unwrap();
            assert_eq!(result, "Company", "department={department}");
        }
        let foreign = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    lock_journal_flow_department_sources(connection).await?;
                    journal_flow_team_name(
                        connection,
                        &CompanyId::new("company").unwrap(),
                        Some("foreign"),
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(foreign, "Company");

        // Calling outside a transaction is rejected.
        let mut connection = pool.raw().acquire().await.unwrap();
        let outside = journal_flow_team_name(
            &mut *connection,
            &CompanyId::new("company").unwrap(),
            None,
        )
        .await;
        assert!(outside.is_err());
    })
    .await;
}

#[tokio::test]
async fn stock_count_flow_organization_projection_retains_exact_source() {
    use erpx_server_organization::public::journal_flow_department::{inspect_stock_count_flow_organization,lock_stock_count_flow_organization_sources};
    fixture::with_database(2,|pool|async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status) VALUES('company','company','company','Company',NULL,'company','active'),('empty','department','empty','','company','company','active'),('space','department','space','   ','company','company','active'),('suspended','department','suspended','Suspended','company','company','suspended'); INSERT INTO staff_levels(id,company_id,code,name,rank,active,created_at,updated_at) VALUES('level','company','L1','Level',1,1,0,0); INSERT INTO organization_positions(id,company_id,title,level_id,created_by,created_at,updated_at) VALUES('position','company','Position','level','fixture',0,0)").execute(pool.raw()).await.unwrap();
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const {finalRootOwnerProjectionSources:s}=await import('./packages/server-organization/src/adapters/final-root-owner-projection-sources.ts');const {peoplePayrollFinalOwnerProjections:p}=await import('./packages/server-organization/src/adapters/people-payroll-final-owner-projections.ts');process.stdout.write(JSON.stringify({nodes:s.organization_nodes,levels:s.staff_levels,positions:s.organization_positions,join:p().workflow_flow_routing_facts_organization_nodes_n_join}));"#;
        let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).args(["--conditions=react-server","-e",script]).output().unwrap();assert!(out.status.success(),"TS Organization projections unavailable: {}",String::from_utf8_lossy(&out.stderr));let oracle:serde_json::Value=serde_json::from_slice(&out.stdout).unwrap();
        let company=CompanyId::new("company").unwrap();
        let mut missing=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *missing).await.unwrap();assert!(inspect_stock_count_flow_organization(&mut missing,&company).await.is_err());missing.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_stock_count_flow_organization_sources(&mut tx).await.unwrap();
        let source=inspect_stock_count_flow_organization(&mut tx,&company).await.unwrap();
        {
            let mut bound=source.bind_projection(&mut tx).await.unwrap();
            for (native,key,table) in [(bound.nodes_sql(),"nodes","organization_nodes"),(bound.levels_sql(),"levels","staff_levels"),(bound.positions_sql(),"positions","organization_positions")] {
                let ts=oracle[key].as_str().unwrap();assert_eq!(native,ts.replace(&format!("FROM {table}"),&format!("FROM public.{table}")));
                let n=format!("SELECT row_to_json(t)::text FROM ({native}) t WHERE company_id=$1 ORDER BY id");let t=format!("SELECT row_to_json(t)::text FROM ({ts}) t WHERE company_id=$1 ORDER BY id");
                let actual:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(n)).bind("company").fetch_all(bound.connection()).await.unwrap();let expected:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(t)).bind("company").fetch_all(bound.connection()).await.unwrap();assert_eq!(actual,expected);
            }
            assert_eq!(oracle["join"],format!("JOIN ({}) n",oracle["nodes"].as_str().unwrap()));
            bound.finish().await.unwrap();
        }
        for change in ["UPDATE organization_nodes SET display_name='changed' WHERE id='empty'","UPDATE staff_levels SET rank=rank+1 WHERE id='level'","UPDATE organization_positions SET title='changed' WHERE id='position'"] {
            sqlx::query("SAVEPOINT org_drift").execute(&mut *tx).await.unwrap();let mut bound=source.bind_projection(&mut tx).await.unwrap();sqlx::query(sqlx::AssertSqlSafe(change)).execute(bound.connection()).await.unwrap();assert!(bound.finish().await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT org_drift").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        }
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_stock_count_flow_organization_sources(&mut other).await.unwrap();assert!(source.bind_projection(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut lost=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *lost).await.unwrap();sqlx::query("SAVEPOINT before_org_lock").execute(&mut *lost).await.unwrap();lock_stock_count_flow_organization_sources(&mut lost).await.unwrap();let retained=inspect_stock_count_flow_organization(&mut lost,&company).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_org_lock").execute(&mut *lost).await.unwrap();assert!(retained.require_current(&mut lost).await.is_err());lost.rollback().await.unwrap();
    }).await;
}
