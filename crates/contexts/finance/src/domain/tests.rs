use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, ReferenceCode, customer, procurement, sales,
};
use uuid::Uuid;

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
}
fn scope() -> TenantScope {
    TenantScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn account_id(value: u128) -> AccountId {
    AccountId::try_from_uuid(id(value)).unwrap()
}
fn account(
    value: u128,
    code: &str,
    category: AccountCategory,
    parent: Option<AccountId>,
) -> Account {
    Account {
        account_id: account_id(value),
        code: code.into(),
        name: code.into(),
        category,
        normal_balance: if matches!(category, AccountCategory::Asset | AccountCategory::Expense) {
            NormalBalance::Debit
        } else {
            NormalBalance::Credit
        },
        parent_account_id: parent,
        status: AccountStatus::Active,
    }
}
fn chart() -> Chart {
    Chart::create(
        ChartId::try_from_uuid(id(10)).unwrap(),
        scope(),
        "MAIN".into(),
        "Main".into(),
        "THB".into(),
        vec![
            account(11, "1100", AccountCategory::Asset, None),
            account(12, "4100", AccountCategory::Revenue, None),
        ],
        at(),
    )
    .unwrap()
}
fn period() -> AccountingPeriod {
    AccountingPeriod::create(
        PeriodId::try_from_uuid(id(20)).unwrap(),
        scope(),
        "2026-08".into(),
        date(1),
        date(31),
        at(),
    )
    .unwrap()
}
fn lines() -> Vec<JournalLine> {
    vec![
        JournalLine {
            line_id: JournalLineId::try_from_uuid(id(31)).unwrap(),
            account_id: account_id(11),
            debit_minor: 100,
            credit_minor: 0,
            currency: "THB".into(),
            memo: "cash".into(),
        },
        JournalLine {
            line_id: JournalLineId::try_from_uuid(id(32)).unwrap(),
            account_id: account_id(12),
            debit_minor: 0,
            credit_minor: 100,
            currency: "THB".into(),
            memo: "revenue".into(),
        },
    ]
}

#[test]
fn chart_rejects_cycles_and_duplicate_codes_without_partial_mutation() {
    let duplicate = Chart::create(
        ChartId::try_from_uuid(id(10)).unwrap(),
        scope(),
        "MAIN".into(),
        "Main".into(),
        "THB".into(),
        vec![
            account(11, "1100", AccountCategory::Asset, None),
            account(12, "1100", AccountCategory::Revenue, None),
        ],
        at(),
    );
    assert!(matches!(
        duplicate,
        Err(FinanceDomainError::InvalidAccountHierarchy)
    ));
    let mut value = chart();
    let before = value.clone();
    assert!(
        value
            .add_account(
                account(13, "1100", AccountCategory::Asset, None),
                at() + Duration::minutes(1)
            )
            .is_err()
    );
    assert_eq!(value, before);
}

#[test]
fn journal_requires_balanced_unique_lines_in_one_currency() {
    let mut invalid = lines();
    invalid[1].credit_minor = 99;
    assert_eq!(
        Journal::create(
            JournalId::try_from_uuid(id(30)).unwrap(),
            scope(),
            chart().id(),
            period().id(),
            "J-30".into(),
            date(5),
            "THB".into(),
            JournalSource::Manual {
                reference: "MAN-30".into()
            },
            "posting".into(),
            invalid,
            at()
        ),
        Err(FinanceDomainError::UnbalancedJournal)
    );
}

#[test]
fn posting_requires_open_bound_period_and_active_accounts() {
    let chart = chart();
    let period = period();
    let mut journal = Journal::create(
        JournalId::try_from_uuid(id(30)).unwrap(),
        scope(),
        chart.id(),
        period.id(),
        "J-30".into(),
        date(5),
        "THB".into(),
        JournalSource::Manual {
            reference: "MAN-30".into(),
        },
        "posting".into(),
        lines(),
        at(),
    )
    .unwrap();
    journal
        .post(&chart, &period, at() + Duration::minutes(1))
        .unwrap();
    assert_eq!(journal.status(), JournalStatus::Posted);
    assert_eq!(journal.debit_total_minor(), journal.credit_total_minor());
    assert_eq!(
        journal.post(&chart, &period, at() + Duration::minutes(2)),
        Err(FinanceDomainError::InvalidJournalTransition)
    );
}

#[test]
fn close_requires_distinct_requester_and_approver_and_reopen_actor() {
    let mut value = period();
    value
        .begin_close("requester", at() + Duration::minutes(1))
        .unwrap();
    let before = value.clone();
    assert_eq!(
        value.close("requester", at() + Duration::minutes(2)),
        Err(FinanceDomainError::CloseApprovalSeparation)
    );
    assert_eq!(value, before);
    value
        .close("approver", at() + Duration::minutes(2))
        .unwrap();
    assert_eq!(
        value.reopen("approver", at() + Duration::minutes(3)),
        Err(FinanceDomainError::InvalidPeriodTransition)
    );
    value
        .reopen("controller", at() + Duration::minutes(3))
        .unwrap();
    assert_eq!(value.status(), PeriodStatus::Open);
}

fn sales_fact() -> sales::SalesOrderFact {
    sales::SalesOrderFact {
        sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
        order_code: ReferenceCode::try_new("SO-40").unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
        status: sales::SalesOrderStatus::Confirmed,
        total: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        lines: vec![sales::SalesOrderLineFact {
            line: sales::SalesOrderLineRef::try_from_uuid(id(42)).unwrap(),
            item_code: ReferenceCode::try_new("ITEM").unwrap(),
            quantity: erpx_integration_contracts::Quantity::try_new(1, 0).unwrap(),
            line_value: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        }],
    }
}

#[test]
fn receivable_uses_sales_fact_and_cannot_over_settle() {
    let mut item = SubledgerItem::receivable_from_sales(
        SubledgerId::try_from_uuid(id(50)).unwrap(),
        scope(),
        sales_fact(),
        at(),
    )
    .unwrap();
    let before = item.clone();
    assert_eq!(
        item.apply_settlement(501, at() + Duration::minutes(1)),
        Err(FinanceDomainError::InvalidSettlement)
    );
    assert_eq!(item, before);
    item.apply_settlement(200, at() + Duration::minutes(1))
        .unwrap();
    assert_eq!(item.outstanding_minor(), 300);
    assert_eq!(item.status(), SubledgerStatus::PartiallySettled);
}

#[test]
fn payable_totals_released_procurement_lines_without_importing_procurement_core() {
    let fact = procurement::ReleasedPurchaseOrderFact {
        purchase_order: procurement::PurchaseOrderRef::try_from_uuid(id(60)).unwrap(),
        supplier: procurement::SupplierRef::try_from_uuid(id(61)).unwrap(),
        order_code: ReferenceCode::try_new("PO-60").unwrap(),
        lines: vec![
            procurement::ReleasedPurchaseOrderLineFact {
                line: procurement::PurchaseOrderLineRef::try_from_uuid(id(62)).unwrap(),
                item_code: ReferenceCode::try_new("ITEM").unwrap(),
                ordered_quantity: erpx_integration_contracts::Quantity::try_new(2, 0).unwrap(),
                line_value: MoneyAmount::new(250, CurrencyCode::try_new("THB").unwrap()),
                destination_code: ReferenceCode::try_new("WH").unwrap(),
            },
            procurement::ReleasedPurchaseOrderLineFact {
                line: procurement::PurchaseOrderLineRef::try_from_uuid(id(63)).unwrap(),
                item_code: ReferenceCode::try_new("SHIP").unwrap(),
                ordered_quantity: erpx_integration_contracts::Quantity::try_new(1, 0).unwrap(),
                line_value: MoneyAmount::new(50, CurrencyCode::try_new("THB").unwrap()),
                destination_code: ReferenceCode::try_new("WH").unwrap(),
            },
        ],
    };
    let item = SubledgerItem::payable_from_procurement(
        SubledgerId::try_from_uuid(id(64)).unwrap(),
        scope(),
        fact,
        at(),
    )
    .unwrap();
    assert_eq!(item.original_minor(), 300);
    assert!(matches!(item.kind(), SubledgerKind::Payable(_)));
}
