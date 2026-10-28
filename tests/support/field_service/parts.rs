//! Disposable actual Account/SSO + published IAM + real source rows. Synthetic
//! historical Job data is not a create/dispatch/start migration acceptance.
#[path = "../finance/review-composition.rs"]
pub mod review;
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{
    AssignmentId, Clock, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis,
};
use erpx_server_workflow::public::commands::CommandOutcome;
use erpx_service::field_service_parts::{
    FieldServicePartsAdministration, FieldServicePartsAdministrationError,
    FieldServicePartsConfiguration, FieldServicePartsEnvironment,
};
pub use review::{Error, account, business, post, reverse};
use std::sync::atomic::{AtomicI64, Ordering};

pub struct Parts {
    pub review: review::Prepared,
    pub selection: HoldingWorkspaceRequest,
}
pub async fn setup(system: account::System) -> Parts {
    setup_tracking(system, "none").await
}
pub async fn setup_tracking(system: account::System, tracking: &str) -> Parts {
    setup_with_grants(system, tracking, true).await
}
/// Source-order test fixture without a Service purchase or retained Service history.
pub async fn setup_without_service_history(system: account::System) -> Parts {
    setup_with_grants(system, "none", false).await
}
async fn setup_with_grants(system: account::System, tracking: &str, grants: bool) -> Parts {
    assert!(matches!(tracking, "none" | "lot" | "serial"));
    let review = review::setup(system).await;
    let pool = &review.system.holdings[0];
    let now = review.system.scenario.clock.now().get();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','north','Synthetic North','company','company','branch','active');INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_field_parts','Synthetic actual Service parts operator',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',1,'[\"records.read\",\"records.write\",\"module.field_service.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic',0);INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('field-person','fixture-a','company','PRIVATE_SYNTHETIC_WORKER','TH','fixture-a',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('field-engagement','field-person','company','employee','salaried','active','2100-01-01','PRIVATE_JOB','THB','fixture-a',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','field-person','active','local',1);INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('customer','company','customer','SYNTHETIC','PRIVATE_CUSTOMER','THB',0,0);INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at)VALUES('inventory','company','chart','110','Inventory','asset','THB',0,0),('expense','company','chart','510','Expense','expense','THB',0,0);INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id)VALUES('company','chart','USD','inventory','inventory','sales','inventory','inventory','expense','inventory','inventory','cash');INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse','company','branch','SYNTHETIC','Synthetic warehouse');").execute(pool.raw()).await.unwrap();
    let mut stock = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('product','company','SYNTHETIC','Synthetic spare',100,$1)").bind(tracking).execute(&mut*stock).await.unwrap();
    sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('warehouse','product',5000,1000)").execute(&mut*stock).await.unwrap();
    if tracking == "lot" {
        sqlx::raw_sql("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,expiry_date,created_by,created_at)VALUES('lot-1','company','product','lot','L1','2026-12-31','fixture-a',0),('lot-2','company','product','lot','L2',NULL,'fixture-a',0);INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES('lot-1','warehouse',3000,600),('lot-2','warehouse',2000,400)").execute(&mut*stock).await.unwrap();
    } else if tracking == "serial" {
        for index in 0..5 {
            sqlx::query("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,serial_number,created_by,created_at)VALUES($1,'company','product','serial',$2,'fixture-a',0)").bind(format!("serial-{index}")).bind(format!("S{index}")).execute(&mut*stock).await.unwrap();
            sqlx::query("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES($1,'warehouse',1000,200)").bind(format!("serial-{index}")).execute(&mut*stock).await.unwrap();
        }
    }
    stock.commit().await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('field-fixture-a','fixture-a','custom_field_parts','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE active_work_contexts SET assignment_id='field-fixture-a',revision=8,updated_at=$1 WHERE user_id='fixture-a'").bind(now).execute(pool.raw()).await.unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,reviewed_by,reviewed_at,version,created_at,updated_at)VALUES('agreement','company','branch','customer','Synthetic retained agreement','2026-01-01','2027-12-31','THB',10000,'active','fixture-a','fixture-b',$1,2,$1,$1)").bind(now-100).execute(&mut*tx).await.unwrap();
    sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at)VALUES('agreement-review','agreement',2,'activate','fixture-b','Independent synthetic approval','active',$1)").bind(now-100).execute(&mut*tx).await.unwrap();
    // Existing historical orders intentionally use only original schema and
    // deferred triggers. No source/policy port is replaced by fixture booleans.
    for (id, status, version) in [
        ("reserve-order", "scheduled", 1_i64),
        ("issue-order", "in_visit", 3_i64),
    ] {
        let ticket = format!("ticket-{id}");
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,source_kind,service_agreement_id,service_order_id,service_order_version)VALUES($1,'company','branch',NULL,'customer','Synthetic retained intake',repeat('PRIVATE_ISSUE',200000),'fixture-a',to_char(to_timestamp(($3::bigint+60000)/1000.0)AT TIME ZONE 'UTC','YYYY-MM-DD'),'fixture-a',$3,$3,'field_service','agreement',$2,$4)")
            .bind(&ticket).bind(id).bind(now).bind(version).execute(&mut*tx).await.unwrap();
        sqlx::query("INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,scheduled_at,status,created_by,version,created_at,updated_at)VALUES($1,'company','branch',$2,1,'agreement',2,repeat('PRIVATE_ISSUE',200000),'PRIVATE_SITE','fixture-a',$3+60000,$4,'fixture-a',$5,$3,$3)")
            .bind(id).bind(&ticket).bind(now).bind(status).bind(version).execute(&mut*tx).await.unwrap();
        sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)VALUES($1,$2,$3,$4,'fixture-a','Synthetic historical retained event',$5,'fixture-a',$6+60000,'{}',$6)")
            .bind(format!("historical-{id}-{version}")).bind(id).bind(version).bind(if version==1{"create"}else{"start"}).bind(status).bind(now).execute(&mut*tx).await.unwrap();
    }
    sqlx::query("INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at)VALUES('reserved-part','issue-order','warehouse','product',3000,$1,'fixture-a','Synthetic original reservation',$2)").bind(now+3_600_000).bind(now-100).execute(&mut*tx).await.unwrap();
    if grants {
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('company','service',1,'active',0,1,'Synthetic approved source','public synthetic evidence',$1),('company','service-parts-assets',1,'active',0,1,'Synthetic approved source','public synthetic evidence',$1)").bind(now).execute(&mut*tx).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'field-origin-'||package_id,company_id,package_id,'fixture-a','synthetic-source-grant',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company'AND package_id IN('service','service-parts-assets')").bind(now).execute(&mut*tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    let selection = HoldingWorkspaceRequest {
        expected: Some(ExpectedWorkSelection {
            revision: 8,
            assignment_id: AssignmentId::new("field-fixture-a").unwrap(),
            assignment_version: 3,
        }),
        ..review.maker_selection.clone()
    };
    Parts { review, selection }
}
pub struct WriteClock(pub AtomicI64);
impl Clock for WriteClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0.fetch_add(1, Ordering::SeqCst))
    }
}
impl Parts {
    pub fn pool(&self) -> &DatabasePool {
        &self.review.system.holdings[0]
    }
    pub async fn execute(&self, id: &str, key: &str, raw: &str) -> Result<CommandOutcome, Error> {
        let guard = self.review.system.scenario.clock.clone();
        let write = std::sync::Arc::new(WriteClock(AtomicI64::new(guard.now().get())));
        let configuration = FieldServicePartsConfiguration::new(
            FieldServicePartsEnvironment::Development,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            PackageEnforcementMode::Enforced,
        )
        .unwrap();
        let administration = FieldServicePartsAdministration::new(
            self.review.authorizer.clone(),
            configuration,
            write,
            guard,
        );
        match administration
            .execute(
                "acme.example.test",
                self.review.maker.clone(),
                self.selection.clone(),
                id.to_owned(),
                raw.as_bytes().to_vec(),
                key.to_owned(),
            )
            .await
        {
            Ok(result) => Ok(result),
            Err(FieldServicePartsAdministrationError::Transaction(error)) => Err(error),
            Err(FieldServicePartsAdministrationError::Wire(error)) => {
                panic!("synthetic fixture command invalid: {error:?}")
            }
        }
    }
    pub async fn execute_schema(
        &self,
        id: &str,
        key: &str,
        raw: &str,
    ) -> Result<CommandOutcome, FieldServicePartsAdministrationError> {
        let guard = self.review.system.scenario.clock.clone();
        let write = std::sync::Arc::new(WriteClock(AtomicI64::new(guard.now().get())));
        let config = FieldServicePartsConfiguration::new(
            FieldServicePartsEnvironment::Development,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            PackageEnforcementMode::Enforced,
        )
        .unwrap();
        let admin = FieldServicePartsAdministration::new(
            self.review.authorizer.clone(),
            config,
            write,
            guard,
        );
        let selection = self.selection.clone();
        admin
            .execute_schema_ingress(
                "acme.example.test",
                self.review.maker.clone(),
                move || Ok(selection),
                RecordId::new(id).unwrap(),
                raw.as_bytes().to_vec(),
                CommandKey::new(key).unwrap(),
            )
            .await
    }
    pub async fn issue(&self, key: &str) -> Result<CommandOutcome, Error> {
        self.execute("issue-order",key,r#"{"action":"issue-part","expectedVersion":3,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Synthetic genuine Service issue"}"#).await
    }
    pub async fn journal(&self, key: &str) -> String {
        sqlx::query_scalar("SELECT journal_id FROM operational_inventory_consumptions WHERE id=$1")
            .bind(format!("operational-part-{key}"))
            .fetch_one(self.pool().raw())
            .await
            .unwrap()
    }
    pub async fn counts(&self, key: &str) -> (i64, i64, i64, i64, i64) {
        sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id=$1),(SELECT count(*)FROM audit_events WHERE command_id=$1),(SELECT count(*)FROM outbox_events WHERE command_id=$1),(SELECT count(*)FROM operational_inventory_consumptions WHERE id='operational-part-'||$1),(SELECT count(*)FROM staff_notifications WHERE id='service-order-notice-'||$1)").bind(key).fetch_one(self.pool().raw()).await.unwrap()
    }
}
pub async fn withdraw(p: &Parts, package: &str) {
    let mut tx = p.pool().raw().begin().await.unwrap();
    let now = p.review.system.scenario.clock.now().get();
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence)FROM company_package_grants WHERE company_id='company'AND package_id=$1").bind(package).fetch_one(&mut*tx).await.unwrap();
    sqlx::query("UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='company'AND package_id=$1").bind(package).execute(&mut*tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT $1,'company',package_id,'fixture-a','synthetic-withdrawal',$2,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$3 FROM company_package_grants WHERE company_id='company'AND package_id=$4").bind(uuid::Uuid::new_v4().to_string()).bind(before).bind(now).bind(package).execute(&mut*tx).await.unwrap();
    tx.commit().await.unwrap();
}

/// Synthetic retained technician transition; no native dispatch/start claim.
pub async fn assign_historical_technician(p: &Parts, technician: &str, orders: &[&str]) {
    let at = p.review.system.scenario.clock.now().get();
    let mut tx = p.pool().raw().begin().await.unwrap();
    for order in orders {
        sqlx::query("UPDATE field_service_orders SET technician_id=$1,version=version+1,updated_at=$2 WHERE id=$3").bind(technician).bind(at).bind(order).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) SELECT $1,id,version,'dispatch','fixture-a','Synthetic retained technician',status,technician_id,scheduled_at,'{}'::jsonb,$2 FROM field_service_orders WHERE id=$3").bind(format!("synthetic-technician-{order}")).bind(at).bind(order).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
}
