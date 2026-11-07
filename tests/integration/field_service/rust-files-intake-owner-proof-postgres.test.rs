//! Actual Field/Commercial/Account witnesses for the Files intake composition.
#[allow(dead_code, reason = "Shared real Account and Field Service fixture")]
#[expect(
    unused_imports,
    reason = "Shared fixture reexports helpers for other targets"
)]
#[path = "../../support/field_service/parts.rs"]
mod facts;

use erpx_server_files::public::field_visit_intake::{
    FieldIntakeAvailability, FieldSetRequest, create_field_set, prepare_field_intake,
};
use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
use erpx_service::field_attachment_intake::{
    FieldAttachmentIntakeAuthority, FieldAttachmentIntakeSources,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[tokio::test]
async fn actual_field_proof_creates_a_set_and_rejects_late_visit_version_change() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let clock = p.review.system.scenario.clock.clone();
        let catalogue = Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap());
        let created = Arc::new(AtomicBool::new(false));
        let observed = created.clone();
        let result = p.review.authorizer.execute_with_sources(
            "acme.example.test", p.review.maker.clone(), p.selection.clone(),
            WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "service/orders".into(), api: true },
            FieldAttachmentIntakeSources,
            move |c, actor| { let clock = clock.clone(); let catalogue = catalogue.clone(); let observed = observed.clone(); Box::pin(async move {
                let holding = HoldingId::new("h001").unwrap();
                let company = CompanyId::new("company").unwrap();
                let authority = FieldAttachmentIntakeAuthority::new(actor, &holding, &company, &catalogue, clock.as_ref());
                let now = clock.now().get();
                let read = prepare_field_intake(c, &authority, actor, "issue-order", false, now).await?;
                let write = prepare_field_intake(c, &authority, actor, "issue-order", true, now).await?;
                let input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: None, purpose_key: "site_photo".into(), expected_target_version: read.scope().version };
                let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
                let set = create_field_set(c, &read, Some(&write), &input, "actual-field-intake", &ready, now).await?;
                assert_eq!(set["targetId"], "issue-order");
                observed.store(true, Ordering::SeqCst);
                // Deliberate fixture corruption: the intake proof must notice a
                // Field version change even though bind permits its own advance.
                sqlx::query("UPDATE field_service_orders SET version=version+1 WHERE id='issue-order'").execute(&mut *c).await?;
                let rejected = read.require_current(c, now).await;
                assert!(matches!(&rejected, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) if error.code == "SERVICE_ATTACHMENT_CHANGED"));
                rejected
            }) },
        ).await;
        assert!(created.load(Ordering::SeqCst), "actual Field proof did not reach Files creation");
        assert!(result.is_err());
        let effects: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE company_id='company' AND action IN ('set.created','set.reused'))").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(effects, (0, 0));
    }).await;
}

#[tokio::test]
async fn actual_field_proof_commits_one_set_and_reuses_its_original_draft() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let clock = p.review.system.scenario.clock.clone();
        let catalogue = Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap());
        let mut saved = None;
        for (key, replay) in [("real-create", false), ("real-create", true), ("real-alias", false), ("real-alias", true)] {
            let clock = clock.clone();
            let catalogue = catalogue.clone();
            let value = p.review.authorizer.execute_with_sources(
                "acme.example.test", p.review.maker.clone(), p.selection.clone(),
                WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "service/orders".into(), api: true },
                FieldAttachmentIntakeSources,
                move |c, actor| { let clock = clock.clone(); let catalogue = catalogue.clone(); Box::pin(async move {
                    let holding = HoldingId::new("h001").unwrap();
                    let company = CompanyId::new("company").unwrap();
                    let authority = FieldAttachmentIntakeAuthority::new(actor, &holding, &company, &catalogue, clock.as_ref());
                    let now = clock.now().get();
                    let read = prepare_field_intake(c, &authority, actor, "issue-order", false, now).await?;
                    let write = if replay { None } else { Some(prepare_field_intake(c, &authority, actor, "issue-order", true, now).await?) };
                    let input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: None, purpose_key: "site_photo".into(), expected_target_version: read.scope().version };
                    let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
                    create_field_set(c, &read, write.as_ref(), &input, key, &ready, now).await
                }) },
            ).await.unwrap();
            if let Some(prior) = &saved { assert_eq!(prior, &value); } else { saved = Some(value); }
        }
        let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action='set.created'),(SELECT count(*) FROM attachment_events WHERE action='set.reused')").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(counts, (1, 1, 1));
    }).await;
}
