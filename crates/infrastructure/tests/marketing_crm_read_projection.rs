//! Disposable acceptance coverage for the established Marketing projection
//! adapter's CRM extension (G10-MKT-021).

use chrono::{Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::marketing_crm::{
    AcceptedLeadHandoff, AccountReference, AttributionInputs, AttributionModel, AttributionTouch,
    ContactReference, CrmOpportunity, MarketingAcquisitionReference, MarketingLeadReference,
    MarketingQualificationReference, MarketingQualifiedSource, OpportunityStatus,
};
use erpx_infrastructure::{SqlxMarketingCrmStore, SqlxMarketingStore};
use erpx_migration::{migrate_holding_read, migrate_holding_write};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "projection-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "marketing.crm.read_projection.test",
    )
    .expect("context")
}

fn handoff(scope: AuthorityScope) -> (AcceptedLeadHandoff, CommandContext) {
    let command = context(scope.clone(), "crm-read-handoff");
    let handoff = AcceptedLeadHandoff::new(
        Uuid::new_v4(),
        scope.clone(),
        MarketingQualifiedSource {
            lead: MarketingLeadReference::new("lead-read-projection", scope.clone()).expect("lead"),
            acquisition: MarketingAcquisitionReference::new(
                "acquisition-read-projection",
                scope.clone(),
            )
            .expect("acquisition"),
            qualification: MarketingQualificationReference::new(
                "qualification-read-projection",
                scope.clone(),
            )
            .expect("qualification"),
        },
        AccountReference::new("account-read-projection", scope.clone()).expect("account"),
        ContactReference::new("contact-read-projection", scope).expect("contact"),
        Utc::now(),
        &command,
    )
    .expect("handoff");
    (handoff, command)
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn crm_events_are_allow_listed_idempotent_rebuildable_and_rls_scoped() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    assert_ne!(write_url, read_url);
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    migrate_holding_write(&write_pool)
        .await
        .expect("write migrations");
    migrate_holding_read(&read_pool)
        .await
        .expect("read migrations");

    let data_profile_key = format!("holding-crm-read-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let (handoff, handoff_context) = handoff(scope.clone());
    let write_store =
        SqlxMarketingCrmStore::new(write_pool.clone(), &data_profile_key).expect("CRM write store");
    write_store
        .save_handoff(&handoff, &handoff_context, None)
        .await
        .expect("persist handoff");

    let attribution = AttributionInputs::new(
        scope.clone(),
        vec![
            AttributionTouch::new(
                "touch-read-projection",
                "source-read-projection",
                Some("campaign-read-projection".into()),
                scope.clone(),
                Utc::now(),
                25,
                None,
            )
            .expect("attribution touch"),
        ],
    )
    .expect("attribution inputs");
    let opportunity_context = context(scope.clone(), "crm-read-opportunity");
    let mut opportunity = CrmOpportunity::new(
        Uuid::new_v4(),
        &handoff,
        AccountReference::new("account-read-projection", scope.clone()).expect("account"),
        ContactReference::new("contact-read-projection", scope.clone()).expect("contact"),
        "Read projection opportunity",
        "THB",
        5_000,
        attribution,
        Utc::now(),
        &opportunity_context,
    )
    .expect("opportunity");
    write_store
        .save_opportunity(&opportunity, &opportunity_context, None)
        .await
        .expect("persist opportunity");
    let transition_context = context(scope.clone(), "crm-read-opportunity-qualified");
    opportunity
        .transition(
            OpportunityStatus::Qualified,
            &transition_context,
            Utc::now() + Duration::seconds(1),
        )
        .expect("qualify opportunity");
    write_store
        .save_opportunity(&opportunity, &transition_context, Some(1))
        .await
        .expect("persist opportunity transition");
    let attribution_context = context(scope.clone(), "crm-read-attribution");
    write_store
        .record_attribution(
            opportunity.id,
            AttributionModel::Linear,
            &attribution_context,
            2,
        )
        .await
        .expect("persist attribution");

    let store = SqlxMarketingStore::from_pools(&write_pool, &read_pool, &data_profile_key)
        .expect("marketing projection store");
    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(page.events.len(), 4);
    assert_eq!(
        page.events
            .iter()
            .filter(|event| event.aggregate_type == "marketing_crm_accepted_lead_handoff")
            .count(),
        1
    );
    let mut forged_payload = page.events[0].payload.clone();
    forged_payload["aggregate"]["reference"] = serde_json::json!("CRM-FORGED");
    assert!(
        store
            .project_event(
                page.events[0].event_id,
                &page.events[0].event_type,
                &page.events[0].aggregate_type,
                page.events[0].aggregate_id,
                page.events[0].aggregate_version,
                &forged_payload,
            )
            .await
            .is_err(),
        "a forged Marketing CRM payload must be rejected before projection"
    );
    let opportunity_events = page
        .events
        .iter()
        .filter(|event| event.aggregate_type == "marketing_crm_opportunity")
        .collect::<Vec<_>>();
    assert_eq!(opportunity_events.len(), 2);
    assert!(
        store
            .project_event(
                opportunity_events[1].event_id,
                &opportunity_events[1].event_type,
                &opportunity_events[1].aggregate_type,
                opportunity_events[1].aggregate_id,
                opportunity_events[1].aggregate_version,
                &opportunity_events[1].payload,
            )
            .await
            .is_err(),
        "a CRM source version gap must be rejected before the read model is changed"
    );
    for event in &page.events {
        let applied = store
            .project_event(
                event.event_id,
                &event.event_type,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project CRM event");
        assert!(applied.applied);
        let replay = store
            .project_event(
                event.event_id,
                &event.event_type,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("replay CRM event");
        assert!(!replay.applied);
    }

    let opportunity_page = store
        .read_crm_page("marketing_crm_opportunity", &scope, None, 1)
        .await
        .expect("read projected CRM opportunity");
    assert_eq!(opportunity_page.rows.len(), 1);
    assert!(!opportunity_page.has_more);
    assert_eq!(opportunity_page.rows[0].aggregate_id, opportunity.id);
    assert_eq!(opportunity_page.rows[0].company_id, company_id);
    assert_eq!(opportunity_page.rows[0].branch_id, None);

    let attribution_page = store
        .read_crm_page("marketing_crm_attribution", &scope, None, 1)
        .await
        .expect("read projected CRM attribution");
    assert_eq!(attribution_page.rows.len(), 1);
    assert_eq!(attribution_page.rows[0].company_id, company_id);
    assert_eq!(
        attribution_page.rows[0].payload["result"]["total_revenue_minor"],
        5_000
    );

    let other_company_scope = AuthorityScope::Company {
        holding_id,
        company_id: Uuid::new_v4(),
    };
    assert!(
        store
            .read_crm_page("marketing_crm_opportunity", &other_company_scope, None, 1,)
            .await
            .expect("read other company CRM opportunity")
            .rows
            .is_empty()
    );

    let mut tx = read_pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let projected: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.crm_accepted_lead_handoff_read_rows
         WHERE data_profile_key=$1 AND handoff_id=$2",
    )
    .bind(&data_profile_key)
    .bind(handoff.id)
    .fetch_one(&mut *tx)
    .await
    .expect("projected handoff");
    assert_eq!(projected, 1);
    let projected_status: String = sqlx::query_scalar(
        "SELECT status FROM erpx_holding.crm_opportunity_read_rows
         WHERE data_profile_key=$1 AND opportunity_id=$2",
    )
    .bind(&data_profile_key)
    .bind(opportunity.id)
    .fetch_one(&mut *tx)
    .await
    .expect("projected opportunity");
    assert_eq!(projected_status, "qualified");
    let projected_revenue: i64 = sqlx::query_scalar(
        "SELECT total_revenue_minor FROM erpx_holding.crm_attribution_read_rows
         WHERE data_profile_key=$1 AND opportunity_id=$2 AND model='linear'",
    )
    .bind(&data_profile_key)
    .bind(opportunity.id)
    .fetch_one(&mut *tx)
    .await
    .expect("projected attribution");
    assert_eq!(projected_revenue, 5_000);
    let receipt: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.crm_projection_event_receipts
         WHERE data_profile_key=$1 AND event_id=$2",
    )
    .bind(&data_profile_key)
    .bind(page.events[0].event_id)
    .fetch_one(&mut *tx)
    .await
    .expect("projection receipt");
    assert_eq!(receipt, 1);
    let receipt_total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.crm_projection_event_receipts
         WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *tx)
    .await
    .expect("all projection receipts");
    assert_eq!(receipt_total, 4);
    tx.commit().await.expect("verification commit");

    let forced_tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c
         JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname='erpx_holding'
           AND c.relname IN (
             'crm_accepted_lead_handoff_read_rows',
             'crm_opportunity_read_rows',
             'crm_attribution_read_rows',
             'crm_projection_event_receipts'
           ) AND c.relforcerowsecurity",
    )
    .fetch_one(&read_pool)
    .await
    .expect("CRM projection RLS metadata");
    assert_eq!(forced_tables, 4);
    let policy_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_policies
         WHERE schemaname='erpx_holding'
           AND policyname LIKE 'holding_scope_crm_%'",
    )
    .fetch_one(&read_pool)
    .await
    .expect("CRM projection policies");
    assert_eq!(policy_count, 4);

    let source_version = store
        .source_event_high_watermark()
        .await
        .expect("Marketing source high watermark");
    assert_eq!(source_version, 4);
    assert!(
        store
            .rebuild_projection_with_source_fence(source_version + 1, 1)
            .await
            .is_err(),
        "a mismatched source fence must reject before clearing the active read model"
    );
    let still_active = store
        .read_crm_page("marketing_crm_opportunity", &scope, None, 1)
        .await
        .expect("active projection after fence rejection");
    assert_eq!(still_active.rows.len(), 1);

    let rebuild = store
        .rebuild_projection_with_source_fence(source_version, 1)
        .await
        .expect("fenced Marketing candidate rebuild");
    assert_eq!(rebuild.source_version, source_version);
    assert_eq!(rebuild.pages_read, 4);
    assert_eq!(rebuild.events_replayed, 4);
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("reconcile fenced Marketing rebuild")
            .is_consistent()
    );

    // A version-only reconciliation would incorrectly accept this state:
    // the CRM opportunity is current, but its disposable payload no longer
    // matches the immutable aggregate carried by the authoritative outbox.
    let mut read_tamper = read_pool.begin().await.expect("read tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *read_tamper)
        .await
        .expect("read tamper scope");
    sqlx::query(
        "UPDATE erpx_holding.crm_opportunity_read_rows
         SET payload=jsonb_set(payload, '{name}', '\"tampered\"'::jsonb, true)
         WHERE data_profile_key=$1 AND opportunity_id=$2",
    )
    .bind(&data_profile_key)
    .bind(opportunity.id)
    .execute(&mut *read_tamper)
    .await
    .expect("tamper CRM opportunity payload");
    read_tamper.commit().await.expect("commit read tamper");
    let payload_mismatch = store
        .reconcile_projection()
        .await
        .expect("reconcile tampered CRM payload");
    assert_eq!(payload_mismatch.payload_mismatch_count, 1);
    assert!(!payload_mismatch.is_consistent());

    store
        .rebuild_projection_with_source_fence(source_version, 1)
        .await
        .expect("repair tampered CRM projection from source");
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("reconcile repaired CRM projection")
            .is_consistent()
    );

    // A max-version-only reconciliation would incorrectly accept this state:
    // the read model still has opportunity version two, while the authoritative
    // source has lost its version-one event. Reconciliation must fail closed.
    let mut source_tamper = write_pool.begin().await.expect("source tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_tamper)
        .await
        .expect("source tamper scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND event_id=$2",
    )
    .bind(&data_profile_key)
    .bind(opportunity_events[0].event_id)
    .execute(&mut *source_tamper)
    .await
    .expect("delete intermediate source event");
    source_tamper.commit().await.expect("commit source tamper");
    let missing_event = store
        .reconcile_projection()
        .await
        .expect_err("missing intermediate source event must fail reconciliation");
    assert!(
        missing_event
            .to_string()
            .contains("Marketing source event stream must start at version one")
    );

    let preparation = store.clear_projection().await.expect("clear projection");
    assert_eq!(preparation.rows_cleared, 3);
    assert_eq!(preparation.checkpoints_cleared, 4);
}
