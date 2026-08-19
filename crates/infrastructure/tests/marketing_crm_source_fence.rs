//! Disposable acceptance evidence for the CRM source-event history fence.

use chrono::{Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::marketing_crm::{
    AcceptedLeadHandoff, AccountReference, AttributionInputs, AttributionTouch, ContactReference,
    CrmOpportunity, MarketingAcquisitionReference, MarketingLeadReference,
    MarketingQualificationReference, MarketingQualifiedSource, OpportunityStatus,
};
use erpx_infrastructure::SqlxMarketingCrmStore;
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn context(scope: AuthorityScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "crm-source-fence-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "marketing.crm.source_fence.test",
    )
    .expect("command context")
}

fn handoff(scope: AuthorityScope, context: &CommandContext) -> AcceptedLeadHandoff {
    AcceptedLeadHandoff::new(
        Uuid::new_v4(),
        scope.clone(),
        MarketingQualifiedSource {
            lead: MarketingLeadReference::new("crm-source-fence-lead", scope.clone())
                .expect("lead"),
            acquisition: MarketingAcquisitionReference::new(
                "crm-source-fence-acquisition",
                scope.clone(),
            )
            .expect("acquisition"),
            qualification: MarketingQualificationReference::new(
                "crm-source-fence-qualification",
                scope.clone(),
            )
            .expect("qualification"),
        },
        AccountReference::new("crm-source-fence-account", scope.clone()).expect("account"),
        ContactReference::new("crm-source-fence-contact", scope).expect("contact"),
        Utc::now(),
        context,
    )
    .expect("handoff")
}

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL write database"]
async fn crm_update_rolls_back_when_prior_source_event_is_missing() {
    let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required for this ignored test");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_holding_write(&pool)
        .await
        .expect("holding write migration");

    let data_profile_key = format!("crm-source-fence-{}", Uuid::new_v4());
    let scope = AuthorityScope::Company {
        holding_id: Uuid::new_v4(),
        company_id: Uuid::new_v4(),
    };
    let handoff_context = context(scope.clone(), "crm-source-fence-handoff");
    let handoff = handoff(scope.clone(), &handoff_context);
    let store = SqlxMarketingCrmStore::new(pool.clone(), &data_profile_key).expect("CRM store");
    store
        .save_handoff(&handoff, &handoff_context, None)
        .await
        .expect("persist handoff");

    let attribution = AttributionInputs::new(
        scope.clone(),
        vec![
            AttributionTouch::new(
                "crm-source-fence-touch",
                "crm-source-fence-source",
                None,
                scope.clone(),
                Utc::now(),
                10,
                None,
            )
            .expect("attribution touch"),
        ],
    )
    .expect("attribution inputs");
    let opportunity_context = context(scope.clone(), "crm-source-fence-opportunity");
    let mut opportunity = CrmOpportunity::new(
        Uuid::new_v4(),
        &handoff,
        AccountReference::new("crm-source-fence-account", scope.clone()).expect("account"),
        ContactReference::new("crm-source-fence-contact", scope.clone()).expect("contact"),
        "CRM source fence opportunity",
        "THB",
        1_000,
        attribution,
        Utc::now(),
        &opportunity_context,
    )
    .expect("opportunity");
    store
        .save_opportunity(&opportunity, &opportunity_context, None)
        .await
        .expect("persist opportunity");

    let mut source_tamper = pool.begin().await.expect("source tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_tamper)
        .await
        .expect("source tamper scope");
    sqlx::query(
        "DELETE FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='marketing_crm_opportunity'
           AND aggregate_id=$2 AND aggregate_version=1",
    )
    .bind(&data_profile_key)
    .bind(opportunity.id)
    .execute(&mut *source_tamper)
    .await
    .expect("remove prior opportunity source event");
    source_tamper.commit().await.expect("commit source tamper");

    let transition_context = context(scope.clone(), "crm-source-fence-qualified");
    opportunity
        .transition(
            OpportunityStatus::Qualified,
            &transition_context,
            Utc::now() + Duration::seconds(1),
        )
        .expect("qualify opportunity");
    let error = store
        .save_opportunity(&opportunity, &transition_context, Some(1))
        .await
        .expect_err("missing prior source event must reject the update");
    assert!(
        error
            .to_string()
            .contains("CRM aggregate source event history is not contiguous")
    );

    let mut verify = pool.begin().await.expect("verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify)
        .await
        .expect("verification scope");
    let row = sqlx::query(
        "SELECT version, status FROM erpx_holding.crm_opportunities
         WHERE data_profile_key=$1 AND opportunity_id=$2",
    )
    .bind(&data_profile_key)
    .bind(opportunity.id)
    .fetch_one(&mut *verify)
    .await
    .expect("authoritative opportunity after rollback");
    assert_eq!(row.get::<i64, _>("version"), 1);
    assert_eq!(row.get::<String, _>("status"), "draft");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1 AND aggregate_type='marketing_crm_opportunity'
               AND aggregate_id=$2",
        )
        .bind(&data_profile_key)
        .bind(opportunity.id)
        .fetch_one(&mut *verify)
        .await
        .expect("opportunity source event count after rollback"),
        0
    );
    verify.rollback().await.expect("verification rollback");
}
