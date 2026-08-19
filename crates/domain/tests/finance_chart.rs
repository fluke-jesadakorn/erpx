use erpx_domain::contracts::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CONTRACT_VERSION, CommandContext,
};
use erpx_domain::finance_chart::{
    AccountCategory, AccountStatus, AccountUsageFacts, FinanceAccount, FinanceChart,
    FinanceChartError, MAX_FINANCE_ACCOUNT_CODE_BYTES, NormalBalance,
};
use uuid::Uuid;

fn context(holding_id: Uuid, company_id: Uuid, key: &str, expected: Option<u64>) -> CommandContext {
    let correlation_id = Uuid::new_v4();
    CommandContext {
        contract_version: CONTRACT_VERSION,
        command_id: Uuid::new_v4(),
        idempotency_key: key.into(),
        correlation_id,
        causation_id: None,
        actor: ActorAttribution {
            principal_ref: "finance-controller".into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        active_scope: AuthorityScope::Company {
            holding_id,
            company_id,
        },
        audit: AuditContext {
            purpose: "maintain chart of accounts".into(),
            customer_visible: false,
            module: Some("finance".into()),
            correlation_id,
        },
        expected_aggregate_version: expected,
        subscription_id: Some(Uuid::new_v4()),
    }
}

fn account(
    id: Uuid,
    code: &str,
    category: AccountCategory,
    parent_id: Option<Uuid>,
) -> FinanceAccount {
    FinanceAccount::new(
        id,
        code,
        format!("{code} account"),
        category,
        category.default_normal_balance(),
        parent_id,
    )
    .expect("account")
}

fn chart() -> FinanceChart {
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let parent = Uuid::new_v4();
    FinanceChart::new(
        Uuid::new_v4(),
        holding_id,
        company_id,
        "Corporate chart",
        vec![
            account(parent, "1000", AccountCategory::Asset, None),
            account(Uuid::new_v4(), "1100", AccountCategory::Asset, Some(parent)),
        ],
        &context(holding_id, company_id, "chart-create", None),
    )
    .expect("chart")
}

#[test]
fn canonical_bounded_accounts_and_company_scope_are_enforced() {
    let account = FinanceAccount::new(
        Uuid::new_v4(),
        " cash.main ",
        "Cash",
        AccountCategory::Asset,
        NormalBalance::Debit,
        None,
    )
    .expect("canonical account");
    assert_eq!(account.code, "CASH.MAIN");

    let too_long = "A".repeat(MAX_FINANCE_ACCOUNT_CODE_BYTES + 1);
    assert!(matches!(
        FinanceAccount::new(
            Uuid::new_v4(),
            too_long,
            "Cash",
            AccountCategory::Asset,
            NormalBalance::Debit,
            None,
        ),
        Err(FinanceChartError::FieldTooLong {
            field: "account_code",
            ..
        })
    ));
    assert!(matches!(
        FinanceAccount::new(
            Uuid::new_v4(),
            "cash/unsafe",
            "Cash",
            AccountCategory::Asset,
            NormalBalance::Debit,
            None,
        ),
        Err(FinanceChartError::InvalidAccountCode)
    ));

    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let wrong_company = Uuid::new_v4();
    assert!(matches!(
        FinanceChart::new(
            Uuid::new_v4(),
            holding_id,
            company_id,
            "Chart",
            vec![],
            &context(holding_id, wrong_company, "wrong-scope", None),
        ),
        Err(FinanceChartError::ScopeOutsideCompany)
    ));
}

#[test]
fn duplicate_codes_and_invalid_parents_are_rejected() {
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let parent = Uuid::new_v4();
    assert!(matches!(
        FinanceChart::new(
            Uuid::new_v4(),
            holding_id,
            company_id,
            "Chart",
            vec![
                account(parent, "1000", AccountCategory::Asset, None),
                account(Uuid::new_v4(), "1000", AccountCategory::Asset, None),
            ],
            &context(holding_id, company_id, "duplicate", None),
        ),
        Err(FinanceChartError::DuplicateAccountCode(code)) if code == "1000"
    ));
    assert!(matches!(
        FinanceChart::new(
            Uuid::new_v4(),
            holding_id,
            company_id,
            "Chart",
            vec![account(
                Uuid::new_v4(),
                "2000",
                AccountCategory::Liability,
                Some(Uuid::new_v4()),
            )],
            &context(holding_id, company_id, "missing-parent", None),
        ),
        Err(FinanceChartError::MissingParent { .. })
    ));
}

#[test]
fn hierarchy_cycles_are_rejected() {
    let mut value = chart();
    let first = value.accounts[0].id;
    let second = value.accounts[1].id;
    value.accounts[0].parent_id = Some(second);
    value.accounts[1].parent_id = Some(first);
    assert!(matches!(
        value.validate(),
        Err(FinanceChartError::HierarchyCycle(_))
    ));
}

#[test]
fn posted_accounts_are_structurally_immutable_but_can_be_deactivated() {
    let mut value = chart();
    let account_id = value.accounts[0].id;
    let mut replacement = value.accounts[0].clone();
    replacement.name = "Renamed cash".into();
    let update_context = context(
        value.holding_id,
        value.company_id,
        "chart-update",
        Some(value.version),
    );
    assert_eq!(
        value.update_account(
            replacement,
            AccountUsageFacts::used(account_id, 7),
            &update_context,
        ),
        Err(FinanceChartError::UsedAccountImmutable(account_id))
    );

    let lifecycle_context = context(
        value.holding_id,
        value.company_id,
        "chart-deactivate",
        Some(value.version),
    );
    value
        .set_account_status(account_id, AccountStatus::Inactive, &lifecycle_context)
        .expect("deactivate used account");
    assert_eq!(
        value.account(account_id).expect("account").status,
        AccountStatus::Inactive
    );
    assert_eq!(value.version, 2);
    assert_eq!(
        value.last_command.idempotency_key,
        lifecycle_context.idempotency_key
    );
}

#[test]
fn optimistic_version_and_command_facts_are_exact() {
    let mut value = chart();
    let stale = context(
        value.holding_id,
        value.company_id,
        "stale",
        Some(value.version + 1),
    );
    assert!(matches!(
        value.add_account(
            account(Uuid::new_v4(), "3000", AccountCategory::Equity, None,),
            &stale,
        ),
        Err(FinanceChartError::ExpectedVersionMismatch { .. })
    ));

    let accepted = context(
        value.holding_id,
        value.company_id,
        "chart-add",
        Some(value.version),
    );
    value
        .add_account(
            account(Uuid::new_v4(), "3000", AccountCategory::Equity, None),
            &accepted,
        )
        .expect("versioned add");
    assert_eq!(value.version, 2);
    assert_eq!(value.last_command.command_id, accepted.command_id);
    assert_eq!(value.last_command.correlation_id, accepted.correlation_id);
}

#[test]
fn account_ids_cannot_be_removed_or_replaced_through_public_commands() {
    let mut value = chart();
    let original_id = value.accounts[0].id;
    let replacement = account(Uuid::new_v4(), "1001", AccountCategory::Asset, None);
    let command = context(
        value.holding_id,
        value.company_id,
        "replace-id",
        Some(value.version),
    );
    assert_eq!(
        value.update_account(
            replacement.clone(),
            AccountUsageFacts::unused(original_id),
            &command,
        ),
        Err(FinanceChartError::UsageFactMismatch(replacement.id))
    );
    assert!(value.account(original_id).is_some());
}
