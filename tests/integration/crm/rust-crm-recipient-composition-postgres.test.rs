//! Joined CRM recipient source classification against the retained legacy SQL.
//! This does not prove selected-actor authorization, command effects or runtime mount.
#[path = "../../support/rust-postgres.rs"]
mod projection_fixture;

#[tokio::test]
async fn crm_sales_recipient_join_matches_real_ts_predicate_for_owner_sources() {
    use erpx_server_crm::public::activity_source::{lock_crm_activity_sources, inspect_crm_activity_recipient_source, CrmActivitySourceKind, CrmActivitySourceRequest};
    use erpx_server_identity_access::public::{permission_catalog_lock::lock_permission_catalog_sources, sales_recipient::{lock_sales_recipient_sources,inspect_sales_assignment_join,inspect_sales_iam_boundary}};
    use erpx_server_people_payroll::public::{workforce_reference::inspect_sales_workforce_join, identity_facts::inspect_people_sales_authority_boundary};
    use erpx_server_organization::public::inspect_organization_sales_authority_boundary;
    use erpx_server_manage_erpx::public::inspect_beta_policy_authority_boundary;
    use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
    use erpx_server_shared::public::CompanyId;
    use erpx_service::crm_recipient_eligibility::inspect_crm_sales_recipient_eligibility;
    projection_fixture::with_database(2,|pool|async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES ('company','company','company','Company',NULL,'company',NULL,'active',1),('branch','branch','branch','Branch','company','company',NULL,'active',1),('department','department','department','Department','branch','company','branch','active',1),('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1); INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','sales_recipient_test','Sales recipient',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','sales_recipient_test',1,'[\"commerce.read\",\"commerce.sales\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('recipient-grant','lead-owner','sales_recipient_test','[]','company',0,9007199254740991,1,'holding'); INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('recipient-person','lead-owner','company','Sales','TH','fixture',0,0); INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('recipient-engagement','recipient-person','company','employee','salaried','active','2000-01-01','Sales','THB','fixture',0,0); INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES('recipient-lead','company','branch','department','Lead','Contact','Detail','Staff','Brief','lead-owner','2026-12-01','open',1,'maker',1,1); INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES('recipient-activity','company','recipient-lead','call','Call','Discuss','activity-owner','2026-12-01','maker',1,1)").execute(pool.raw()).await.unwrap();
        let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../.."))
            .args(["--conditions=react-server","-e","import {salesRecipientEligibility} from './packages/server-sales/src/adapters/recipient.ts'; process.stdout.write(salesRecipientEligibility('l','l.owner_id'))"])
            .output().unwrap();
        assert!(oracle.status.success()); let predicate=String::from_utf8(oracle.stdout).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        lock_permission_catalog_sources(&mut tx).await.unwrap();
        let mut prep=lock_sales_recipient_sources(&mut tx).await.unwrap();
        lock_crm_activity_sources(prep.connection()).await.unwrap();
        let locks=prep.bind().await.unwrap();
        let crm=inspect_crm_activity_recipient_source(&mut tx,CrmActivitySourceRequest{company_id:CompanyId::new("company").unwrap(),branch_id:Some("branch".into()),department_id:Some("department".into()),source_id:"recipient-activity".into(),kind:CrmActivitySourceKind::Activity}).await.unwrap();
        let identity=inspect_sales_assignment_join(&mut tx,&locks).await.unwrap();
        let people=inspect_sales_workforce_join(&mut tx).await.unwrap();
        let iam=inspect_sales_iam_boundary(&mut tx,&locks).await.unwrap();
        let people_authority=inspect_people_sales_authority_boundary(&mut tx).await.unwrap();
        let organization=inspect_organization_sales_authority_boundary(&mut tx).await.unwrap();
        let management=inspect_beta_policy_authority_boundary(&mut tx).await.unwrap();
        let oracle_sql=format!("SELECT ({predicate}) eligible FROM public.crm_lead_activities x JOIN public.crm_leads l ON l.id=x.lead_id AND l.company_id=x.company_id WHERE x.id=$1 AND x.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4)");
        async fn legacy(tx:&mut sqlx::PgConnection,crm:&erpx_server_crm::public::activity_source::CurrentCrmActivityRecipientSource,sql:&str)->bool {
            let row=crm.bind_source(sqlx::query(sqlx::AssertSqlSafe(sql))).fetch_one(tx).await.unwrap();
            sqlx::Row::try_get::<bool,_>(&row,"eligible").unwrap()
        }
        let initial=inspect_crm_sales_recipient_eligibility(&mut tx,&crm,&identity,&iam,&people,&people_authority,&organization,&management).await.unwrap();
        assert!(legacy(&mut tx,&crm,&oracle_sql).await);assert!(initial.is_eligible());initial.require_current(&mut tx).await.unwrap();
        for change in ["UPDATE assignments SET revoked_at=1 WHERE id='recipient-grant'", "UPDATE hr_people SET status='inactive' WHERE id='recipient-person'", "UPDATE hr_engagements SET start_date='9999-01-01' WHERE id='recipient-engagement'", "UPDATE assignments SET scope_kind='branch',scope_node_id='wrong-branch' WHERE id='recipient-grant'", "UPDATE assignments SET scope_kind='department',scope_node_id=NULL WHERE id='recipient-grant'"] {
            sqlx::query("SAVEPOINT recipient_negative").execute(&mut *tx).await.unwrap();
            sqlx::query(sqlx::AssertSqlSafe(change)).execute(&mut *tx).await.unwrap();
            assert!(!legacy(&mut tx,&crm,&oracle_sql).await);
            assert!(!inspect_crm_sales_recipient_eligibility(&mut tx,&crm,&identity,&iam,&people,&people_authority,&organization,&management).await.unwrap().is_eligible());
            assert!(initial.require_current(&mut tx).await.is_err());
            sqlx::query("ROLLBACK TO SAVEPOINT recipient_negative").execute(&mut *tx).await.unwrap();
            initial.require_current(&mut tx).await.unwrap();
        }
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn staged_reassign_binds_collaboration_notice_to_real_crm_witness() {
    use erpx_server_crm::public::{activities_native::{ActivityActor,ActivityChangeInput,PgCrmActivityCommands,RetainedCrmActivityReassign},activity_source::lock_crm_activity_sources};
    use erpx_server_collaboration::public::crm_activity_notification::{CrmActivityNotificationFacts,PgCrmActivityNotificationSource,lock_crm_activity_notifications,prepare_crm_activity_notification};
    use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack,PgTransactionError};
    use erpx_server_shared::public::BusinessError;
    use sqlx::PgConnection;
    type Error=PgTransactionError<BusinessError>;
    // This is test-local composition of the two public owner ports. The
    // persisted CRM witness, not caller-provided notification labels, supplies
    // the facts and the same-unit before/after source verification.
    struct Source<'a>{crm:&'a RetainedCrmActivityReassign,facts:CrmActivityNotificationFacts}
    impl PgCrmActivityNotificationSource for Source<'_>{
        fn facts(&self)->&CrmActivityNotificationFacts{&self.facts}
        async fn require_before(&self,c:&mut PgConnection)->Result<(),Error>{self.crm.require_current(c).await}
        async fn require_after(&self,c:&mut PgConnection)->Result<(),Error>{self.crm.require_current(c).await}
    }
    projection_fixture::with_database(2,|pool|async move{
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','company','Company',NULL,'company',NULL,'active'),('branch','branch','branch','Branch','company','company',NULL,'active'); INSERT INTO crm_leads(id,company_id,branch_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES('lead','company','branch','Lead','Customer','Contact','Staff','Brief','owner','2026-12-01','open',1,'maker',1,1); INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,status,created_by,created_at,updated_at) VALUES('activity','company','lead','call','Call','Discuss','old-owner','2026-11-20','open','maker',1,1)").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_crm_activity_sources(&mut tx).await.unwrap();lock_crm_activity_notifications(&mut tx).await.unwrap();
        // The selected actor is an owner fixture. This does not establish IAM,
        // Sales recipient eligibility, purchase, Workflow or runtime mounting.
        let actor=ActivityActor{user_id:"manager".into(),company_id:"company".into(),branch_id:Some("branch".into()),department_id:None,can_sales:true,can_manage:true,has_read:true,same_person_ids:vec![]};
        let input:ActivityChangeInput=serde_json::from_value(serde_json::json!({"action":"reassign","expectedVersion":1,"ownerId":"new-owner","dueDate":"2026-12-05","reason":"  Handoff  "})).unwrap();
        let prepared=PgCrmActivityCommands::new(&mut tx).prepare_reassign(&actor,"activity","notice-reassign",&input).await.unwrap();let crm=PgCrmActivityCommands::new(&mut tx).apply_reassign(prepared,1234).await.unwrap();let f=crm.notification_facts();
        let source=Source{crm:&crm,facts:CrmActivityNotificationFacts{command_key:f.command_key,company_id:f.company_id,recipient:f.recipient,activity_id:f.activity_id,version:f.version,status:f.status.into(),updated_at:f.updated_at}};
        let notice=prepare_crm_activity_notification(&mut tx,&source).await.unwrap().persist(&mut tx).await.unwrap();notice.require_current_with_source(&mut tx,&source).await.unwrap();
        let row:(String,String,String,i64,String,i64)=sqlx::query_as("SELECT company_id,user_id,record_id,record_version,message,created_at FROM public.staff_notifications WHERE id='crm-activity-notice-notice-reassign'").fetch_one(&mut *tx).await.unwrap();assert_eq!(row,("company".into(),"new-owner".into(),"activity".into(),2,"CRM activity assigned: review the due date and next action".into(),1234));
        let inbox:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM public.inbox_events WHERE id='legacy:crm-activity-notice-notice-reassign'),(SELECT count(*) FROM public.inbox_deliveries WHERE id='crm-activity-notice-notice-reassign')").fetch_one(&mut *tx).await.unwrap();assert_eq!(inbox,(1,1));
        sqlx::query("SAVEPOINT later_effect").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.crm_lead_activities SET status='completed' WHERE id='activity'").execute(&mut *tx).await.unwrap();assert!(notice.require_current_with_source(&mut tx,&source).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT later_effect").execute(&mut *tx).await.unwrap();notice.require_current_with_source(&mut tx,&source).await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(notice.require_current_with_source(&mut other,&source).await.is_err());other.rollback().await.unwrap();tx.rollback().await.unwrap();
        let rollback:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM crm_lead_activities WHERE id='activity'),(SELECT count(*) FROM crm_lead_activity_events WHERE activity_id='activity'),(SELECT count(*) FROM staff_notifications WHERE id='crm-activity-notice-notice-reassign'),(SELECT count(*) FROM inbox_events WHERE id='legacy:crm-activity-notice-notice-reassign'),(SELECT count(*) FROM inbox_deliveries WHERE id='crm-activity-notice-notice-reassign')").fetch_one(pool.raw()).await.unwrap();assert_eq!(rollback,(1,0,0,0,0));
    }).await;
}

#[test]
fn crm_reassign_raw_wire_matches_actual_json_parse_and_ts_schema() {
    use erpx_service::crm_reassign_wire::parse_crm_reassign_wire;
    // Exercise JSON.parse itself, including duplicate keys and isolated UTF-16;
    // compare logical receipt identity rather than lossy PostgreSQL storage.
    let cases=[
        r#"{"id":"asset-1","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"  Reviewed  ","extra":{"x":1}}"#,
        r#"{"reason":" old ","id":"first","ownerId":"bad","expectedVersion":99,"action":"reassign","id":"second","ownerId":"owner","expectedVersion":1e0,"dueDate":"2026-12-05","reason":"  New reason  "}"#,
        r#"{"id":"\ud800","action":"reassign","expectedVersion":1,"ownerId":"new\udc00","dueDate":"2026-12-05","reason":"\ufeffx\ud800y\u00a0"}"#,
        r#"{"id":"","action":"reassign","expectedVersion":9007199254740991,"ownerId":"😀","dueDate":"2000-02-29","reason":"😀ok","\ud800":"unknown"}"#,
        r#"{"id":"abc\u0000def","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"valid"}"#,
        r#"{"id":"a","action":"reassign","expectedVersion":9007199254740992,"ownerId":"owner","dueDate":"2026-12-05","reason":"valid"}"#,
        r#"{"id":"a","action":"reassign","expectedVersion":1e400,"ownerId":"owner","dueDate":"2026-12-05","reason":"valid"}"#,
        r#"{"id":"a","action":"reassign","expectedVersion":1,"ownerId":"","dueDate":"2026-12-05","reason":"valid"}"#,
        r#"{"id":"a","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-02-29","reason":"valid"}"#,
        r#"{"id":null,"action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"valid"}"#,
        r#"{"id":"a","action":"cancel","expectedVersion":1,"reason":"valid"}"#,
        r#"{"id":"a","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"valid""#,
    ];
    let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const cases=JSON.parse(process.argv[2]);const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));function unsafe(s){for(let i=0;i<s.length;i++){const c=s.charCodeAt(i);if(c===0)return true;if(c>=0xd800&&c<=0xdbff){if(i+1>=s.length||s.charCodeAt(i+1)<0xdc00||s.charCodeAt(i+1)>0xdfff)return true;i++}else if(c>=0xdc00&&c<=0xdfff)return true}return false}process.stdout.write(JSON.stringify(cases.map(raw=>{try{const obj=JSON.parse(raw);if(!obj||typeof obj!=='object'||typeof obj.id!=='string')return null;const parsed=m.crmActivityChangeSchema.safeParse(obj);if(!parsed.success||parsed.data.action!=='reassign')return null;return {target:units(obj.id),input:JSON.stringify(parsed.data),unsafe:unsafe(obj.id)||unsafe(parsed.data.ownerId)}}catch{return null}})));"#;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let module=root.join("packages/server-crm/src/domain/activities.ts");let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(module).arg(serde_json::to_string(&cases).unwrap()).output().unwrap();assert!(out.status.success(),"actual TS schema oracle unavailable");let expected:Vec<serde_json::Value>=serde_json::from_slice(&out.stdout).unwrap();
    for (index,raw) in cases.iter().enumerate(){let actual=parse_crm_reassign_wire(raw.as_bytes()).ok().map(|wire|serde_json::json!({"target":wire.target_utf16,"input":wire.input.as_str(),"unsafe":wire.unsafe_lookup})).unwrap_or(serde_json::Value::Null);assert_eq!(actual,expected[index],"raw case {index}");}
}

#[test]
fn crm_reassign_raw_wire_known_size_and_depth_mismatches_are_visible() {
    use erpx_service::crm_reassign_wire::parse_crm_reassign_wire;
    use std::io::Write;
    use std::process::Stdio;
    // The retained Next route uses Request.json() and Zod strips unknown keys.
    // These in-process Request cases are not an HTTP gateway size measurement.
    let large=format!(r#"{{"id":"activity","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"Review","unknown":"{}"}}"#,"x".repeat(2*1024*1024));
    let depth=format!(r#"{{"id":"activity","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"Review","unknown":{}0{}}}"#,"[".repeat(130),"]".repeat(130));
    let small=r#"{"id":"activity","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"Review"}"#.to_owned();
    let cases=vec![small,large,depth];
    let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const cases=JSON.parse(await Bun.stdin.text());const out=[];for(const body of cases){try{const raw=await new Request('https://acme.example.test',{method:'POST',body}).json();const p=m.crmActivityChangeSchema.safeParse(raw);out.push(p.success&&p.data.action==='reassign'&&typeof raw.id==='string')}catch{out.push(false)}}process.stdout.write(JSON.stringify(out));"#;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let module=root.join("packages/server-crm/src/domain/activities.ts");
    let mut child=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(module).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();child.stdin.take().unwrap().write_all(serde_json::to_string(&cases).unwrap().as_bytes()).unwrap();let result=child.wait_with_output().unwrap();assert!(result.status.success(),"actual Request.json/CRM schema oracle unavailable");let accepted:Vec<bool>=serde_json::from_slice(&result.stdout).unwrap();assert_eq!(accepted,vec![true,true,true]);
    assert!(parse_crm_reassign_wire(cases[0].as_bytes()).is_ok());
    assert!(parse_crm_reassign_wire(cases[1].as_bytes()).is_err(),"CommandJson 2 MiB bound currently rejects this TS-accepted body");
    assert!(parse_crm_reassign_wire(cases[2].as_bytes()).is_err(),"CommandJson 128-level bound currently rejects this TS-accepted body");
}

#[test]
fn crm_reassign_node_request_json_bom_and_invalid_utf8_match() {
    use erpx_service::crm_reassign_wire::parse_crm_reassign_wire;
    let base=br#"{"id":"activity","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"Review"}"#.to_vec();
    let mut bom=vec![0xef,0xbb,0xbf];bom.extend_from_slice(&base);
    let mut malformed=base.clone();let at=malformed.windows(6).position(|w|w==b"Review").unwrap()+2;malformed.insert(at,0xff);
    fn insert_at(base:&[u8],marker:&[u8],offset:usize,bytes:&[u8])->Vec<u8>{let mut out=base.to_vec();let at=out.windows(marker.len()).position(|w|w==marker).unwrap()+offset;out.splice(at..at,bytes.iter().copied());out}
    let truncated=insert_at(&base,b"Review",6,&[0xe2,0x82]);
    let bad_continuation=insert_at(&base,b"Review",2,&[0xe2,0x28,0xa1]);
    let overlong=insert_at(&base,b"Review",2,&[0xc0,0xaf]);
    let bad_id=insert_at(&base,b"activity",3,&[0xff]);
    let bad_owner=insert_at(&base,b"owner\"",2,&[0xf0,0x9f]);
    let mut double_bom=vec![0xef,0xbb,0xbf,0xef,0xbb,0xbf];double_bom.extend_from_slice(&base);
    let mut triple_bom=vec![0xef,0xbb,0xbf,0xef,0xbb,0xbf,0xef,0xbb,0xbf];triple_bom.extend_from_slice(&base);
    let escape=br#"{"id":"\ud800","action":"reassign","expectedVersion":1,"ownerId":"owner","dueDate":"2026-12-05","reason":"Review"}"#.to_vec();
    let broken_escape=insert_at(&escape,b"\\ud800",3,&[0xff]);
    let cases=vec![base,bom,malformed,truncated,bad_continuation,overlong,bad_id,bad_owner,double_bom,escape,broken_escape,triple_bom];
    let script=r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const m=await import(process.argv[1]);const cases=JSON.parse(process.argv[2]);const unsafe=s=>{for(let i=0;i<s.length;i++){const c=s.charCodeAt(i);if(c===0)return true;if(c>=0xd800&&c<=0xdbff){if(i+1>=s.length||s.charCodeAt(i+1)<0xdc00||s.charCodeAt(i+1)>0xdfff)return true;i++}else if(c>=0xdc00&&c<=0xdfff)return true}return false};const out=[];for(const bytes of cases){try{const obj=await new Request('https://acme.example.test',{method:'POST',body:Uint8Array.from(bytes)}).json();const parsed=m.crmActivityChangeSchema.safeParse(obj);if(typeof obj.id!=='string'||!parsed.success||parsed.data.action!=='reassign'){out.push(null);continue}out.push({target:Array.from({length:obj.id.length},(_,i)=>obj.id.charCodeAt(i)),input:JSON.stringify(parsed.data),unsafe:unsafe(obj.id)||unsafe(parsed.data.ownerId)})}catch{out.push(null)}}process.stdout.write(JSON.stringify(out));"#;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let module=root.join("packages/server-crm/src/domain/activities.ts");
    let out=std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required")).current_dir(root).args(["--conditions=react-server","--input-type=module","-e",script]).arg(module).arg(serde_json::to_string(&cases).unwrap()).output().unwrap();
    assert!(out.status.success(),"Node Request.json and actual TS schema oracle: {}",String::from_utf8_lossy(&out.stderr));
    let expected:Vec<serde_json::Value>=serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(expected.len(),12);assert!(expected[..8].iter().all(|v|!v.is_null()));
    assert_eq!(expected[8],expected[0],"Node Request.json accepts two leading BOMs");
    assert!(expected[11].is_null(),"three leading BOMs are rejected");
    assert_eq!(expected[9]["unsafe"],true,"lone surrogate retains unsafe lookup protection");
    assert!(expected[10].is_null(),"malformed bytes inside a Unicode escape must not parse");
    assert!(expected[6]["target"].as_array().unwrap().contains(&serde_json::json!(65533)));
    assert!(expected[7]["input"].as_str().unwrap().contains('�'));
    assert_eq!(expected[1],expected[0],"Request.json strips the leading UTF-8 BOM");
    assert!(expected[2]["input"].as_str().unwrap().contains('�'),"malformed UTF-8 becomes U+FFFD before schema parsing");
    let actual:Vec<_>=cases.iter().map(|raw|parse_crm_reassign_wire(raw).ok().map(|wire|serde_json::json!({"target":wire.target_utf16,"input":wire.input.as_str(),"unsafe":wire.unsafe_lookup})).unwrap_or(serde_json::Value::Null)).collect();
    assert_eq!(actual,expected,"Rust wire normalization must match Node Request.json and the actual CRM schema");
}
