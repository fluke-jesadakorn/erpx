//! Native Marketing campaign/attribution reads over real migrated PostgreSQL.
//! These prove owner read SQL, Company/branch/department isolation, attribution
//! aggregation, serializable transaction guards, rollback and persisted-source
//! readback. They are not a Marketing command, package or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_marketing::public::campaign_attribution::{
    CandidateProvenance, MarketingAttributionMode, MarketingCampaignActor,
    MarketingCampaignEntitlement, MarketingCampaignEntitlementPort, MarketingCampaignFuture,
    PgMarketingCampaignRead, lock_campaign_attribution_sources, marketing_campaign_report,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use sqlx::Acquire;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

async fn wait_for_blocker(pool: &DatabasePool, waiter: i32, blocker: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT $2=ANY(pg_blocking_pids($1))")
                .bind(waiter)
                .bind(blocker)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("expected exact backend lock wait");
}

#[tokio::test]
async fn provenance_source_locks_wait_for_actual_writer_in_both_orders() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut writer=pool.raw().acquire().await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let mut tx=writer.begin().await.unwrap();
        // The retained TS writer starts with assets, then locks the remaining
        // owner tables in this order. Pause after its first lock.
        sqlx::query("LOCK TABLE marketing_assets IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO marketing_asset_events(id,asset_id,version,action,actor_id,reason,status,created_at) VALUES('race-activation','form-company',1,'activate','reviewer','approved','active',10); INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES('race-submission-1','candidate-1',1,'form_submit','web','evidence','owner',100)").execute(&mut *tx).await.unwrap();
        let (send_pid,recv_pid)=tokio::sync::oneshot::channel();let reader_pool=pool.clone();
        let reader=tokio::spawn(async move {
            let mut c=reader_pool.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send_pid.send(pid).unwrap();
            let mut t=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *t).await.unwrap();
            lock_campaign_attribution_sources(&mut t).await.unwrap();let mut reader=PgMarketingCampaignRead::new(&mut t);
            let rows=reader.attribution_with_provenance(&CompanyId::new("company").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await.unwrap();
            assert_eq!(rows.len(),1);assert_eq!(rows[0].captured,1);assert_eq!(rows[0].campaign,"AUTUMN");t.commit().await.unwrap();
        });
        let reader_pid=recv_pid.await.unwrap();wait_for_blocker(&pool,reader_pid,writer_pid).await;
        // NOWAIT verifies the blocked reader did not first retain campaigns;
        // the former opposite order fails here with 55P03.
        sqlx::query("LOCK TABLE marketing_asset_events,marketing_lead_candidates,marketing_engagement_events,marketing_campaigns,marketing_touches IN SHARE ROW EXCLUSIVE MODE NOWAIT").execute(&mut *tx).await.expect("reader must not hold a later Marketing lock while waiting on assets");
        tx.commit().await.unwrap();reader.await.unwrap();drop(writer);
        let mut reader=pool.raw().acquire().await.unwrap();let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let mut tx=reader.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_campaign_attribution_sources(&mut tx).await.unwrap();
        let mut source=PgMarketingCampaignRead::new(&mut tx);let before=source.attribution_with_provenance(&CompanyId::new("company").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await.unwrap();assert_eq!(before[0].captured,1);
        let (send_pid,recv_pid)=tokio::sync::oneshot::channel();let writer_pool=pool.clone();let writer=tokio::spawn(async move {
            let mut c=writer_pool.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send_pid.send(pid).unwrap();
            sqlx::query("INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES('race-submission-2','candidate-2',1,'form_submit','web','evidence','owner',101)").execute(&mut *c).await.unwrap();
        });
        let writer_pid=recv_pid.await.unwrap();wait_for_blocker(&pool,writer_pid,reader_pid).await;
        let after=source.attribution_with_provenance(&CompanyId::new("company").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await.unwrap();assert_eq!(after,before);let _ = source;tx.commit().await.unwrap();writer.await.unwrap();
        let mut fresh=reader.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_campaign_attribution_sources(&mut fresh).await.unwrap();let mut source=PgMarketingCampaignRead::new(&mut fresh);let rows=source.attribution_with_provenance(&CompanyId::new("company").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await.unwrap();assert_eq!(rows[0].captured,2);let _ = source;fresh.rollback().await.unwrap();
    }).await;
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[&str],
) -> MarketingCampaignActor {
    MarketingCampaignActor {
        user_id: "owner".into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["marketing".into()],
        expires_at: NOW.get() + 60_000,
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
        "INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('form-company','company','form','Lead form','{\"consentRequired\":true}','{}','active','owner','reviewer',1,10,10),\
         ('form-foreign','foreign','form','Foreign form','{\"consentRequired\":true}','{}','active','owner','reviewer',1,10,10);\
         INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES \
         ('campaign-autumn','company','AUTUMN','Autumn demand','owner',10),\
         ('campaign-summer','company','SUMMER','Summer demand','owner',11),\
         ('campaign-winter','company','WINTER','Winter demand','owner',12),\
         ('campaign-foreign','foreign','FOREIGN','Foreign campaign','owner',13);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('candidate-1','company','branch',NULL,'form-company',1,'Synthetic one','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','pending',NULL,'owner',NULL,1,100,100),\
         ('candidate-2','company','branch',NULL,'form-company',1,'Synthetic two','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','handed_off',NULL,'owner','reviewer',1,101,101),\
         ('candidate-3','company','branch',NULL,'form-company',1,'Synthetic three','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','rejected',NULL,'owner','reviewer',1,102,102),\
         ('candidate-4','company','branch',NULL,'form-company',1,'Synthetic four','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"email\"}','Synthetic brief','pending',NULL,'owner',NULL,1,103,103),\
         ('candidate-5','company','other-branch',NULL,'form-company',1,'Synthetic five','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','pending',NULL,'owner',NULL,1,104,104),\
         ('candidate-6','company','branch','department','form-company',1,'Synthetic six','Synthetic channel','Synthetic consent','{\"campaign\":\"SUMMER\",\"utmSource\":\"social\"}','Synthetic brief','pending',NULL,'owner',NULL,1,105,105),\
         ('candidate-7','company','branch',NULL,'form-company',1,'Synthetic seven','Synthetic channel','Synthetic consent','{}','Synthetic brief','pending',NULL,'owner',NULL,1,106,106),\
         ('candidate-foreign','foreign','foreign-branch',NULL,'form-foreign',1,'Foreign one','Synthetic channel','Synthetic consent','{\"campaign\":\"FOREIGN\",\"source\":\"web\"}','Synthetic brief','pending',NULL,'owner',NULL,1,107,107);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

struct Packages(bool);

#[tokio::test]
async fn captured_form_provenance_filters_only_matching_company_scoped_submissions() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO marketing_asset_events(id,asset_id,version,action,actor_id,reason,status,created_at) VALUES ('activation-company','form-company',1,'activate','reviewer','approved','active',10),('activation-foreign','form-foreign',1,'activate','reviewer','approved','active',10); INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES ('submission-1','candidate-1',1,'form_submit','web','evidence','owner',100),('submission-2','candidate-2',1,'form_submit','web','evidence','owner',101),('submission-foreign','candidate-foreign',1,'form_submit','web','evidence','owner',107)").execute(pool.raw()).await.unwrap();
        let no_lock = PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move |c| Box::pin(async move {
            let mut reader=PgMarketingCampaignRead::new(c);
            reader.attribution_with_provenance(&CompanyId::new("company").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await?;
            Ok(())
        })).await;
        assert!(matches!(no_lock,Err(PgTransactionError::Persistence(e)) if e.code=="MARKETING_PROVENANCE_LOCKS_REQUIRED"));
        let company=CompanyId::new("company").unwrap();
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move |c| {
            let company=company.clone();
            Box::pin(async move {
                lock_campaign_attribution_sources(c).await?;
                let mut reader=PgMarketingCampaignRead::new(c);
                let all=reader.attribution_with_provenance(&company,Some("branch"),None,1001,CandidateProvenance::All).await?;
                let filtered=reader.attribution_with_provenance(&company,Some("branch"),None,1001,CandidateProvenance::CapturedForm).await?;
                assert_eq!(all.iter().map(|row|row.captured).sum::<i64>(),6);
                assert_eq!(filtered.len(),1);
                assert_eq!((&*filtered[0].campaign,&*filtered[0].source,filtered[0].captured,filtered[0].pending,filtered[0].handed_off,filtered[0].rejected),("AUTUMN","web",2,1,1,0));
                let selected=actor("company",Some("branch"),None,&["records.read"]);
                for (mode,expected) in [(MarketingAttributionMode::Disabled,6_i64),(MarketingAttributionMode::Shadow,2),(MarketingAttributionMode::Enforced,2)] {
                    let mut packages=Packages(mode==MarketingAttributionMode::Enforced);
                    let report=marketing_campaign_report(&selected,&mut reader,&mut packages,NOW,mode).await?;
                    assert_eq!(report.attribution.iter().map(|row|row.captured).sum::<i64>(),expected);
                    if mode==MarketingAttributionMode::Disabled { assert!(report.purchase.is_none()); }
                    else { let purchase=report.purchase.unwrap();assert!(purchase.history);assert!(purchase.new_work);assert!(purchase.settle_existing);assert_eq!(purchase.revision,"7"); }
                }
                let department=reader.attribution_with_provenance(&company,Some("branch"),Some("department"),1001,CandidateProvenance::CapturedForm).await?;
                assert!(department.is_empty());
                let foreign=reader.attribution_with_provenance(&CompanyId::new("foreign").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await?;
                assert_eq!(foreign.len(),1);assert_eq!(foreign[0].campaign,"FOREIGN");assert_eq!(foreign[0].captured,1);
                Ok(())
            })
        }).await.unwrap();
    }).await;
}

#[tokio::test]
async fn provenance_rejects_each_mismatched_source_and_handles_ineligible_malformed_json() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at) VALUES ('form-wrong-action','company','form','Wrong action','{}','{}','active','owner','reviewer',1,10,10),('form-wrong-version','company','form','Wrong version','{}','{}','active','owner','reviewer',2,10,10),('form-wrong-kind','company','content','Wrong kind','{}','{}','active','owner','reviewer',1,10,10); INSERT INTO marketing_asset_events(id,asset_id,version,action,actor_id,reason,status,created_at) VALUES ('activation-good','form-company',1,'activate','reviewer','approved','active',10),('activation-wrong-action','form-wrong-action',1,'reject','reviewer','reason','active',10),('activation-wrong-version','form-wrong-version',2,'activate','reviewer','reason','active',10),('activation-wrong-kind','form-wrong-kind',1,'activate','reviewer','reason','active',10),('activation-foreign','form-foreign',1,'activate','reviewer','reason','active',10)").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at) VALUES ('form-wrong-status','company','form','Wrong status','{}','{}','active','owner','reviewer',1,10,10),('form-no-activation','company','form','No activation','{}','{}','active','owner','reviewer',1,10,10); INSERT INTO marketing_asset_events(id,asset_id,version,action,actor_id,reason,status,created_at) VALUES ('activation-wrong-status','form-wrong-status',1,'activate','reviewer','reason','retired',10)").execute(pool.raw()).await.unwrap();
        for (id,form,consent,json) in [
            ("valid","form-company","consent",r#"{"campaign":"VALID","source":"web"}"#),
            ("wrong-action","form-wrong-action","consent",r#"{"campaign":"WRONG-ACTION"}"#),
            ("wrong-version","form-wrong-version","consent",r#"{"campaign":"WRONG-VERSION"}"#),
            ("wrong-kind","form-wrong-kind","consent",r#"{"campaign":"WRONG-KIND"}"#),
            ("wrong-status","form-wrong-status","consent",r#"{"campaign":"WRONG-STATUS"}"#),
            ("no-activation","form-no-activation","consent",r#"{"campaign":"NO-ACTIVATION"}"#),
            ("short-consent","form-company"," x ",r#"{"campaign":"SHORT-CONSENT"}"#),
            ("wrong-submission","form-company","consent",r#"{"campaign":"WRONG-SUBMISSION"}"#),
            ("wrong-event-type","form-company","consent",r#"{"campaign":"WRONG-EVENT-TYPE"}"#),
            ("foreign-form","form-foreign","consent",r#"{"campaign":"FOREIGN-FORM"}"#),
            ("bad-json","form-company","consent","{not-json"),
        ] {
            sqlx::query("INSERT INTO marketing_lead_candidates(id,company_id,branch_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,created_by,version,created_at,updated_at) VALUES($1,'company','branch',$2,1,'name','contact',$3,$4,'brief','pending','owner',1,10,10)")
                .bind(id).bind(form).bind(consent).bind(json).execute(pool.raw()).await.unwrap();
            if id != "bad-json" {
                sqlx::query("INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES($1,$2,$3,$4,'web','evidence','owner',10)")
                    .bind(format!("submission-{id}")).bind(id).bind(if id=="wrong-submission" {2_i64} else {1_i64}).bind(if id=="wrong-event-type" {"email"} else {"form_submit"}).execute(pool.raw()).await.unwrap();
            }
        }
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move |c| Box::pin(async move {
            lock_campaign_attribution_sources(c).await?;
            let mut reader=PgMarketingCampaignRead::new(c);let company=CompanyId::new("company").unwrap();
            let all=reader.attribution_with_provenance(&company,None,None,1001,CandidateProvenance::All).await.unwrap_err();
            assert_eq!(business(all).code,"MARKETING_CAMPAIGN_SOURCE_INVALID");
            let eligible=reader.attribution_with_provenance(&company,None,None,1001,CandidateProvenance::CapturedForm).await?;
            assert_eq!(eligible.len(),1);assert_eq!(eligible[0].campaign,"VALID");assert_eq!(eligible[0].captured,1);
            Ok(())
        })).await;result.unwrap();
        sqlx::query("INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES('submission-bad-json','bad-json',1,'form_submit','web','evidence','owner',10)").execute(pool.raw()).await.unwrap();
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move |c| Box::pin(async move {
            lock_campaign_attribution_sources(c).await?;
            let mut reader=PgMarketingCampaignRead::new(c);
            reader.attribution_with_provenance(&CompanyId::new("company").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await?;
            Ok(())
        })).await.unwrap_err();
        assert_eq!(business(result).code,"MARKETING_CAMPAIGN_SOURCE_INVALID");
    }).await;
}

impl MarketingCampaignEntitlementPort for Packages {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> MarketingCampaignFuture<'a, MarketingCampaignEntitlement, Self::Error> {
        Box::pin(async {
            Ok(MarketingCampaignEntitlement {
                enforced: self.0,
                history_allowed: true,
                new_work: true,
                settle_existing: true,
                revision: "7".into(),
            })
        })
    }
}

struct DeniedPackage;

impl MarketingCampaignEntitlementPort for DeniedPackage {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> MarketingCampaignFuture<'a, MarketingCampaignEntitlement, Self::Error> {
        Box::pin(async {
            Ok(MarketingCampaignEntitlement {
                enforced: true,
                history_allowed: false,
                new_work: false,
                settle_existing: false,
                revision: "9".into(),
            })
        })
    }
}

#[tokio::test]
async fn persisted_campaign_names_preserve_zod_content_and_company_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let names = ["Line\nbreak".to_owned(), "😀".repeat(80), "\u{85}A".to_owned()];
        for (index, name) in names.iter().enumerate() {
            sqlx::query("INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES($1,'company',$2,$3,'owner',20)")
                .bind(format!("boundary-{index}"))
                .bind(format!("BOUNDARY-{index}"))
                .bind(name)
                .execute(pool.raw()).await.unwrap();
        }
        // A foreign historical row outside the name budget must not affect the
        // selected Company's source validation.
        sqlx::query("INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES('foreign-boundary','foreign','FOREIGN-BOUNDARY',$1,'owner',20)")
            .bind("😀".repeat(81)).execute(pool.raw()).await.unwrap();
        let current = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
            let current = current.clone();
            let names = names.clone();
            Box::pin(async move {
                let mut reader = PgMarketingCampaignRead::new(c);
                let mut packages = Packages(false);
                let report = marketing_campaign_report(&current, &mut reader, &mut packages, NOW, MarketingAttributionMode::Disabled).await?;
                for (index, name) in names.iter().enumerate() {
                    let row = report.campaigns.iter().find(|row| row.id == format!("boundary-{index}")).expect("persisted campaign missing");
                    assert_eq!(&row.name, name);
                    assert_eq!(row.company_id, "company");
                }
                assert!(!report.campaigns.iter().any(|row| row.id == "foreign-boundary"));
                Ok(())
            })
        }).await.unwrap();
    }).await;
}

#[tokio::test]
async fn report_filters_company_branch_department_and_groups_attribution() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let company = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = company.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let report = marketing_campaign_report(
                        &current,
                        &mut reader,
                        &mut packages,
                        NOW,
                        MarketingAttributionMode::Disabled,
                    )
                    .await?;
                    assert_eq!(
                        report
                            .campaigns
                            .iter()
                            .map(|row| row.code.as_str())
                            .collect::<Vec<_>>(),
                        vec!["AUTUMN", "SUMMER", "WINTER"]
                    );
                    assert!(
                        report
                            .campaigns
                            .iter()
                            .all(|row| row.company_id == "company")
                    );
                    assert_eq!(
                        report
                            .attribution
                            .iter()
                            .map(|row| (
                                row.campaign.as_str(),
                                row.source.as_str(),
                                row.captured,
                                row.pending,
                                row.handed_off,
                                row.rejected
                            ))
                            .collect::<Vec<_>>(),
                        vec![
                            ("AUTUMN", "web", 4, 2, 1, 1),
                            ("AUTUMN", "email", 1, 1, 0, 0),
                            ("SUMMER", "social", 1, 1, 0, 0),
                            ("Unspecified", "Unspecified", 1, 1, 0, 0),
                        ]
                    );
                    assert!(report.purchase.is_none());
                    Ok(())
                })
            })
            .await
            .unwrap();

        let branch = actor("company", Some("branch"), None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = branch.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let report = marketing_campaign_report(
                        &current,
                        &mut reader,
                        &mut packages,
                        NOW,
                        MarketingAttributionMode::Disabled,
                    )
                    .await?;
                    assert!(
                        report
                            .attribution
                            .iter()
                            .all(|row| row.campaign != "other-branch")
                    );
                    assert_eq!(
                        report
                            .attribution
                            .iter()
                            .map(|row| (row.campaign.as_str(), row.captured))
                            .collect::<Vec<_>>(),
                        vec![
                            ("AUTUMN", 3),
                            ("AUTUMN", 1),
                            ("SUMMER", 1),
                            ("Unspecified", 1)
                        ]
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();

        let department = actor(
            "company",
            Some("branch"),
            Some("department"),
            &["records.read"],
        );
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = department.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let report = marketing_campaign_report(
                        &current,
                        &mut reader,
                        &mut packages,
                        NOW,
                        MarketingAttributionMode::Disabled,
                    )
                    .await?;
                    assert_eq!(report.attribution.len(), 1);
                    assert_eq!(report.attribution[0].campaign, "SUMMER");
                    assert_eq!(report.attribution[0].source, "social");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let foreign = actor("foreign", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let report = marketing_campaign_report(
                        &current,
                        &mut reader,
                        &mut packages,
                        NOW,
                        MarketingAttributionMode::Disabled,
                    )
                    .await?;
                    assert_eq!(report.campaigns.len(), 1);
                    assert_eq!(report.campaigns[0].id, "campaign-foreign");
                    assert_eq!(report.attribution.len(), 1);
                    assert_eq!(report.attribution[0].campaign, "FOREIGN");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn invalid_attribution_json_fails_closed_and_rolls_back() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) VALUES \
                         ('candidate-invalid','company','branch',NULL,'form-company',1,'Synthetic invalid','Synthetic channel','Synthetic consent','{not-json','Synthetic brief','pending',NULL,'owner',NULL,1,900,900);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let error =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW, MarketingAttributionMode::Disabled)
                            .await
                            .unwrap_err();
                    let error = business(error);
                    assert_eq!(error.code, "MARKETING_CAMPAIGN_SOURCE_INVALID");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let persisted: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM marketing_lead_candidates WHERE id='candidate-invalid'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted, 0);
    })
    .await;
}

#[tokio::test]
async fn transaction_guards_rollback_and_persisted_readback_hold() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", None, None, &["records.read"]);
        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgMarketingCampaignRead::new(&mut outside)
            .campaigns(&CompanyId::new("company").unwrap(), 501)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "MARKETING_CAMPAIGN_READ_TRANSACTION_REQUIRED"
        ));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let error = PgMarketingCampaignRead::new(&mut default_isolation)
            .attribution(&CompanyId::new("company").unwrap(), None, None, 1001)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "MARKETING_CAMPAIGN_READ_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();

        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM marketing_campaigns")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let read_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_owner.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let report =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW, MarketingAttributionMode::Disabled)
                            .await?;
                    assert_eq!(report.campaigns.len(), 3);
                    Ok(())
                })
            })
            .await
            .unwrap();
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM marketing_campaigns")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(before, after);

        let rollback_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = rollback_owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES \
                         ('campaign-rollback','company','ROLLBACK','Rollback campaign','owner',9);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let report =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW, MarketingAttributionMode::Disabled)
                            .await?;
                    assert!(
                        report
                            .campaigns
                            .iter()
                            .any(|row| row.id == "campaign-rollback")
                    );
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM marketing_campaigns WHERE id='campaign-rollback'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(persisted, 0);
    })
    .await;
}

#[tokio::test]
async fn permission_and_package_denial_fail_before_source_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let denied = actor("company", None, None, &[]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = denied.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "ALTER TABLE marketing_campaigns RENAME TO marketing_campaigns_hidden",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let error = marketing_campaign_report(
                        &current,
                        &mut reader,
                        &mut packages,
                        NOW,
                        MarketingAttributionMode::Disabled,
                    )
                    .await
                    .unwrap_err();
                    let error = business(error);
                    assert_eq!(error.code, "MARKETING_CAMPAIGN_PERMISSION_REQUIRED");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let restored: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_class WHERE relname='marketing_campaigns')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(restored);

        let owner = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "ALTER TABLE marketing_campaigns RENAME TO marketing_campaigns_hidden",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = DeniedPackage;
                    let error = marketing_campaign_report(
                        &current,
                        &mut reader,
                        &mut packages,
                        NOW,
                        MarketingAttributionMode::Enforced,
                    )
                    .await
                    .unwrap_err();
                    let error = business(error);
                    assert_eq!(error.code, "PACKAGE_REQUIRED");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
    })
    .await;
}

#[tokio::test]
async fn campaign_source_bounds_reject_excess_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) \
             SELECT 'bulk-'||n,'company','B'||lpad(n::text,4,'0'),'Bulk campaign '||n,'owner',100 \
             FROM generate_series(1,498) AS n;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let owner = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages(false);
                    let error = marketing_campaign_report(
                        &current,
                        &mut reader,
                        &mut packages,
                        NOW,
                        MarketingAttributionMode::Disabled,
                    )
                    .await
                    .unwrap_err();
                    let error = business(error);
                    assert_eq!(error.code, "MARKETING_CAMPAIGN_TOO_LARGE");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

mod lead_candidate_commands {
    use super::fixture;
    use super::setup;
    use erpx_server_marketing::public::lead_candidate::{
        MarketingCampaignCreateInput, MarketingCrmHandoffPort, MarketingCrmLeadRequest,
        MarketingCrmLeadResult, MarketingLeadActionInput, MarketingLeadActor,
        MarketingLeadCandidateInput, PgMarketingLead, asset_profiles, create_asset,
        create_campaign, create_candidate, handoff_candidate, lead_candidates, lead_detail,
        record_engagement,
    };
    use erpx_server_runtime::public::postgres::{
        PgTransactionError, PgUnitOfWork, PgWorkFuture, TransactionOptions,
    };
    use erpx_server_shared::public::{BusinessError, RecordId, TimestampMillis};
    use sqlx::PgConnection;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    #[derive(Default)]
    struct RecordingCrm {
        calls: Arc<Mutex<Vec<MarketingCrmLeadRequest>>>,
    }

    impl MarketingCrmHandoffPort for RecordingCrm {
        fn create_lead<'a>(
            &'a mut self,
            _c: &'a mut PgConnection,
            request: &'a MarketingCrmLeadRequest,
            _key: &'a str,
            _now: i64,
        ) -> PgWorkFuture<'a, MarketingCrmLeadResult, BusinessError> {
            Box::pin(async move {
                self.calls.lock().unwrap().push(request.clone());
                Ok(MarketingCrmLeadResult {
                    lead_id: "lead-crm-native-1".into(),
                })
            })
        }
    }

    fn lead_actor(user: &str, assignment: &str) -> MarketingLeadActor {
        MarketingLeadActor {
            user_id: user.into(),
            company_id: "company".into(),
            branch_id: Some(RecordId::new("branch").unwrap()),
            department_id: None,
            assignment_id: assignment.into(),
            role: Some("sales_manager".into()),
            authority_revision: Some(1),
            assignment_version: Some(1),
            permissions: vec!["records.read".into(), "records.write".into()],
            modules: vec!["marketing".into()],
            expires_at: NOW.get() + 60_000,
        }
    }

    fn candidate_input() -> MarketingLeadCandidateInput {
        let mut attribution = BTreeMap::new();
        attribution.insert("campaign".into(), "AUTUMN".into());
        attribution.insert("source".into(), "web".into());
        attribution.insert("campaignId".into(), "campaign-autumn".into());
        MarketingLeadCandidateInput {
            form_id: "form-company".into(),
            form_version: 1,
            branch_id: "branch".into(),
            contact_name: "Native candidate".into(),
            contact_detail: "native@example.test".into(),
            consent_evidence: "Signed web consent".into(),
            attribution,
            brief: "Requested a call".into(),
        }
    }

    fn action(action: &str, expected_version: i64) -> MarketingLeadActionInput {
        MarketingLeadActionInput {
            action: action.into(),
            expected_version,
            reason: "Reviewed current candidate".into(),
            event_type: (action == "record-engagement").then(|| "email_open".into()),
            channel: (action == "record-engagement").then(|| "email".into()),
            due_date: (action == "handoff").then(|| "2026-11-20".into()),
        }
    }

    async fn add_iam(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
             ('company','company','company','Company',NULL,'company',NULL,'active',1),\
             ('branch','branch','branch','Branch','company','company','branch','active',1);\
             INSERT INTO crm_leads(id,company_id,branch_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
             ('lead-crm-native-1','company','branch','Marketing lead','Native candidate','native@example.test','Marketing','Requested a call','reviewer','2026-11-20','open',1,'reviewer',10,10);\
             INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
             ('company','owner','local:owner','active','local',1),\
             ('company','reviewer','local:reviewer','active','local',1),\
             ('company','stale','local:stale','active','local',1);\
             INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
             ('assignment-owner','owner','sales_manager','[\"marketing\"]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
             ('assignment-reviewer','reviewer','sales_manager','[\"marketing\"]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
             ('assignment-stale','stale','sales_manager','[\"marketing\"]','company','branch',0,9000000000000,1,'branch','branch','manual');\
             INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
             ('form-files','company','form','Files form','{\"consentRequired\":true,\"contentAssetId\":\"content-1\",\"contentVersion\":1}','{}','active','owner','reviewer',1,10,10),\
             ('form-inactive','company','form','Inactive form','{\"consentRequired\":true}','{}','draft','owner',NULL,1,10,10),\
             ('content-1','company','content','Approved content','{}','{\"headline\":\"Headline\"}','active','owner','reviewer',1,10,10),\
             ('domain-1','company','domain','Domain readiness','{}','{}','draft','owner',NULL,1,10,10),\
             ('provider-1','company','provider','Provider readiness','{}','{}','draft','owner',NULL,1,10,10),\
             ('content-foreign','foreign','content','Foreign content','{}','{}','active','owner','reviewer',1,10,10);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn campaign_create_is_idempotent_and_rejects_duplicate_codes() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            add_iam(&pool).await;
            let actor = lead_actor("owner", "assignment-owner");
            let input = MarketingCampaignCreateInput {
                code: "AUTUMN2".into(),
                name: "Autumn follow-up".into(),
            };
            let receipt = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let input = input.clone();
                    move |c| {
                        let actor = actor.clone();
                        let input = input.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_campaign(&actor, &input, "campaign-native-key", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(receipt.id, "campaign-campaign-native-key");
            assert_eq!(receipt.code.as_deref(), Some("AUTUMN2"));
            assert_eq!(receipt.name.as_deref(), Some("Autumn follow-up"));
            assert_eq!(receipt.version, 1);
            let row: (String, String, i64) = sqlx::query_as(
                "SELECT code,name,version FROM marketing_campaigns WHERE id='campaign-campaign-native-key'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row, ("AUTUMN2".into(), "Autumn follow-up".into(), 1));
            let evidence: (i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='campaign-native-key'),\
                 (SELECT count(*) FROM audit_events WHERE command_id='campaign-native-key'),\
                 (SELECT count(*) FROM outbox_events WHERE command_id='campaign-native-key')",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(evidence, (1, 1, 1));

            let replay = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let input = input.clone();
                    move |c| {
                        let actor = actor.clone();
                        let input = input.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_campaign(&actor, &input, "campaign-native-key", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(replay, receipt);
            let duplicate = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let input = input.clone();
                    move |c| {
                        let actor = actor.clone();
                        let input = input.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_campaign(&actor, &input, "campaign-other-key", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(duplicate.code, "MARKETING_CAMPAIGN_CODE_TAKEN");
            let changed = MarketingCampaignCreateInput {
                code: "AUTUMN2".into(),
                name: "Different name".into(),
            };
            let conflict = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let changed = changed.clone();
                    move |c| {
                        let actor = actor.clone();
                        let changed = changed.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_campaign(&actor, &changed, "campaign-native-key", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(conflict.code, "IDEMPOTENCY_CONFLICT");
        })
        .await;
    }

    #[tokio::test]
    async fn candidate_create_enforces_form_assets_and_persists_attribution() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            add_iam(&pool).await;
            let actor = lead_actor("owner", "assignment-owner");
            let input = candidate_input();
            let receipt = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let input = input.clone();
                    move |c| {
                        let actor = actor.clone();
                        let input = input.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_candidate(&actor, &input, "candidate-native-key", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(receipt.id, "marketing-candidate-candidate-native-key");
            assert_eq!(receipt.status, "pending");
            assert_eq!(receipt.version, 1);
            let row: (String, i64, String, String, String, i64) = sqlx::query_as(
                "SELECT branch_id,form_version,contact_name,consent_evidence,attribution_json,version \
                 FROM marketing_lead_candidates WHERE id='marketing-candidate-candidate-native-key'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row.0, "branch");
            assert_eq!(row.1, 1);
            assert_eq!(row.2, "Native candidate");
            assert_eq!(row.3, "Signed web consent");
            assert_eq!(
                row.4,
                "{\"campaign\":\"AUTUMN\",\"campaignId\":\"campaign-autumn\",\"source\":\"web\"}"
            );
            assert_eq!(row.5, 1);
            let event: (i64, String, String) = sqlx::query_as(
                "SELECT version,event_type,channel FROM marketing_engagement_events \
                 WHERE candidate_id='marketing-candidate-candidate-native-key'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(event, (1, "form_submit".into(), "web".into()));

            let replay = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let input = input.clone();
                    move |c| {
                        let actor = actor.clone();
                        let input = input.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_candidate(&actor, &input, "candidate-native-key", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(replay, receipt);

            let mut files = candidate_input();
            files.form_id = "form-files".into();
            let boundary = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let files = files.clone();
                    move |c| {
                        let actor = actor.clone();
                        let files = files.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_candidate(&actor, &files, "candidate-files", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(boundary.code, "MARKETING_FORM_READINESS_PORT_REQUIRED");

            let mut inactive = candidate_input();
            inactive.form_id = "form-inactive".into();
            let inactive_error = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let inactive = inactive.clone();
                    move |c| {
                        let actor = actor.clone();
                        let inactive = inactive.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_candidate(&actor, &inactive, "candidate-inactive", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(inactive_error.code, "MARKETING_FORM_NOT_READY");

            let mut wrong_branch = candidate_input();
            wrong_branch.branch_id = "other-branch".into();
            let branch_error = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let wrong_branch = wrong_branch.clone();
                    move |c| {
                        let actor = actor.clone();
                        let wrong_branch = wrong_branch.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_candidate(&actor, &wrong_branch, "candidate-branch", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(branch_error.code, "MARKETING_BRANCH_SCOPE_INVALID");
        })
        .await;
    }

    #[tokio::test]
    async fn engagement_requires_expected_version_and_persists_history() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            add_iam(&pool).await;
            let actor = lead_actor("owner", "assignment-owner");
            let receipt = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    move |c| {
                        let actor = actor.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            record_engagement(
                                &actor,
                                "candidate-1",
                                &action("record-engagement", 1),
                                "engagement-native",
                                &mut repository,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(receipt.id, "candidate-1");
            assert_eq!(receipt.status, "pending");
            assert_eq!(receipt.version, 2);
            let row: (i64, String) = sqlx::query_as(
                "SELECT version,status FROM marketing_lead_candidates WHERE id='candidate-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row, (2, "pending".into()));
            let history: (i64, String, String, String) = sqlx::query_as(
                "SELECT version,event_type,channel,evidence FROM marketing_engagement_events \
                 WHERE candidate_id='candidate-1' AND version=2",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                history,
                (
                    2,
                    "email_open".into(),
                    "email".into(),
                    "Reviewed current candidate".into()
                )
            );
            let stale = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    move |c| {
                        let actor = actor.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            record_engagement(
                                &actor,
                                "candidate-1",
                                &action("record-engagement", 1),
                                "engagement-stale",
                                &mut repository,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(stale.code, "MARKETING_CANDIDATE_CHANGED");
        })
        .await;
    }

    #[tokio::test]
    async fn handoff_requires_independence_uses_crm_port_and_replays_once() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            add_iam(&pool).await;
            let owner = lead_actor("owner", "assignment-owner");
            let reviewer = lead_actor("reviewer", "assignment-reviewer");
            let created = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let owner = owner.clone();
                    move |c| {
                        let owner = owner.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_candidate(
                                &owner,
                                &candidate_input(),
                                "candidate-handoff",
                                &mut repository,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(created.version, 1);
            let independent = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let owner = owner.clone();
                    move |c| {
                        let owner = owner.clone();
                        let mut crm = RecordingCrm::default();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            handoff_candidate(
                                &owner,
                                "marketing-candidate-candidate-handoff",
                                &action("handoff", 1),
                                "handoff-owner",
                                &mut repository,
                                &mut crm,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(independent.code, "MARKETING_INDEPENDENT_REVIEW_REQUIRED");

            let calls = Arc::new(Mutex::new(Vec::new()));
            let handoff = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let reviewer = reviewer.clone();
                    let calls = calls.clone();
                    move |c| {
                        let reviewer = reviewer.clone();
                        let calls = calls.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            let mut crm = RecordingCrm { calls };
                            handoff_candidate(
                                &reviewer,
                                "marketing-candidate-candidate-handoff",
                                &action("handoff", 1),
                                "handoff-reviewer",
                                &mut repository,
                                &mut crm,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(handoff.id, "marketing-candidate-candidate-handoff");
            assert_eq!(handoff.status, "handed_off");
            assert_eq!(handoff.version, 2);
            assert_eq!(handoff.crm_lead_id.as_deref(), Some("lead-crm-native-1"));
            let row: (String, Option<String>, String, i64) = sqlx::query_as(
                "SELECT status,crm_lead_id,reviewed_by,version FROM marketing_lead_candidates \
                 WHERE id='marketing-candidate-candidate-handoff'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                row,
                (
                    "handed_off".into(),
                    Some("lead-crm-native-1".into()),
                    "reviewer".into(),
                    2
                )
            );
            let event: (i64, String, String) = sqlx::query_as(
                "SELECT version,event_type,channel FROM marketing_engagement_events \
                 WHERE candidate_id='marketing-candidate-candidate-handoff' AND version=2",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(event, (2, "crm_handoff".into(), "internal".into()));
            {
                let recorded = calls.lock().unwrap();
                assert_eq!(recorded.len(), 1);
                assert_eq!(recorded[0].branch_id, "branch");
                assert_eq!(recorded[0].source_kind, "form");
                assert_eq!(
                    recorded[0].source_id,
                    "marketing-candidate-candidate-handoff"
                );
                assert_eq!(recorded[0].campaign_id.as_deref(), Some("campaign-autumn"));
            }

            let replay_calls = Arc::new(Mutex::new(Vec::new()));
            let replay = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let reviewer = reviewer.clone();
                    let replay_calls = replay_calls.clone();
                    move |c| {
                        let reviewer = reviewer.clone();
                        let replay_calls = replay_calls.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            let mut crm = RecordingCrm {
                                calls: replay_calls,
                            };
                            handoff_candidate(
                                &reviewer,
                                "marketing-candidate-candidate-handoff",
                                &action("handoff", 1),
                                "handoff-reviewer",
                                &mut repository,
                                &mut crm,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(replay, handoff);
            assert!(replay_calls.lock().unwrap().is_empty());
            let receipt_count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM command_receipts WHERE command_id='handoff-reviewer'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(receipt_count, 1);
        })
        .await;
    }

    #[tokio::test]
    async fn handoff_late_failure_rolls_back_marketing_rows_and_retry_persists_once() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            add_iam(&pool).await;
            let owner = lead_actor("owner", "assignment-owner");
            let reviewer = lead_actor("reviewer", "assignment-reviewer");
            PgUnitOfWork::new(pool.clone())
                .run::<(), BusinessError, _>(TransactionOptions::default(), {
                    let owner = owner.clone();
                    move |c| {
                        let owner = owner.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_candidate(
                                &owner,
                                &candidate_input(),
                                "candidate-rollback",
                                &mut repository,
                                c,
                                NOW,
                            )
                            .await?;
                            Ok(())
                        })
                    }
                })
                .await
                .unwrap();
            sqlx::raw_sql(
                "CREATE FUNCTION fail_marketing_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN IF NEW.command_id='handoff-rollback' THEN \
                 RAISE EXCEPTION 'blocked marketing outbox'; END IF; RETURN NEW; END $$; \
                 CREATE TRIGGER fail_marketing_outbox BEFORE INSERT ON outbox_events \
                 FOR EACH ROW EXECUTE FUNCTION fail_marketing_outbox();",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let calls = Arc::new(Mutex::new(Vec::new()));
            let failed = PgUnitOfWork::new(pool.clone())
                .run::<(), BusinessError, _>(TransactionOptions::default(), {
                    let reviewer = reviewer.clone();
                    let calls = calls.clone();
                    move |c| {
                        let reviewer = reviewer.clone();
                        let calls = calls.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            let mut crm = RecordingCrm { calls };
                            handoff_candidate(
                                &reviewer,
                                "marketing-candidate-candidate-rollback",
                                &action("handoff", 1),
                                "handoff-rollback",
                                &mut repository,
                                &mut crm,
                                c,
                                NOW,
                            )
                            .await?;
                            Ok(())
                        })
                    }
                })
                .await;
            assert!(failed.is_err());
            let row: (String, i64, i64, i64) = sqlx::query_as(
                "SELECT status,(SELECT count(*) FROM marketing_engagement_events WHERE candidate_id='marketing-candidate-candidate-rollback'),\
                 (SELECT count(*) FROM command_receipts WHERE command_id='handoff-rollback'),\
                 (SELECT count(*) FROM audit_events WHERE command_id='handoff-rollback') \
                 FROM marketing_lead_candidates WHERE id='marketing-candidate-candidate-rollback'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(row, ("pending".into(), 1, 0, 0));
            sqlx::raw_sql(
                "DROP TRIGGER fail_marketing_outbox ON outbox_events; \
                 DROP FUNCTION fail_marketing_outbox();",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let retry = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let reviewer = reviewer.clone();
                    move |c| {
                        let reviewer = reviewer.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            let mut crm = RecordingCrm::default();
                            handoff_candidate(
                                &reviewer,
                                "marketing-candidate-candidate-rollback",
                                &action("handoff", 1),
                                "handoff-rollback",
                                &mut repository,
                                &mut crm,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(retry.status, "handed_off");
            assert_eq!(retry.version, 2);
        })
        .await;
    }

    #[tokio::test]
    async fn reads_are_company_scoped_and_asset_profiles_keep_owner_boundaries() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            add_iam(&pool).await;
            let actor = lead_actor("owner", "assignment-owner");
            PgUnitOfWork::new(pool.clone())
                .run::<(), BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    move |c| {
                        let actor = actor.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            let assets = asset_profiles(&actor, &mut repository, c, NOW).await?;
                            let kinds = assets
                                .assets
                                .iter()
                                .map(|asset| asset.kind.as_str())
                                .collect::<std::collections::BTreeSet<_>>();
                            assert!(kinds.contains("form"));
                            assert!(kinds.contains("content"));
                            assert!(kinds.contains("domain"));
                            assert!(kinds.contains("provider"));
                            assert!(
                                assets
                                    .assets
                                    .iter()
                                    .all(|asset| asset.company_id == "company")
                            );
                            let candidates =
                                lead_candidates(&actor, &mut repository, c, NOW).await?;
                            assert!(
                                candidates
                                    .iter()
                                    .all(|candidate| candidate.company_id == "company")
                            );
                            assert!(
                                candidates
                                    .iter()
                                    .all(|candidate| candidate.branch_id == "branch")
                            );
                            assert!(
                                !candidates
                                    .iter()
                                    .any(|candidate| candidate.id == "candidate-5")
                            );
                            let detail =
                                lead_detail(&actor, "candidate-1", &mut repository, c, NOW).await?;
                            assert_eq!(detail.candidate.id, "candidate-1");
                            assert_eq!(detail.candidate.form_name, "Lead form");
                            let missing =
                                lead_detail(&actor, "candidate-foreign", &mut repository, c, NOW)
                                    .await;
                            assert!(missing.is_err());
                            Ok(())
                        })
                    }
                })
                .await
                .unwrap();

            let stale = lead_actor("stale", "assignment-stale");
            let revoked = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let stale = stale.clone();
                    move |c| {
                        let stale = stale.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_campaign(
                                &stale,
                                &MarketingCampaignCreateInput {
                                    code: "STALE".into(),
                                    name: "Stale attempt".into(),
                                },
                                "campaign-stale",
                                &mut repository,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(revoked.code, "MARKETING_PERMISSION_REQUIRED");
        })
        .await;
    }

    #[tokio::test]
    async fn command_requires_a_serializable_owner_transaction() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            add_iam(&pool).await;
            let mut c = pool.raw().begin().await.unwrap();
            let mut repository = PgMarketingLead;
            let error = create_campaign(
                &lead_actor("owner", "assignment-owner"),
                &MarketingCampaignCreateInput {
                    code: "SERIAL".into(),
                    name: "Serializable check".into(),
                },
                "campaign-serial",
                &mut repository,
                &mut c,
                NOW,
            )
            .await
            .unwrap_err();
            let PgTransactionError::Persistence(error) = error else {
                panic!("expected serializable refusal");
            };
            assert_eq!(error.code, "MARKETING_SERIALIZABLE_REQUIRED");
            c.rollback().await.unwrap();
        })
        .await;
    }

    #[tokio::test]
    async fn asset_create_persists_profiles_and_enforces_form_consent() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            add_iam(&pool).await;
            let actor = lead_actor("owner", "assignment-owner");
            let content =
                erpx_server_marketing::public::lead_candidate::MarketingAssetCreateInput {
                    kind: "content".into(),
                    name: "Approved content draft".into(),
                    configuration_json: "{}".into(),
                    preview_json: "{\"headline\":\"Draft\"}".into(),
                };
            let profile = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let content = content.clone();
                    move |c| {
                        let actor = actor.clone();
                        let content = content.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_asset(&actor, &content, "asset-content", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(profile.id, "marketing-content-asset-content");
            assert_eq!(profile.status, "draft");
            assert_eq!(profile.version, 1);
            let replay = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let content = content.clone();
                    move |c| {
                        let actor = actor.clone();
                        let content = content.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_asset(&actor, &content, "asset-content", &mut repository, c, NOW)
                                .await
                        })
                    }
                })
                .await
                .unwrap();
            assert_eq!(replay, profile);

            let form = erpx_server_marketing::public::lead_candidate::MarketingAssetCreateInput {
                kind: "form".into(),
                name: "Consented form".into(),
                configuration_json: "{\"consentRequired\":true}".into(),
                preview_json: "{}".into(),
            };
            PgUnitOfWork::new(pool.clone())
                .run::<(), BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let form = form.clone();
                    move |c| {
                        let actor = actor.clone();
                        let form = form.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_asset(&actor, &form, "asset-form", &mut repository, c, NOW)
                                .await?;
                            Ok(())
                        })
                    }
                })
                .await
                .unwrap();
            let missing_consent =
                erpx_server_marketing::public::lead_candidate::MarketingAssetCreateInput {
                    kind: "form".into(),
                    name: "No consent".into(),
                    configuration_json: "{}".into(),
                    preview_json: "{}".into(),
                };
            let denied = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), {
                    let actor = actor.clone();
                    let missing_consent = missing_consent.clone();
                    move |c| {
                        let actor = actor.clone();
                        let missing_consent = missing_consent.clone();
                        Box::pin(async move {
                            let mut repository = PgMarketingLead;
                            create_asset(
                                &actor,
                                &missing_consent,
                                "asset-form-invalid",
                                &mut repository,
                                c,
                                NOW,
                            )
                            .await
                        })
                    }
                })
                .await
                .map_err(business)
                .unwrap_err();
            assert_eq!(denied.code, "MARKETING_CONSENT_REQUIRED");
            let assets: (i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM marketing_assets \
                   WHERE id IN ('marketing-content-asset-content','marketing-form-asset-form')),\
                 (SELECT count(*) FROM command_receipts WHERE command_id='asset-content')",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(assets, (2, 1));
        })
        .await;
    }

    fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
        match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected owner business failure, got {other:?}"),
        }
    }
}
