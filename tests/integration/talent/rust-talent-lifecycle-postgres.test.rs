//! Native Talent candidate/privacy, application/proposal, offer/handoff over
//! real migrated PostgreSQL: current IAM authority, Company/branch scope,
//! version CAS, idempotency, Money snapshot port, audit/outbox and readback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use erpx_server_talent::public::candidate::{
    CandidateRequest, PgTalentCandidates, PrivacyAction, PrivacyRequest, TalentActor,
    candidate_privacy_workspace, change_candidate_privacy, create_candidate,
};
use erpx_server_talent::public::offer::{
    OfferAction, OfferChange, OfferMoneyRequest, OfferMoneySnapshot, OfferRequest,
    PgOfferMoneySnapshotCapture, PgTalentOffers, change_offer, create_offer, hire_handoff_source,
};
use erpx_server_talent::public::talent_application::{
    ApplicationAction, ApplicationChange, ApplicationRequest, PgTalentApplications, ProposalAction,
    ProposalChange, ProposalRequest, Recommendation, change_application, change_proposal,
    create_application, create_proposal,
};
use sqlx::Row;

const NOW: i64 = 1_800_000_000_000;
const TODAY: &str = "2026-11-11";

fn actor(
    user: &str,
    assignment: &str,
    role: &str,
    branch: Option<&str>,
    permissions: &[&str],
) -> TalentActor {
    TalentActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: Some(role.into()),
        authority_revision: Some(1),
        assignment_version: Some(1),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 600_000,
    }
}

fn recruiter() -> TalentActor {
    actor(
        "recruiter",
        "assignment-recruiter",
        "people_manager",
        Some("branch"),
        &["records.read", "records.write", "records.review"],
    )
}

fn reviewer() -> TalentActor {
    actor(
        "reviewer",
        "assignment-reviewer",
        "people_manager",
        Some("branch"),
        &["records.read", "records.write", "records.review"],
    )
}

fn legal(user: &str) -> TalentActor {
    actor(
        user,
        &format!("assignment-{user}"),
        "holding_owner",
        None,
        &[
            "records.read",
            "records.write",
            "records.review",
            "legal.hold",
            "legal.hold.release",
            "legal.destroy",
            "people.manage",
        ],
    )
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn business<T>(error: PgTransactionError<BusinessError>) -> T {
    match error {
        PgTransactionError::Business(error) => panic!("unexpected business failure: {error:?}"),
        other => panic!("unexpected persistence failure: {other:?}"),
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
        "INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','recruiter','local:recruiter','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1),\
         ('company','legal-a','local:legal-a','active','local',1),\
         ('company','legal-b','local:legal-b','active','local',1),\
         ('company','owner','local:owner','active','local',1),\
         ('company','revoked','local:revoked','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-recruiter','recruiter','people_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-reviewer','reviewer','people_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-legal-a','legal-a','holding_owner','[]','company',NULL,0,9000000000000,NULL,'holding','company','manual'),\
         ('assignment-legal-b','legal-b','holding_owner','[]','company',NULL,0,9000000000000,NULL,'holding','company','manual'),\
         ('assignment-owner','owner','people_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-revoked','revoked','people_manager','[]','company','branch',0,9000000000000,1,'branch','branch','manual');\
         INSERT INTO talent_requisitions(id,company_id,branch_id,department_id,job_title,openings,target_start_date,description,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('talent-req-1','company','branch',NULL,'Service specialist',1,'2026-11-01','Serve customer cases.','open','creator','reviewer',2,1,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn candidate_create(pool: &DatabasePool, key_value: &str) -> String {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), {
            let key_value = key_value.to_owned();
            move |connection| {
                let key_value = key_value.clone();
                Box::pin(async move {
                    let mut repository = PgTalentCandidates::new(connection);
                    let result = create_candidate(
                        &mut repository,
                        &recruiter(),
                        &key(&key_value),
                        &CandidateRequest {
                            legal_name: "  Anong Candidate  ".into(),
                            email: " Anong@Example.TEST ".into(),
                            phone: Some("0812345678".into()),
                            consent_evidence: " Signed recruitment consent form ".into(),
                            retention_until: "2027-01-01".into(),
                        },
                        TODAY,
                        NOW,
                    )
                    .await?;
                    Ok(result["id"].as_str().unwrap().to_owned())
                })
            }
        })
        .await
        .unwrap_or_else(business)
}

async fn application_create(
    pool: &DatabasePool,
    key_value: &str,
    candidate: &str,
    requisition_version: i64,
) -> Result<String, BusinessError> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), {
            let key_value = key_value.to_owned();
            let candidate = candidate.to_owned();
            move |connection| {
                let key_value = key_value.clone();
                let candidate = candidate.clone();
                Box::pin(async move {
                    let mut repository = PgTalentApplications::new(connection);
                    let result = create_application(
                        &mut repository,
                        &recruiter(),
                        &key(&key_value),
                        &ApplicationRequest {
                            requisition_id: "talent-req-1".into(),
                            requisition_version,
                            candidate_id: candidate,
                            owner_id: "recruiter".into(),
                        },
                        TODAY,
                        NOW,
                    )
                    .await?;
                    Ok(result["id"].as_str().unwrap().to_owned())
                })
            }
        })
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected a business failure, got {other:?}"),
        })
}

#[tokio::test(flavor = "multi_thread")]
async fn candidate_privacy_round_trip_persists_evidence_and_history() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = candidate_create(&pool, "talent-candidate-test-0001").await;
        assert_eq!(id, "talent-candidate-talent-candidate-test-0001");

        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let id = id.clone();
                move |connection| {
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentCandidates::new(connection);
                        for (action, expected, key_value, user) in [
                            (
                                PrivacyAction::Hold,
                                1,
                                "talent-privacy-hold-0001",
                                "legal-a",
                            ),
                            (
                                PrivacyAction::Release,
                                2,
                                "talent-privacy-release-0001",
                                "legal-a",
                            ),
                            (
                                PrivacyAction::Withdraw,
                                3,
                                "talent-privacy-withdraw-0001",
                                "legal-a",
                            ),
                            (
                                PrivacyAction::Erase,
                                4,
                                "talent-privacy-erase-0001",
                                "legal-b",
                            ),
                        ] {
                            let result = change_candidate_privacy(
                                &mut repository,
                                &legal(user),
                                &key(key_value),
                                &id,
                                &PrivacyRequest {
                                    action,
                                    expected_version: expected,
                                    reason: "Independent privacy review recorded".into(),
                                },
                                NOW,
                            )
                            .await?;
                            assert_eq!(result["version"], expected + 1);
                        }
                        let view = candidate_privacy_workspace(
                            &mut repository,
                            &legal("legal-a"),
                            &id,
                            NOW,
                        )
                        .await?;
                        assert_eq!(view["candidate"]["status"], "erased");
                        assert_eq!(view["candidate"]["privacyHold"], false);
                        assert_eq!(view["history"].as_array().unwrap().len(), 4);
                        Ok(())
                    })
                }
            })
            .await
            .unwrap_or_else(business);

        let row = sqlx::query(
            "SELECT legal_name,email,phone,consent_evidence,status,privacy_hold,\
             privacy_requested_by,version FROM talent_candidates WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            row.get::<String, _>("legal_name"),
            "Erased recruitment contact"
        );
        assert_eq!(
            row.get::<String, _>("email"),
            format!("{id}@erased.invalid")
        );
        assert_eq!(row.get::<Option<String>, _>("phone"), None);
        assert_eq!(row.get::<String, _>("status"), "erased");
        assert!(!row.get::<bool, _>("privacy_hold"));
        assert_eq!(
            row.get::<Option<String>, _>("privacy_requested_by"),
            Some("legal-a".into())
        );
        assert_eq!(row.get::<i64, _>("version"), 5);

        let events: Vec<(String, i64, String)> = sqlx::query_as(
            "SELECT action,candidate_version,actor_id FROM talent_candidate_privacy_events \
             WHERE candidate_id=$1 ORDER BY candidate_version",
        )
        .bind(&id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![
                ("hold".into(), 2, "legal-a".into()),
                ("release".into(), 3, "legal-a".into()),
                ("withdraw".into(), 4, "legal-a".into()),
                ("erase".into(), 5, "legal-b".into()),
            ]
        );
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE record_id=$1),\
             (SELECT count(*) FROM audit_events WHERE record_id=$1),\
             (SELECT count(*) FROM outbox_events WHERE aggregate_id=$1)",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (5, 5, 5));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn candidate_create_denials_are_owner_bound_and_leak_free() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let error = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                move |connection| {
                    Box::pin(async move {
                        let mut repository = PgTalentCandidates::new(connection);
                        create_candidate(
                            &mut repository,
                            &actor(
                                "recruiter",
                                "assignment-recruiter",
                                "people_manager",
                                Some("branch"),
                                &["records.read", "records.review"],
                            ),
                            &key("talent-candidate-denied-0001"),
                            &CandidateRequest {
                                legal_name: "Anong Candidate".into(),
                                email: "anong@example.test".into(),
                                phone: None,
                                consent_evidence: "Signed recruitment consent form".into(),
                                retention_until: "2027-01-01".into(),
                            },
                            TODAY,
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap_err();
        let error = match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected a business failure, got {other:?}"),
        };
        assert_eq!(error.code, "TALENT_PERMISSION_REQUIRED");

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM talent_candidates")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0);

        let branch_scope = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                move |connection| {
                    Box::pin(async move {
                        let mut repository = PgTalentCandidates::new(connection);
                        candidate_privacy_workspace(
                            &mut repository,
                            &actor(
                                "legal-a",
                                "assignment-legal-a",
                                "holding_owner",
                                Some("branch"),
                                &["records.read", "legal.hold"],
                            ),
                            "talent-candidate-1",
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap_err();
        let branch_scope = match branch_scope {
            PgTransactionError::Business(error) => error,
            other => panic!("expected a business failure, got {other:?}"),
        };
        assert_eq!(branch_scope.code, "COMPANY_HR_SCOPE_REQUIRED");
    })
    .await;
}

struct Capture;

impl PgOfferMoneySnapshotCapture for Capture {
    fn capture<'a>(
        &'a self,
        _: &'a mut sqlx::PgConnection,
        _: &'a TalentActor,
        request: &'a OfferMoneyRequest,
        now: i64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<OfferMoneySnapshot, BusinessError>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            Ok(OfferMoneySnapshot {
                currency_scale: request.currency_scale,
                amount_units: request.amount_units,
                usd_units: 125_000,
                rate_id: Some("fx-rate-1".into()),
                used_latest_fallback: false,
                captured_at: now - 1,
            })
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn application_proposal_offer_and_handoff_round_trip() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let candidate = candidate_create(&pool, "talent-candidate-flow-0001").await;

        let stale = application_create(&pool, "talent-application-stale-0001", &candidate, 1)
            .await
            .unwrap_err();
        assert_eq!(stale.code, "TALENT_SOURCE_CHANGED");

        let application = application_create(&pool, "talent-application-flow-0001", &candidate, 2)
            .await
            .unwrap();
        assert_eq!(
            application,
            "talent-application-talent-application-flow-0001"
        );

        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let application = application.clone();
                move |connection| {
                    let application = application.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentApplications::new(connection);
                        let interview = change_application(
                            &mut repository,
                            &recruiter(),
                            &key("talent-application-interview-0001"),
                            &application,
                            &ApplicationChange {
                                action: ApplicationAction::Interview,
                                expected_version: 1,
                                reason: "Schedule the interview".into(),
                                owner_id: None,
                            },
                            TODAY,
                            NOW,
                        )
                        .await?;
                        assert_eq!(interview["status"], "interview");
                        // The owner is the application creator; a proposal without
                        // its own reviewer stays submitted until independent review.
                        let proposal = create_proposal(
                            &mut repository,
                            &recruiter(),
                            &key("talent-proposal-create-0001"),
                            &ProposalRequest {
                                application_id: application.clone(),
                                application_version: 2,
                                recommendation: Recommendation::Hire,
                                score_bps: 8_750,
                                evidence: "Strong interview evidence and verified references."
                                    .into(),
                            },
                            TODAY,
                            NOW,
                        )
                        .await?;
                        let proposal_id = proposal["id"].as_str().unwrap().to_owned();
                        let accepted = change_proposal(
                            &mut repository,
                            &reviewer(),
                            &key("talent-proposal-accept-0001"),
                            &proposal_id,
                            &ProposalChange {
                                action: ProposalAction::AcceptProposal,
                                expected_version: 1,
                                reason: "Independent proposal review".into(),
                            },
                            TODAY,
                            NOW,
                        )
                        .await?;
                        assert_eq!(accepted["status"], "accepted");
                        let selected = change_application(
                            &mut repository,
                            &recruiter(),
                            &key("talent-application-select-0001"),
                            &application,
                            &ApplicationChange {
                                action: ApplicationAction::Select,
                                expected_version: 2,
                                reason: "Selection approved by the panel".into(),
                                owner_id: None,
                            },
                            TODAY,
                            NOW,
                        )
                        .await?;
                        assert_eq!(selected["status"], "selected");
                        Ok(())
                    })
                }
            })
            .await
            .unwrap_or_else(business);

        let application_id = application.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let application = application_id.clone();
                move |connection| {
                    let application = application.clone();
                    Box::pin(async move {
                        let fail_closed = {
                            let mut repository = PgTalentOffers::new(connection);
                            create_offer(
                                &mut repository,
                                &recruiter(),
                                &key("talent-offer-denied-0001"),
                                &OfferRequest {
                                    application_id: application.clone(),
                                    application_version: 3,
                                    currency: "thb".into(),
                                    base_amount: "45000.50".into(),
                                    start_date: "2026-12-01".into(),
                                    expires_on: "2026-12-15".into(),
                                    terms: "Twelve-month contract".into(),
                                },
                                TODAY,
                                NOW,
                            )
                            .await
                        };
                        let fail_closed = match fail_closed {
                            Ok(_) => {
                                panic!("offer create must fail closed without the Finance producer")
                            }
                            Err(PgTransactionError::Business(error)) => error,
                            Err(other) => panic!("unexpected persistence failure: {other:?}"),
                        };
                        assert_eq!(fail_closed.code, "TALENT_OFFER_MONEY_SNAPSHOT_UNAVAILABLE");
                        Ok(())
                    })
                }
            })
            .await
            .unwrap_or_else(business);

        let offer = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let application = application_id.clone();
                move |connection| {
                    let application = application.clone();
                    Box::pin(async move {
                        let capture = Capture;
                        let mut repository =
                            PgTalentOffers::with_money_snapshot(connection, &capture);
                        let offer = create_offer(
                            &mut repository,
                            &recruiter(),
                            &key("talent-offer-create-0001"),
                            &OfferRequest {
                                application_id: application,
                                application_version: 3,
                                currency: "thb".into(),
                                base_amount: "45000.50".into(),
                                start_date: "2026-12-01".into(),
                                expires_on: "2026-12-15".into(),
                                terms: "  Twelve-month contract with standard benefits.  ".into(),
                            },
                            TODAY,
                            NOW,
                        )
                        .await?;
                        assert_eq!(offer["status"], "draft");
                        let self_approve = change_offer(
                            &mut repository,
                            &recruiter(),
                            &key("talent-offer-self-approve-0001"),
                            offer["id"].as_str().unwrap(),
                            &OfferChange {
                                action: OfferAction::ApproveOffer,
                                expected_version: 1,
                                reason: "Independent approval".into(),
                                acceptance_evidence: None,
                            },
                            TODAY,
                            NOW,
                        )
                        .await;
                        let self_approve = match self_approve {
                            Ok(_) => panic!("creator approval must be refused"),
                            Err(PgTransactionError::Business(error)) => error,
                            Err(other) => panic!("unexpected persistence failure: {other:?}"),
                        };
                        assert_eq!(self_approve.code, "TALENT_INDEPENDENT_REVIEW_REQUIRED");
                        let approved = change_offer(
                            &mut repository,
                            &reviewer(),
                            &key("talent-offer-approve-0001"),
                            offer["id"].as_str().unwrap(),
                            &OfferChange {
                                action: OfferAction::ApproveOffer,
                                expected_version: 1,
                                reason: "Independent approval".into(),
                                acceptance_evidence: None,
                            },
                            TODAY,
                            NOW,
                        )
                        .await?;
                        assert_eq!(approved["status"], "approved");
                        let sent = change_offer(
                            &mut repository,
                            &recruiter(),
                            &key("talent-offer-send-0001"),
                            offer["id"].as_str().unwrap(),
                            &OfferChange {
                                action: OfferAction::Send,
                                expected_version: 2,
                                reason: "Send the approved offer".into(),
                                acceptance_evidence: None,
                            },
                            TODAY,
                            NOW,
                        )
                        .await?;
                        assert_eq!(sent["status"], "sent");
                        let accepted = change_offer(
                            &mut repository,
                            &recruiter(),
                            &key("talent-offer-accept-0001"),
                            offer["id"].as_str().unwrap(),
                            &OfferChange {
                                action: OfferAction::AcceptOffer,
                                expected_version: 3,
                                reason: "Candidate accepted the offer".into(),
                                acceptance_evidence: Some("Signed offer acceptance".into()),
                            },
                            TODAY,
                            NOW,
                        )
                        .await?;
                        assert_eq!(accepted["status"], "accepted");
                        let handoff_id = accepted["handoffId"].as_str().unwrap().to_owned();
                        let source =
                            hire_handoff_source(&mut repository, &recruiter(), &handoff_id, NOW)
                                .await?;
                        assert_eq!(source["handoff"]["offer_status"], "accepted");
                        assert_eq!(source["handoff"]["source_offer_version"], 4);
                        assert_eq!(source["handoff"]["currency"], "THB");
                        assert_eq!(source["handoff"]["base_amount_minor"], 4_500_050);
                        assert_eq!(source["handoff"]["job_title"], "Service specialist");
                        Ok(offer["id"].as_str().unwrap().to_owned())
                    })
                }
            })
            .await
            .unwrap_or_else(business);

        let row = sqlx::query(
            "SELECT o.currency,o.base_amount_minor,o.status,o.version,o.approved_by,\
             o.accepted_evidence,a.status application_status,\
             h.id handoff_id,h.source_offer_version \
             FROM talent_offers o JOIN talent_applications a ON a.id=o.application_id \
             JOIN talent_hire_handoffs h ON h.offer_id=o.id WHERE o.id=$1",
        )
        .bind(&offer)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("currency"), "THB");
        assert_eq!(row.get::<i64, _>("base_amount_minor"), 4_500_050);
        assert_eq!(row.get::<String, _>("status"), "accepted");
        assert_eq!(row.get::<i64, _>("version"), 4);
        assert_eq!(
            row.get::<Option<String>, _>("approved_by"),
            Some("reviewer".into())
        );
        assert_eq!(
            row.get::<Option<String>, _>("accepted_evidence"),
            Some("Signed offer acceptance".into())
        );
        assert_eq!(row.get::<String, _>("application_status"), "accepted");
        assert_eq!(
            row.get::<String, _>("handoff_id"),
            "talent-hire-talent-offer-accept-0001"
        );
        assert_eq!(row.get::<i64, _>("source_offer_version"), 4);

        let events: Vec<(i64, String, String)> = sqlx::query_as(
            "SELECT version,action,status FROM talent_offer_events WHERE offer_id=$1 \
             ORDER BY version",
        )
        .bind(&offer)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![
                (1, "create".into(), "draft".into()),
                (2, "approve-offer".into(), "approved".into()),
                (3, "send".into(), "sent".into()),
                (4, "accept-offer".into(), "accepted".into()),
            ]
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn application_and_proposal_guards_hold_under_real_versions() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let candidate = candidate_create(&pool, "talent-candidate-guard-0001").await;

        let withdrawn = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let candidate = candidate.clone();
                move |connection| {
                    let candidate = candidate.clone();
                    Box::pin(async move {
                        let mut repository = PgTalentCandidates::new(connection);
                        change_candidate_privacy(
                            &mut repository,
                            &legal("legal-a"),
                            &key("talent-privacy-guard-0001"),
                            &candidate,
                            &PrivacyRequest {
                                action: PrivacyAction::Withdraw,
                                expected_version: 1,
                                reason: "Candidate withdrew their application".into(),
                            },
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap_or_else(business);
        assert_eq!(withdrawn["status"], "withdrawn");

        let error = application_create(&pool, "talent-application-guard-0001", &candidate, 2)
            .await
            .unwrap_err();
        assert_eq!(error.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM talent_applications")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0);

        let no_owner = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                move |connection| {
                    Box::pin(async move {
                        let mut repository = PgTalentApplications::new(connection);
                        create_application(
                            &mut repository,
                            &recruiter(),
                            &key("talent-application-guard-0002"),
                            &ApplicationRequest {
                                requisition_id: "talent-req-1".into(),
                                requisition_version: 2,
                                candidate_id: "missing-candidate".into(),
                                owner_id: "ghost".into(),
                            },
                            TODAY,
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap_err();
        let no_owner = match no_owner {
            PgTransactionError::Business(error) => error,
            other => panic!("expected a business failure, got {other:?}"),
        };
        assert_eq!(no_owner.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");
    })
    .await;
}
