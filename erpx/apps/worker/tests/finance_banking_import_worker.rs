use chrono::NaiveDate;
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_infrastructure::{
    BankProviderAccountBinding, BankProviderAccountRef, BankProviderCredentialKey,
    BankProviderError, PersistenceError,
};
use erpx_worker::{FinanceBankProviderImportError, FinanceBankProviderImportRequest};
use uuid::Uuid;

fn request(scope: TenantScope) -> FinanceBankProviderImportRequest {
    let holding = Uuid::from_u128(1);
    let company = Uuid::from_u128(2);
    let account = BankProviderAccountBinding::try_new(
        Uuid::from_u128(3),
        BankProviderAccountRef::try_new("mockbank", "acct-opaque-1").expect("account ref"),
        scope,
        holding,
        company,
        "THB",
        "Operating account",
    )
    .expect("account binding");
    let context = CommandContext::try_new(
        "bank-import-integration-test",
        ActorAttribution {
            principal_ref: "finance-user".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::from_u128(5)),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        account.scope().clone(),
        "authorized bank import integration test",
    )
    .expect("context");
    FinanceBankProviderImportRequest {
        provider_account_ref: "mockbank:acct-opaque-1".into(),
        provider_key: "mockbank".into(),
        tenant_id: "tenant-opaque-1".into(),
        holding_ref: "holding-opaque-1".into(),
        credential_key: BankProviderCredentialKey::try_new("vault/finance/mockbank")
            .expect("credential key"),
        account,
        starts_on: NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
        ends_on: NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
        page_size: 100,
        import_id: Uuid::from_u128(4),
        context,
    }
}

#[test]
fn request_validation_preserves_exact_account_scope_and_rejects_platform_scope() {
    let tenant_request = request(TenantScope::Company {
        holding_id: Uuid::from_u128(1),
        company_id: Uuid::from_u128(2),
    });
    assert!(tenant_request.validate().is_ok());

    let mut platform_request = request(TenantScope::Company {
        holding_id: Uuid::from_u128(1),
        company_id: Uuid::from_u128(2),
    });
    platform_request.context.tenant_scope = TenantScope::Platform;
    assert_eq!(
        platform_request.validate(),
        Err(FinanceBankProviderImportError::AuthorizationDenied)
    );
}

#[test]
fn provider_and_persistence_failures_are_redacted_and_classified() {
    assert_eq!(
        FinanceBankProviderImportError::from(BankProviderError::Unavailable),
        FinanceBankProviderImportError::ProviderTemporarilyUnavailable
    );
    assert_eq!(
        FinanceBankProviderImportError::from(BankProviderError::AuthenticationFailed),
        FinanceBankProviderImportError::ProviderResponseRejected
    );
    assert_eq!(
        FinanceBankProviderImportError::from(PersistenceError::IdempotencyConflict {
            key: "secret-free-key".into(),
        }),
        FinanceBankProviderImportError::IdempotencyConflict
    );
    assert!(
        !format!(
            "{}",
            FinanceBankProviderImportError::ProviderTemporarilyUnavailable
        )
        .contains("secret")
    );
}
