#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{
    MigrationFile, MigrationTrack, inspect_migrations, load_migrations, migrate_database,
};
use fixture::with_database;

#[tokio::test]
async fn whole_plpgsql_file_receipts_and_failures_are_atomic_and_reruns_are_checked() {
    with_database(2,|pool|async move {
        let first=MigrationFile::from_sql("0001_owner.sql","CREATE TABLE migrated(value integer); DO $$ BEGIN INSERT INTO migrated VALUES(7); INSERT INTO migrated VALUES(8); END $$;").unwrap();
        let failed=MigrationFile::from_sql("0002_failure.sql","INSERT INTO migrated VALUES(9); CREATE TABLE rolled_back(id integer); DO $$ BEGIN RAISE EXCEPTION 'late migration failure'; END $$;").unwrap();
        assert!(migrate_database(&pool,MigrationTrack::Control,&[failed,first.clone()]).await.is_err());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT sum(value) FROM migrated").fetch_one(pool.raw()).await.unwrap(),15);
        assert!(!sqlx::query_scalar::<_,bool>("SELECT to_regclass('rolled_back') IS NOT NULL").fetch_one(pool.raw()).await.unwrap());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM erpx_migrations").fetch_one(pool.raw()).await.unwrap(),1);
        let run=migrate_database(&pool,MigrationTrack::Control,std::slice::from_ref(&first)).await.unwrap();assert_eq!((run.applied,run.skipped),(0,1));
        let changed=MigrationFile::from_sql(first.name(),"CREATE TABLE changed(id int)").unwrap();
        assert_eq!(migrate_database(&pool,MigrationTrack::Control,&[changed]).await.unwrap_err().code,"MIGRATION_CHANGED");
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT checksum FROM erpx_migrations").fetch_one(pool.raw()).await.unwrap(),first.checksum());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND pid=pg_backend_pid()").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}
#[tokio::test]
async fn concurrent_runners_serialize_before_creating_receipts() {
    with_database(2, |pool| async move {
        let files = vec![
            MigrationFile::from_sql(
                "0001_race.sql",
                "DO $$ BEGIN PERFORM pg_sleep(0.05); END $$; CREATE TABLE race(id int)",
            )
            .unwrap(),
        ];
        let (a, b) = tokio::join!(
            migrate_database(&pool, MigrationTrack::Control, &files),
            migrate_database(&pool, MigrationTrack::Control, &files)
        );
        let a = a.unwrap();
        let b = b.unwrap();
        assert_eq!(a.applied + b.applied, 1);
        assert_eq!(a.skipped + b.skipped, 1);
    })
    .await;
}
#[tokio::test]
async fn inspection_refuses_privileged_table_fallback_when_the_narrow_reader_is_missing() {
    with_database(2,|pool|async move {
        let files=vec![MigrationFile::from_sql("0001_init.sql","CREATE TABLE owner_data(id int)").unwrap()];
        assert_eq!(inspect_migrations(&pool,MigrationTrack::Control,&files).await.unwrap().pending,vec!["0001_init.sql"]);
        migrate_database(&pool,MigrationTrack::Control,&files).await.unwrap();
        assert_eq!(inspect_migrations(&pool,MigrationTrack::Control,&files).await.unwrap_err().code,"MIGRATION_RECEIPTS_UNAVAILABLE");
        sqlx::raw_sql("CREATE FUNCTION erpx_migration_receipts() RETURNS TABLE(name text) LANGUAGE sql STABLE SECURITY DEFINER SET search_path=public AS $$ SELECT name FROM erpx_migrations $$").execute(pool.raw()).await.unwrap();
        let status=inspect_migrations(&pool,MigrationTrack::Control,&files).await.unwrap();assert_eq!(status.applied,1);assert!(status.pending.is_empty());
    }).await;
}
#[tokio::test]
async fn all_immutable_schema_tracks_bootstrap_fresh_isolated_databases() {
    for track in [
        MigrationTrack::Control,
        MigrationTrack::ControlRead,
        MigrationTrack::HoldingWrite,
        MigrationTrack::HoldingRead,
    ] {
        with_database(2, move |pool| async move {
            let root =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
            let files = load_migrations(root, track).await.unwrap();
            assert!(!files.is_empty());
            let result = migrate_database(&pool, track, &files).await.unwrap();
            assert_eq!(result.applied, files.len());
            assert_eq!(result.skipped, 0);
            let again = migrate_database(&pool, track, &files).await.unwrap();
            assert_eq!(again.applied, 0);
            assert_eq!(again.skipped, files.len());
        })
        .await;
    }
}

#[tokio::test]
async fn grandfathered_receipts_require_both_exact_hashes_and_are_never_rewritten() {
    with_database(2, |pool| async move {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let all = load_migrations(root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &[])
            .await
            .unwrap();
        for (name, old) in [
            (
                "0041_leave_self_service.sql",
                "fb926c55f087b296c11f0f3356a32c2473bd3c56ea13834f25f186564da9038a",
            ),
            (
                "0293_executive_evidence.sql",
                "250b2bbc409499b500d4dfc3b667c3e9bd849a15d04749cf3047c804563790e9",
            ),
        ] {
            let file = all.iter().find(|file| file.name() == name).unwrap();
            sqlx::query("INSERT INTO erpx_migrations(name,checksum) VALUES($1,$2)")
                .bind(name)
                .bind(old)
                .execute(pool.raw())
                .await
                .unwrap();
            assert_eq!(
                migrate_database(
                    &pool,
                    MigrationTrack::HoldingWrite,
                    std::slice::from_ref(file)
                )
                .await
                .unwrap()
                .skipped,
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT checksum FROM erpx_migrations WHERE name=$1"
                )
                .bind(name)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                old
            );
            let mutated = MigrationFile::from_sql(
                name,
                "SELECT 'different migration with the same filename'",
            )
            .unwrap();
            assert_eq!(
                migrate_database(&pool, MigrationTrack::HoldingWrite, &[mutated])
                    .await
                    .unwrap_err()
                    .to_string(),
                format!("MIGRATION_CHANGED:holding-write:{name}")
            );
            assert_eq!(
                migrate_database(&pool, MigrationTrack::Control, std::slice::from_ref(file))
                    .await
                    .unwrap_err()
                    .code,
                "MIGRATION_CHANGED"
            );
        }
    })
    .await;
}

/// Original populated1047→1048 preservation assertions, without a retired
/// business writer. Raw rows here are explicitly historical fixture data.
#[tokio::test]
async fn populated_field_intake_retains_every_original_column_receipt_and_write_shape() {
    with_database(2, |pool| async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let mut files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        files.retain(|file|file.name()<"1049");
        let split=files.iter().position(|file|file.name().starts_with("1048_")).unwrap();
        assert_eq!(split+1,files.len());
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files[..split]).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::raw_sql(r#"
INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company','company','company','Historical Company','company');
INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES('branch','branch','branch','Historical branch','company','company','branch');
INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('customer','company','customer','HIST','Retained Service customer','THB',1,1);
INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse','company','branch','HIST','Retained Service warehouse');
INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)VALUES('sale','company','branch','sales','HIST','customer','warehouse','THB','2026-10-15','2026-11-15',10000,0,'creator',1,1);
INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at)VALUES('ticket','company','branch','sale','customer','Retained source case','Original recorded issue','technician','2026-10-16','creator',1,1);
INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json)VALUES('ticket-create','ticket',1,'create','creator','Declared populated migration fixture','technician','2026-10-16','open',1,'{}');
INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,reviewed_by,reviewed_at,version,created_at,updated_at)VALUES('agreement','company','branch','customer','Retained reviewed agreement','2026-01-01','2027-12-31','THB',10000,'active','creator','reviewer',1,2,1,1);
INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at)VALUES('agreement-create','agreement',1,'create','creator','Declared populated migration fixture','draft',1),('agreement-activate','agreement',2,'activate','reviewer','Declared populated migration fixture','active',1);
INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,scheduled_at,status,created_by,created_at,updated_at)VALUES('order','company','branch','ticket',1,'agreement',2,'Original recorded issue','Retained customer site','technician',60001,'scheduled','creator',1,1);
INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)VALUES('order-create','order',1,'create','creator','Declared populated migration fixture','scheduled','technician',60001,'{}',1);
"#).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        let shapes:Vec<(String,Vec<String>)>=sqlx::query_as("SELECT t.tablename::text,array_agg(c.column_name::text ORDER BY c.ordinal_position) FROM pg_tables t JOIN information_schema.columns c ON c.table_schema=t.schemaname AND c.table_name=t.tablename WHERE t.schemaname='public' AND t.tablename<>'erpx_migrations' GROUP BY t.tablename ORDER BY t.tablename").fetch_all(pool.raw()).await.unwrap();
        async fn snapshot(pool:&erpx_server_runtime::public::postgres::DatabasePool,shapes:&[(String,Vec<String>)])->Vec<(String,i64,String)>{
            let mut result=Vec::new();
            for(table,columns)in shapes{
                assert!(std::iter::once(table).chain(columns).all(|v|!v.is_empty()&&v.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'_')));
                let projection=columns.iter().map(|v|format!("\"{v}\"")).collect::<Vec<_>>().join(",");
                let query=format!("SELECT count(*)::bigint,md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) FROM(SELECT {projection} FROM \"{table}\")t");
                // Identifiers came from this disposable schema and passed the closed ASCII check above.
                let(count,digest):(i64,String)=sqlx::query_as(sqlx::AssertSqlSafe(query)).fetch_one(pool.raw()).await.unwrap();result.push((table.clone(),count,digest));
            }
            result
        }
        let before=snapshot(&pool,&shapes).await;
        let receipts:Vec<(String,String)>=sqlx::query_as("SELECT name,checksum FROM erpx_migrations ORDER BY name").fetch_all(pool.raw()).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        assert_eq!(snapshot(&pool,&shapes).await,before);
        let old_receipts:Vec<(String,String)>=sqlx::query_as("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1048_%' ORDER BY name").fetch_all(pool.raw()).await.unwrap();assert_eq!(old_receipts,receipts);
        let markers:(Option<String>,Option<String>,Option<i64>)=sqlx::query_as("SELECT service_agreement_id,service_order_id,service_order_version FROM support_tickets WHERE id='ticket'").fetch_one(pool.raw()).await.unwrap();assert_eq!(markers,(None,None,None));
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files[..split]).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::raw_sql(r#"
UPDATE field_service_orders SET status='dispatched',version=version+1,updated_at=2 WHERE id='order' AND version=1;
INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)VALUES('order-dispatch','order',2,'dispatch','technician','Unchanged pre-intake write shape','dispatched','technician',60001,'{}',2);
UPDATE support_tickets SET title='Retained source case amended',version=version+1,updated_at=2 WHERE id='ticket' AND version=1;
INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json)VALUES('ticket-amend','ticket',2,'add-information','creator','Unchanged pre-intake write shape','technician','2026-10-16','open',2,'{}');
"#).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        let order:(String,i64)=sqlx::query_as("SELECT status,version FROM field_service_orders WHERE id='order'").fetch_one(pool.raw()).await.unwrap();assert_eq!(order,("dispatched".into(),2));
        let ticket:(String,String,i64,Option<String>)=sqlx::query_as("SELECT source_kind,status,version,service_order_id FROM support_tickets WHERE id='ticket'").fetch_one(pool.raw()).await.unwrap();assert_eq!(ticket,("sales_order".into(),"open".into(),2,None));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grants").fetch_one(pool.raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM erpx_migrations WHERE name LIKE '1048_%'").fetch_one(pool.raw()).await.unwrap(),1);
    }).await;
}
