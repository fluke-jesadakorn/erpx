//! Disposable acceptance coverage for G10-MKT-020.
//!
//! The test exercises the complete write-side handoff boundary: immutable
//! Marketing acceptance, CRM opportunity creation and lifecycle transition,
//! deterministic attribution, idempotent replay, atomic snapshots/outbox,
//! and holding RLS.

use chrono::{Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::marketing_crm::{
    AcceptedLeadHandoff, AccountReference, AttributionInputs, AttributionModel, AttributionTouch,
    ContactReference, CrmOpportunity, MarketingAcquisitionReference, MarketingLeadReference,
    MarketingQualificationReference, MarketingQualifiedSource, OpportunityStatus,
};
use erpx_infrastructure::SqlxMarketingCrmStore;
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope, key: &str, actor: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: actor.to_owned(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "marketing.crm.test",
    )
    .expect("command context")
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn live_marketing_crm_write_boundary_is_atomic_idempotent_and_scoped() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("holding-crm-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let handoff_context = context(scope.clone(), "crm-handoff-command", "marketing-agent");
    let handoff = AcceptedLeadHandoff::new(
        Uuid::new_v4(),
        scope.clone(),
        MarketingQualifiedSource {
            lead: MarketingLeadReference::new("mkt-lead-1", scope.clone()).expect("lead"),
            acquisition: MarketingAcquisitionReference::new("mkt-acquisition-1", scope.clone())
                .expect("acquisition"),
            qualification: MarketingQualificationReference::new(
                "mkt-qualification-1",
                scope.clone(),
            )
            .expect("qualification"),
        },
        AccountReference::new("crm-account-1", scope.clone()).expect("account"),
        ContactReference::new("crm-contact-1", scope.clone()).expect("contact"),
        Utc::now(),
        &handoff_context,
    )
    .expect("handoff");

    let store = SqlxMarketingCrmStore::new(pool.clone(), &data_profile_key).expect("CRM store");
    let first_handoff = store
        .save_handoff(&handoff, &handoff_context, None)
        .await
        .expect("persist handoff");
    assert_eq!(first_handoff.version, 1);
    assert!(!first_handoff.replayed);
    assert!(
        store
            .save_handoff(&handoff, &handoff_context, None)
            .await
            .expect("replay handoff")
            .replayed
    );

    let forged_opportunity_context =
        context(scope.clone(), "crm-opportunity-forged-source", "crm-agent");
    let forged_attribution = AttributionInputs::new(
        scope.clone(),
        vec![
            AttributionTouch::new(
                "forged-touch-1",
                "source-1",
                Some("campaign-1".to_owned()),
                scope.clone(),
                Utc::now(),
                125,
                None,
            )
            .expect("forged attribution touch"),
        ],
    )
    .expect("forged attribution");
    let forged_opportunity = CrmOpportunity::new(
        Uuid::new_v4(),
        &handoff,
        AccountReference::new("crm-account-forged", scope.clone()).expect("forged account"),
        ContactReference::new("crm-contact-1", scope.clone()).expect("contact"),
        "Forged source opportunity",
        "THB",
        10_001,
        forged_attribution,
        Utc::now(),
        &forged_opportunity_context,
    )
    .expect("forged opportunity shape");
    let forged_error = store
        .save_opportunity(&forged_opportunity, &forged_opportunity_context, None)
        .await
        .expect_err("an opportunity must not substitute the handoff account");
    let forged_error_text = forged_error.to_string();
    assert!(
        forged_error_text
            .contains("CRM opportunity accepted-lead source identity binding is invalid")
    );
    assert!(
        !forged_error_text.contains("crm-account-forged"),
        "source-binding errors must not echo CRM identifiers"
    );
    let mut source_binding_check = pool.begin().await.expect("source binding check");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_binding_check)
        .await
        .expect("holding scope");
    let forged_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.crm_opportunities
         WHERE data_profile_key=$1 AND opportunity_id=$2",
    )
    .bind(&data_profile_key)
    .bind(forged_opportunity.id)
    .fetch_one(&mut *source_binding_check)
    .await
    .expect("forged opportunity row count");
    assert_eq!(forged_rows, 0);
    let forged_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(forged_opportunity.id)
    .fetch_one(&mut *source_binding_check)
    .await
    .expect("forged opportunity outbox count");
    assert_eq!(forged_events, 0);
    source_binding_check
        .commit()
        .await
        .expect("commit source binding check");

    let attribution = AttributionInputs::new(
        scope.clone(),
        vec![
            AttributionTouch::new(
                "touch-1",
                "source-1",
                Some("campaign-1".to_owned()),
                scope.clone(),
                Utc::now(),
                125,
                None,
            )
            .expect("first touch"),
            AttributionTouch::new(
                "touch-2",
                "source-2",
                None,
                scope.clone(),
                Utc::now() + Duration::seconds(1),
                75,
                None,
            )
            .expect("second touch"),
        ],
    )
    .expect("attribution inputs");
    let opportunity_context = context(scope.clone(), "crm-opportunity-command", "crm-agent");
    let mut opportunity = CrmOpportunity::new(
        Uuid::new_v4(),
        &handoff,
        AccountReference::new("crm-account-1", scope.clone()).expect("account"),
        ContactReference::new("crm-contact-1", scope.clone()).expect("contact"),
        "Expansion opportunity",
        "THB",
        10_001,
        attribution,
        Utc::now(),
        &opportunity_context,
    )
    .expect("opportunity");
    let first_opportunity = store
        .save_opportunity(&opportunity, &opportunity_context, None)
        .await
        .expect("persist opportunity");
    assert_eq!(first_opportunity.version, 1);
    assert!(
        store
            .save_opportunity(&opportunity, &opportunity_context, None)
            .await
            .expect("replay opportunity")
            .replayed
    );

    let transition_context = context(scope.clone(), "crm-opportunity-qualified", "crm-agent");
    opportunity
        .transition(
            OpportunityStatus::Qualified,
            &transition_context,
            Utc::now() + Duration::seconds(2),
        )
        .expect("qualify opportunity");
    let transitioned = store
        .save_opportunity(&opportunity, &transition_context, Some(1))
        .await
        .expect("persist lifecycle transition");
    assert_eq!(transitioned.version, 2);
    assert!(!transitioned.replayed);
    assert!(
        store
            .save_opportunity(&opportunity, &transition_context, Some(1))
            .await
            .expect("replay lifecycle transition")
            .replayed
    );

    let attribution_context = context(scope.clone(), "crm-attribution-command", "crm-agent");
    let attribution_receipt = store
        .record_attribution(
            opportunity.id,
            AttributionModel::Linear,
            &attribution_context,
            2,
        )
        .await
        .expect("persist attribution");
    assert_eq!(attribution_receipt.source_opportunity_version, 2);
    assert_eq!(attribution_receipt.model, "linear");
    assert!(!attribution_receipt.replayed);
    assert!(
        store
            .record_attribution(
                opportunity.id,
                AttributionModel::Linear,
                &attribution_context,
                2,
            )
            .await
            .expect("replay attribution")
            .replayed
    );
    store
        .reconcile_attribution(opportunity.id, AttributionModel::Linear)
        .await
        .expect("authoritative attribution reconciliation");

    let unauthorized_event_scope = AuthorityScope::Company {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
    };
    let mut unauthorized_event = pool.begin().await.expect("event authorization transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *unauthorized_event)
        .await
        .expect("event authorization holding scope");
    sqlx::query(
        "UPDATE erpx_holding.outbox_events
         SET active_scope=$3
         WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$4",
    )
    .bind(&data_profile_key)
    .bind("marketing_crm_attribution")
    .bind(serde_json::to_value(&unauthorized_event_scope).expect("unauthorized scope JSON"))
    .bind(attribution_receipt.record_id)
    .execute(&mut *unauthorized_event)
    .await
    .expect("tamper attribution event authorization");
    unauthorized_event
        .commit()
        .await
        .expect("commit event authorization tamper");
    let authorization_error = store
        .reconcile_attribution(opportunity.id, AttributionModel::Linear)
        .await
        .expect_err("an attribution event outside the opportunity scope must fail reconciliation");
    assert!(
        authorization_error
            .to_string()
            .contains("CRM attribution source event is incomplete, unauthorized, or divergent")
    );
    let mut restore_event = pool
        .begin()
        .await
        .expect("restore event authorization transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *restore_event)
        .await
        .expect("restore event holding scope");
    sqlx::query(
        "UPDATE erpx_holding.outbox_events
         SET active_scope=$3
         WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$4",
    )
    .bind(&data_profile_key)
    .bind("marketing_crm_attribution")
    .bind(serde_json::to_value(&scope).expect("scope JSON"))
    .bind(attribution_receipt.record_id)
    .execute(&mut *restore_event)
    .await
    .expect("restore attribution event authorization");
    restore_event
        .commit()
        .await
        .expect("commit event authorization restore");

    let mut scoped = pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *scoped)
        .await
        .expect("holding scope");
    let status: String = sqlx::query_scalar(
        "SELECT status FROM erpx_holding.crm_opportunities
         WHERE data_profile_key=$1 AND opportunity_id=$2",
    )
    .bind(&data_profile_key)
    .bind(opportunity.id)
    .fetch_one(&mut *scoped)
    .await
    .expect("qualified opportunity");
    assert_eq!(status, "qualified");
    let attribution_total: i64 = sqlx::query_scalar(
        "SELECT total_revenue_minor FROM erpx_holding.crm_attribution_records
         WHERE data_profile_key=$1 AND opportunity_id=$2 AND model='linear'",
    )
    .bind(&data_profile_key)
    .bind(opportunity.id)
    .fetch_one(&mut *scoped)
    .await
    .expect("attribution record");
    assert_eq!(attribution_total, 10_001);
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events WHERE data_profile_key=$1
         AND aggregate_type IN ($2,$3,$4)",
    )
    .bind(&data_profile_key)
    .bind("marketing_crm_accepted_lead_handoff")
    .bind("marketing_crm_opportunity")
    .bind("marketing_crm_attribution")
    .fetch_one(&mut *scoped)
    .await
    .expect("CRM outbox count");
    assert_eq!(
        outbox_count, 4,
        "create, replay-free transition, and attribution"
    );
    let incomplete_envelopes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1
           AND aggregate_type IN ($2,$3,$4)
           AND (contract_version IS NULL OR event_version IS NULL
                OR active_scope IS NULL OR actor IS NULL
                OR correlation_id IS NULL OR idempotency_key IS NULL)",
    )
    .bind(&data_profile_key)
    .bind("marketing_crm_accepted_lead_handoff")
    .bind("marketing_crm_opportunity")
    .bind("marketing_crm_attribution")
    .fetch_one(&mut *scoped)
    .await
    .expect("CRM envelope completeness");
    assert_eq!(
        incomplete_envelopes, 0,
        "CRM events require full provenance"
    );
    let scoped_envelopes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1
           AND aggregate_type IN ($2,$3,$4)
           AND contract_version=$5 AND event_version=1
           AND active_scope=$6::jsonb
           AND idempotency_key IN ($7,$8,$9,$10)",
    )
    .bind(&data_profile_key)
    .bind("marketing_crm_accepted_lead_handoff")
    .bind("marketing_crm_opportunity")
    .bind("marketing_crm_attribution")
    .bind(1_i32)
    .bind(serde_json::to_value(&scope).expect("scope JSON"))
    .bind(&handoff_context.idempotency_key)
    .bind(&opportunity_context.idempotency_key)
    .bind(&transition_context.idempotency_key)
    .bind(&attribution_context.idempotency_key)
    .fetch_one(&mut *scoped)
    .await
    .expect("CRM envelope provenance");
    assert_eq!(
        scoped_envelopes, 4,
        "each CRM event keeps command provenance"
    );
    scoped.commit().await.expect("commit verification");

    let forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname='erpx_holding' AND c.relname='crm_opportunities'",
    )
    .fetch_one(&pool)
    .await
    .expect("CRM RLS metadata");
    assert!(forced, "CRM opportunity rows must force holding RLS");

    let mut missing_event = pool.begin().await.expect("missing event transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *missing_event)
        .await
        .expect("missing event holding scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$3",
    )
    .bind(&data_profile_key)
    .bind("marketing_crm_attribution")
    .bind(attribution_receipt.record_id)
    .execute(&mut *missing_event)
    .await
    .expect("remove attribution event for fail-closed check");
    missing_event
        .commit()
        .await
        .expect("commit missing event setup");
    let reconciliation_error = store
        .reconcile_attribution(opportunity.id, AttributionModel::Linear)
        .await
        .expect_err("missing attribution event must fail reconciliation");
    assert!(
        reconciliation_error
            .to_string()
            .contains("CRM attribution source event count is not exactly one")
    );
    let replay_error = store
        .record_attribution(
            opportunity.id,
            AttributionModel::Linear,
            &attribution_context,
            2,
        )
        .await
        .expect_err("an attribution replay must revalidate its source event");
    assert!(
        replay_error
            .to_string()
            .contains("CRM attribution source event count is not exactly one")
    );
}
