//! Existing shared prerequisite owner fixture, relocated without changing its facts.
use crate::facts::{account,review};
use erpx_server_shared::public::Clock;

pub async fn service_posting_finance_fixture(system: account::System) -> review::Prepared {
    let p = review::setup(system).await;
    let now = p.system.scenario.clock.now().get();
    let pool = &p.system.holdings[0];
    sqlx::raw_sql("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,created_at,updated_at) VALUES ('part-ar','company','chart','111','AR','asset','THB','receivable',0,0),('part-ap','company','chart','211','AP','liability','THB','payable',0,0),('part-revenue','company','chart','411','Revenue','revenue','THB','none',0,0),('part-inventory','company','chart','121','Inventory','asset','THB','none',0,0),('part-clearing','company','chart','221','Clearing','liability','THB','none',0,0),('part-cogs','company','chart','511','COGS','expense','THB','none',0,0),('part-input','company','chart','131','Input','asset','THB','none',0,0),('part-output','company','chart','231','Output','liability','THB','none',0,0),('part-cash','company','chart','141','Cash','asset','THB','bank',0,0)").execute(pool.raw()).await.unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at) VALUES('company','service',1,'active',0,1,'Synthetic source','synthetic',$1),('company','service-parts-assets',1,'active',0,1,'Synthetic source','synthetic',$1)").bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'setup-origin-'||package_id,company_id,package_id,'fixture-a','synthetic',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id IN('service','service-parts-assets')").bind(now).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    p
}
