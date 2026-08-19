use chrono::{Duration, Utc};
use erpx_application::{
    Customer360DetailReadStore, CustomerGlobalReadAuditDetailQuery,
    CustomerGlobalReadAuditDetailStore, CustomerReadStore, CustomerReadView,
    CustomerSecurityAdminScope,
};
use erpx_domain::{
    commercial::BillingState,
    contracts::AuthorityScope as Scope,
    customer::{
        BillingAddress, Customer360Projection, CustomerAccountProjection,
        CustomerCommercialProjection, CustomerHealthProjection, CustomerOnboardingProjection,
        CustomerOrganizationProjection, CustomerScopeBindingProjection, CustomerStatus,
        CustomerSuccessOwner, GlobalReadActor, GlobalReadAuditRecord, GlobalReadScope, HealthState,
        ModuleAllocation, OnboardingState, OrganizationUnit, ResponsibilityContact,
        TradingLegalIdentity,
    },
};
use erpx_infrastructure::{ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use uuid::Uuid;

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required for this ignored test"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required for this ignored test"),
    }
}

fn projection(customer_id: Uuid) -> Customer360Projection {
    // Keep child identities isolated when ignored tests share a database.
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let branch_id = Uuid::new_v4();
    Customer360Projection {
        account: CustomerAccountProjection {
            id: customer_id,
            account_code: format!("DETAIL-{customer_id}"),
            identity: TradingLegalIdentity {
                trading_name: "Detail Customer".into(),
                legal_name: Some("Detail Customer Co., Ltd.".into()),
                registration_number: Some("registration-secret".into()),
                tax_identifier: Some("tax-secret".into()),
            },
            industry: Some("services".into()),
            country: "TH".into(),
            locale: "th-TH".into(),
            billing_address: Some(BillingAddress {
                line_1: "private address".into(),
                ..Default::default()
            }),
            domains: vec!["detail.example".into()],
            responsibility_contacts: vec![ResponsibilityContact {
                id: Uuid::new_v4(),
                display_name: "Security Head".into(),
                email: Some("security@detail.example".into()),
                responsibilities: vec!["security".into()],
                ..Default::default()
            }],
            customer_success_owner: Some(CustomerSuccessOwner {
                principal_ref: "csm-detail".into(),
                display_name: "Customer Success".into(),
            }),
            account_state: CustomerStatus::Active,
            subscription_state: BillingState::Active,
            ..Default::default()
        },
        organization: CustomerOrganizationProjection {
            headquarters: Some(OrganizationUnit {
                id: Uuid::new_v4(),
                name: "Headquarters".into(),
                kind: "headquarters".into(),
                active: true,
                ..Default::default()
            }),
            holdings: vec![OrganizationUnit {
                id: holding_id,
                name: "Holding".into(),
                kind: "holding".into(),
                active: true,
                ..Default::default()
            }],
            companies: vec![OrganizationUnit {
                id: company_id,
                name: "Company".into(),
                kind: "company".into(),
                parent_id: Some(holding_id),
                active: true,
            }],
            branches: vec![OrganizationUnit {
                id: branch_id,
                name: "Branch".into(),
                kind: "branch".into(),
                parent_id: Some(company_id),
                active: true,
            }],
            active_user_count: 12,
            expiring_role_count: 2,
            open_ticket_count: 3,
            holding_count: 1,
            company_count: 1,
            branch_count: 1,
            module_count: 2,
            module_allocations: vec![ModuleAllocation {
                module_key: "operations".into(),
                purchased_quantity: 2,
                allocated_quantity: 1,
                active: true,
            }],
            ..Default::default()
        },
        commercial: CustomerCommercialProjection {
            stripe_customer_ref: Some("cus_sensitive_reference".into()),
            stripe_subscription_ref: Some("sub_sensitive_reference".into()),
            payment_state: BillingState::Active,
            lifetime_value_minor: 250_000,
            module_allocations: vec![ModuleAllocation {
                module_key: "operations".into(),
                purchased_quantity: 2,
                allocated_quantity: 2,
                active: true,
            }],
            ..Default::default()
        },
        onboarding: CustomerOnboardingProjection {
            state: OnboardingState::Live,
            imports_completed: 4,
            training_completed: true,
            owner: Some(CustomerSuccessOwner {
                principal_ref: "csm-detail".into(),
                display_name: "Customer Success".into(),
            }),
            ..Default::default()
        },
        health: CustomerHealthProjection {
            state: HealthState::Healthy,
            database_schema_version: "2026.07".into(),
            projection_lag_seconds: 2,
            worker_failures: 0,
            storage_status: "healthy".into(),
            backup_status: "verified".into(),
            point_in_time_recovery_status: "ready".into(),
            integration_failure_count: 0,
            api_usage_count: 42,
            last_activity_at: Some(Utc::now()),
        },
    }
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn customer_360_detail_reads_are_read_only_and_legacy_compatible() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = SqlxControlPlane::new(pools);
    let customer_id = Uuid::new_v4();
    let projection = projection(customer_id);
    store
        .upsert_customer_projection_read_model(&projection)
        .await
        .expect("seed read projection");

    let audit_updated_at = Utc::now() - Duration::seconds(30);
    sqlx::query(
        "INSERT INTO erpx_control.customer_access_audit_summaries
            (customer_id, read_count, last_access_at, customer_visible, updated_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(customer_id)
    .bind(4_i64)
    .bind(audit_updated_at)
    .bind(true)
    .bind(audit_updated_at)
    .execute(&store.pools().read)
    .await
    .expect("seed access audit summary");

    let notes_updated_at = Utc::now() - Duration::minutes(6);
    let first_note_id = Uuid::new_v4();
    let second_note_id = Uuid::new_v4();
    for (id, created_at, author_ref, body) in [
        (
            first_note_id,
            notes_updated_at,
            "platform-agent",
            "First customer note",
        ),
        (
            second_note_id,
            notes_updated_at + Duration::seconds(1),
            "customer-success",
            "Second customer note",
        ),
    ] {
        sqlx::query(
            "INSERT INTO erpx_control.customer_notes
                (id, customer_id, author_ref, created_at, body, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(customer_id)
        .bind(author_ref)
        .bind(created_at)
        .bind(body)
        .bind(notes_updated_at)
        .execute(&store.pools().read)
        .await
        .expect("seed customer note");
    }

    let stale_at = Utc::now() - Duration::minutes(6);
    sqlx::query("UPDATE erpx_control.customers SET updated_at = $1 WHERE id = $2")
        .bind(stale_at)
        .bind(customer_id)
        .execute(&store.pools().read)
        .await
        .expect("age read projection");

    let organization = store
        .get_customer_organization(customer_id)
        .await
        .expect("organization read");
    let CustomerReadView::Present {
        data: organization,
        metadata,
    } = organization
    else {
        panic!("organization projection should be present");
    };
    assert_eq!(organization.organization.holding_count, 1);
    assert_eq!(organization.organization.branches.len(), 1);
    assert_eq!(
        metadata.freshness,
        erpx_domain::customer::CustomerReadFreshness::Stale
    );
    assert!(metadata.read_only);

    let contacts = store
        .get_customer_contacts(customer_id)
        .await
        .expect("contacts read");
    let CustomerReadView::Present { data: contacts, .. } = contacts else {
        panic!("contacts projection should be present");
    };
    assert_eq!(contacts.contacts[0].display_name, "Security Head");

    let commercial = store
        .get_customer_commercial(customer_id)
        .await
        .expect("commercial read");
    let CustomerReadView::Present {
        data: commercial, ..
    } = commercial
    else {
        panic!("commercial projection should be present");
    };
    assert_eq!(
        commercial.commercial.stripe_customer_ref.as_deref(),
        Some("cus_sensitive_reference")
    );

    let entitlements = store
        .get_customer_entitlements(customer_id)
        .await
        .expect("entitlements read");
    let CustomerReadView::Present {
        data: entitlements, ..
    } = entitlements
    else {
        panic!("entitlements projection should be present");
    };
    assert_eq!(entitlements.active_module_count, 1);
    assert_eq!(entitlements.subscription_allocations.len(), 1);

    let onboarding = store
        .get_customer_onboarding(customer_id)
        .await
        .expect("onboarding read");
    let CustomerReadView::Present {
        data: onboarding, ..
    } = onboarding
    else {
        panic!("onboarding projection should be present");
    };
    assert_eq!(onboarding.onboarding.state, OnboardingState::Live);

    let health = store
        .get_customer_health(customer_id)
        .await
        .expect("health read");
    let CustomerReadView::Present { data: health, .. } = health else {
        panic!("health projection should be present");
    };
    assert_eq!(health.health.projection_lag_seconds, 2);

    let support = store
        .get_customer_support_summary(customer_id)
        .await
        .expect("support summary read");
    let CustomerReadView::Present { data: support, .. } = support else {
        panic!("support summary projection should be present");
    };
    assert_eq!(support.open_ticket_count, 3);

    let access_audit = store
        .get_customer_access_audit_summary(customer_id)
        .await
        .expect("access audit read");
    let CustomerReadView::Present {
        data: access_audit,
        metadata,
    } = access_audit
    else {
        panic!("access audit summary should be present");
    };
    assert_eq!(access_audit.read_count, 4);
    assert_eq!(access_audit.last_access_at, Some(audit_updated_at));
    assert!(access_audit.customer_visible);
    assert!(metadata.read_only);
    assert_eq!(
        metadata.freshness,
        erpx_domain::customer::CustomerReadFreshness::Fresh
    );

    let notes = store
        .get_customer_notes(customer_id)
        .await
        .expect("notes read");
    let CustomerReadView::Present {
        data: notes,
        metadata,
    } = notes
    else {
        panic!("customer notes should be present");
    };
    assert_eq!(notes.notes.len(), 2);
    assert_eq!(notes.notes[0].id, second_note_id);
    assert_eq!(notes.notes[0].body, "Second customer note");
    assert_eq!(
        metadata.freshness,
        erpx_domain::customer::CustomerReadFreshness::Stale
    );
    assert!(metadata.read_only);

    let legacy = store
        .get_customer(customer_id)
        .await
        .expect("legacy detail read")
        .expect("legacy detail projection");
    assert_eq!(legacy.holding_count, 1);
    assert_eq!(legacy.company_count, 1);
    assert_eq!(legacy.branch_count, 1);
    assert_eq!(legacy.open_ticket_count, 3);

    let unknown = Uuid::new_v4();
    let absent_access_audit = store
        .get_customer_access_audit_summary(unknown)
        .await
        .expect("unknown access audit read");
    assert!(matches!(
        absent_access_audit,
        CustomerReadView::Absent { .. }
    ));
    let absent_notes = store
        .get_customer_notes(unknown)
        .await
        .expect("unknown notes read");
    let CustomerReadView::Absent { metadata } = absent_notes else {
        panic!("unknown customer should be absent");
    };
    assert_eq!(metadata.customer_id, unknown);
    assert_eq!(
        metadata.freshness,
        erpx_domain::customer::CustomerReadFreshness::Unknown
    );
    assert!(metadata.read_only);
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn customer_scope_reconciliation_revokes_removed_and_empty_bindings_atomically() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = SqlxControlPlane::new(pools);
    let customer_id = Uuid::new_v4();
    let original = projection(customer_id);
    let holding_id = original.organization.holdings[0].id;

    store
        .upsert_customer_projection_read_model(&original)
        .await
        .expect("seed complete customer projection");
    let binding_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
    )
    .bind(customer_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("initial binding count");
    assert_eq!(binding_count, 3);

    let mut branch_removed = original.clone();
    branch_removed.organization.branches.clear();
    branch_removed.organization.companies.clear();
    store
        .upsert_customer_projection_read_model(&branch_removed)
        .await
        .expect("reconcile removed branch");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
        )
        .bind(customer_id)
        .fetch_one(&store.pools().write)
        .await
        .expect("binding count after branch removal"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.customer_scope_bindings
             WHERE customer_id = $1 AND company_id IS NOT NULL",
        )
        .bind(customer_id)
        .fetch_one(&store.pools().write)
        .await
        .expect("removed company binding count"),
        0,
        "the remaining holding binding intentionally still covers descendants"
    );

    let invalid = CustomerScopeBindingProjection {
        customer_id,
        holding_id: Uuid::nil(),
        company_id: None,
        branch_id: None,
    };
    assert!(
        store
            .reconcile_customer_scope_bindings(customer_id, &[invalid])
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
        )
        .bind(customer_id)
        .fetch_one(&store.pools().write)
        .await
        .expect("binding count after rejected reconciliation"),
        1,
        "validation happens before delete, preserving the previous authority set"
    );

    let mut fully_revoked = branch_removed;
    fully_revoked.organization.holdings.clear();
    fully_revoked.organization.companies.clear();
    store
        .upsert_customer_projection_read_model(&fully_revoked)
        .await
        .expect("reconcile empty binding set");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
        )
        .bind(customer_id)
        .fetch_one(&store.pools().write)
        .await
        .expect("binding count after full revocation"),
        0
    );
    assert!(
        !store
            .customer_scope_contains(
                customer_id,
                &erpx_domain::contracts::AuthorityScope::Holding { holding_id },
            )
            .await
            .expect("fully revoked holding lookup")
    );
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn stale_customer_projection_cannot_regress_scope_bindings() {
    let config = test_config();
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = SqlxControlPlane::new(pools);
    let customer_id = Uuid::new_v4();
    let original = projection(customer_id);
    let holding_id = original.organization.holdings[0].id;
    let fresh_at = Utc::now();
    store
        .upsert_customer_projection_read_model_at(&original, fresh_at)
        .await
        .expect("fresh projection");

    let mut stale = original.clone();
    stale.organization = CustomerOrganizationProjection::default();
    store
        .upsert_customer_projection_read_model_at(&stale, fresh_at - Duration::minutes(1))
        .await
        .expect("stale projection is ignored");

    let persisted = store
        .get_customer(customer_id)
        .await
        .expect("read customer")
        .expect("customer remains present");
    assert_eq!(persisted.account.id, customer_id);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
        )
        .bind(customer_id)
        .fetch_one(&store.pools().write)
        .await
        .expect("binding count"),
        3,
        "an older event must not revoke newer authoritative bindings"
    );
    assert!(
        store
            .customer_scope_contains(customer_id, &Scope::Holding { holding_id })
            .await
            .expect("holding scope")
    );
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn subtree_branch_authorization_requires_the_requested_company() {
    let config = test_config();
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = SqlxControlPlane::new(pools);
    let customer_id = Uuid::new_v4();
    let holding_id = Uuid::new_v4();
    let requested_company = Uuid::new_v4();
    let foreign_company = Uuid::new_v4();
    let requested_branch = Uuid::new_v4();
    let foreign_branch = Uuid::new_v4();
    store
        .upsert_customer_scope_bindings(&[
            CustomerScopeBindingProjection {
                customer_id,
                holding_id,
                company_id: Some(requested_company),
                branch_id: Some(requested_branch),
            },
            CustomerScopeBindingProjection {
                customer_id,
                holding_id,
                company_id: Some(foreign_company),
                branch_id: Some(foreign_branch),
            },
        ])
        .await
        .expect("scope bindings");

    assert!(
        !store
            .customer_scope_contains(
                customer_id,
                &Scope::Subtree {
                    holding_id,
                    organization_node_id: Uuid::new_v4(),
                },
            )
            .await
            .expect("cross-company subtree authorization")
    );
    assert!(
        store
            .customer_scope_contains(
                customer_id,
                &Scope::Subtree {
                    holding_id,
                    organization_node_id: requested_branch,
                },
            )
            .await
            .expect("matching subtree authorization")
    );
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn customer_read_adapter_rejects_cross_customer_projection_identity_and_recovers() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = SqlxControlPlane::new(pools);
    let customer_id = Uuid::new_v4();
    let foreign_customer_id = Uuid::new_v4();
    let original = projection(customer_id);
    let foreign = projection(foreign_customer_id);

    store
        .upsert_customer_projection_read_model(&original)
        .await
        .expect("seed customer projection");
    let initial_binding_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
    )
    .bind(customer_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("initial binding count");
    assert_eq!(initial_binding_count, 3);

    // Simulate a stale or incorrectly rebuilt read row whose database key is
    // still customer_id but whose payload belongs to another customer. The
    // write-side scope authority remains untouched by this read-side fault.
    sqlx::query("UPDATE erpx_control.customers SET payload = $1 WHERE id = $2")
        .bind(serde_json::to_value(&foreign).expect("foreign projection JSON"))
        .bind(customer_id)
        .execute(&store.pools().read)
        .await
        .expect("forge mismatched read payload");

    let list_error = store
        .list_customers(None)
        .await
        .expect_err("list must reject cross-customer read payload");
    let get_error = store
        .get_customer(customer_id)
        .await
        .expect_err("get must reject cross-customer read payload");
    for error in [list_error, get_error] {
        let message = error.to_string();
        assert_eq!(
            message,
            "repository error: invalid persisted value: customer read projection identity does not match read-model key"
        );
        assert!(!message.contains(&foreign_customer_id.to_string()));
        assert!(!message.contains("registration-secret"));
        assert!(!message.contains("tax-secret"));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
        )
        .bind(customer_id)
        .fetch_one(&store.pools().write)
        .await
        .expect("binding count after rejected read"),
        initial_binding_count,
        "read identity rejection must not alter authoritative scope authority"
    );

    // A normal projector rebuild restores the canonical payload and keeps the
    // existing authoritative binding set usable.
    store
        .upsert_customer_projection_read_model(&original)
        .await
        .expect("rebuild canonical customer projection");
    let rebuilt = store
        .get_customer(customer_id)
        .await
        .expect("rebuilt customer read")
        .expect("rebuilt customer should be present");
    assert_eq!(rebuilt.account.id, customer_id);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
        )
        .bind(customer_id)
        .fetch_one(&store.pools().write)
        .await
        .expect("binding count after rebuild"),
        initial_binding_count
    );
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn customer_access_audit_detail_filters_to_security_admin_scope() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect(&config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = SqlxControlPlane::new(pools);
    let customer_id = Uuid::new_v4();
    store
        .upsert_customer_projection_read_model(&projection(customer_id))
        .await
        .expect("seed customer projection");

    let holding = Uuid::new_v4();
    let other_holding = Uuid::new_v4();
    let company = Uuid::new_v4();
    let sibling_company = Uuid::new_v4();
    let branch = Uuid::new_v4();
    let sibling_branch = Uuid::new_v4();
    let scopes = [
        ("platform", None, None, None),
        ("holding", Some(holding), None, None),
        ("company", Some(holding), Some(company), None),
        ("branch", Some(holding), Some(company), Some(branch)),
        (
            "sibling-company",
            Some(holding),
            Some(sibling_company),
            None,
        ),
        (
            "sibling-branch",
            Some(holding),
            Some(sibling_company),
            Some(sibling_branch),
        ),
        ("other-holding", Some(other_holding), None, None),
    ];
    for (index, (label, holding_id, company_id, branch_id)) in scopes.iter().enumerate() {
        let record = GlobalReadAuditRecord::new(
            GlobalReadActor {
                principal_ref: format!("actor-{label}"),
                session_ref: Some(format!("session-{label}")),
            },
            customer_id,
            "platform.customer_360",
            GlobalReadScope {
                label: (*label).into(),
                holding_id: *holding_id,
                company_id: *company_id,
                branch_id: *branch_id,
            },
            "scope-filter-test",
            Utc::now() + Duration::seconds(index as i64),
            Uuid::new_v4(),
        )
        .expect("global-read record");
        store
            .record_global_read(&record)
            .await
            .expect("persist audit");
    }

    async fn list_for(
        store: &SqlxControlPlane,
        customer_id: Uuid,
        holding_id: Option<Uuid>,
        company_id: Option<Uuid>,
        branch_id: Option<Uuid>,
    ) -> Vec<String> {
        let query = CustomerGlobalReadAuditDetailQuery {
            customer_id,
            security_admin_scope: CustomerSecurityAdminScope {
                customer_id,
                platform: false,
                holding_id,
                company_id,
                branch_id,
            },
            audit: erpx_application::CustomerGlobalReadAuditQueryContext {
                requester: GlobalReadActor {
                    principal_ref: "security-admin".into(),
                    session_ref: Some("security-session".into()),
                },
                purpose: "scope-filter-test".into(),
                correlation_id: Uuid::new_v4(),
            },
            before: None,
            limit: 100,
        };
        let CustomerReadView::Present { data, .. } = store
            .get_customer_global_read_audit_details(&query)
            .await
            .expect("scoped audit query")
        else {
            panic!("customer projection should be present");
        };
        data.entries
            .into_iter()
            .map(|entry| entry.scope.label)
            .collect()
    }

    let holding_labels = list_for(&store, customer_id, Some(holding), None, None).await;
    assert_eq!(holding_labels.len(), 6);
    assert!(!holding_labels.iter().any(|label| label == "other-holding"));

    let company_labels = list_for(&store, customer_id, Some(holding), Some(company), None).await;
    assert_eq!(company_labels, vec!["branch", "company", "platform"]);

    let branch_labels = list_for(
        &store,
        customer_id,
        Some(holding),
        Some(company),
        Some(branch),
    )
    .await;
    assert_eq!(branch_labels, vec!["branch", "platform"]);

    let platform_query = CustomerGlobalReadAuditDetailQuery {
        customer_id,
        security_admin_scope: CustomerSecurityAdminScope {
            customer_id,
            platform: true,
            holding_id: None,
            company_id: None,
            branch_id: None,
        },
        audit: erpx_application::CustomerGlobalReadAuditQueryContext {
            requester: GlobalReadActor {
                principal_ref: "platform-agent".into(),
                session_ref: None,
            },
            purpose: "scope-filter-test".into(),
            correlation_id: Uuid::new_v4(),
        },
        before: None,
        limit: 100,
    };
    let CustomerReadView::Present { data, .. } = store
        .get_customer_global_read_audit_details(&platform_query)
        .await
        .expect("platform audit query")
    else {
        panic!("customer projection should be present");
    };
    assert_eq!(data.entries.len(), scopes.len());
}
