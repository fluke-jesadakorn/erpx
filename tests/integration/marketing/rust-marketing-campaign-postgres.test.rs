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
        let after=source.attribution_with_provenance(&CompanyId::new("company").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await.unwrap();assert_eq!(after,before);drop(source);tx.commit().await.unwrap();writer.await.unwrap();
        let mut fresh=reader.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_campaign_attribution_sources(&mut fresh).await.unwrap();let mut source=PgMarketingCampaignRead::new(&mut fresh);let rows=source.attribution_with_provenance(&CompanyId::new("company").unwrap(),None,None,1001,CandidateProvenance::CapturedForm).await.unwrap();assert_eq!(rows[0].captured,2);drop(source);fresh.rollback().await.unwrap();
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
