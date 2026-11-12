//! Native payroll run command policy and calculation tests.
//! PostgreSQL transaction behaviour and the composed Finance journal port are
//! proven by sibling integration targets; these are owner unit facts.
use erpx_server_benefits::public::payroll_source::BenefitPayrollInputRow;
use erpx_server_people_payroll::public::payroll_run::{
    CreatePayrollRunInput, PayrollCalculationInput, PayrollCommandActor, PayrollCompensation,
    PayrollCostAssignment, PayrollEngagement, PayrollPayee, PayrollRun, PayrollTimeEntry,
    PostPayrollInput, ThaiPayrollCalculation, ThaiPayrollRules, calculate_payroll_item,
    calculate_thai_payroll, cancellable, decimal_to_minor, require_calculation_ready,
};
use erpx_server_shared::public::{CompanyId, HoldingId, TimestampMillis};
use serde_json::json;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn run() -> PayrollRun {
    PayrollRun {
        id: "run".into(),
        company_id: "company".into(),
        pay_group_id: "group".into(),
        pay_group_currency: "THB".into(),
        pay_group_status: "active".into(),
        overtime_multiplier_bps: 15_000,
        proration_method: "none".into(),
        country_pack_id: None,
        period_start: "2026-10-01".into(),
        period_end: "2026-10-31".into(),
        pay_date: "2026-10-31".into(),
        currency: "THB".into(),
        mode: "preview".into(),
        status: "draft".into(),
        revision: 1,
        totals_json: "{}".into(),
        input_digest: None,
        version: 1,
        created_by: "owner".into(),
        submitted_by: None,
        approved_by: None,
        posted_by: None,
        adjustment_of_run_id: None,
        scope_engagement_id: None,
    }
}

fn calculation_input() -> PayrollCalculationInput {
    PayrollCalculationInput {
        engagement: PayrollEngagement {
            id: "engagement".into(),
            version: 1,
            person_id: "person".into(),
            worker_type: "employee".into(),
            employment_type: "salaried".into(),
            start_date: "2026-01-01".into(),
            end_date: None,
            job_title: "Engineer".into(),
            branch_id: Some("branch".into()),
            department_id: None,
        },
        payee: PayrollPayee {
            id: "person".into(),
            version: 1,
            legal_name: "Person".into(),
            preferred_name: None,
        },
        compensation: Some(PayrollCompensation {
            id: "compensation".into(),
            version: 1,
            base_amount_minor: 3_000_000,
            hourly_rate_minor: 20_000,
            daily_rate_minor: 0,
            standard_daily_hours: 8.0,
            currency: "THB".into(),
            pay_frequency: "monthly".into(),
            effective_from: "2026-01-01".into(),
            effective_to: None,
            status: "approved".into(),
            components_json: Some(
                r#"[{"code":"BONUS","label":"Bonus","kind":"earning","amount":"1000.00","incomeTiming":"irregular","socialSecurityWage":true}]"#
                    .into(),
            ),
        }),
        time_entries: vec![PayrollTimeEntry {
            id: "time".into(),
            version: 1,
            work_date: "2026-10-01".into(),
            regular_hours: 160.0,
            overtime_hours: 0.0,
            cost_assignment_id: Some("assignment".into()),
            cost_branch_id: Some("branch".into()),
            cost_department_id: None,
        }],
        handoffs: Vec::new(),
        advances: Vec::new(),
        cost_assignments: vec![PayrollCostAssignment {
            id: "assignment".into(),
            version: 1,
            effective_from: "2026-01-01".into(),
            effective_to: None,
            branch_id: Some("branch".into()),
            department_id: None,
            evidence_status: "verified".into(),
        }],
        benefits: Vec::new(),
        thai_statutory: None,
    }
}

#[test]
fn create_input_retains_period_pay_date_and_company_scope_rules() {
    let input = CreatePayrollRunInput::parse(&json!({
        "payGroupId": "group",
        "periodStart": "2026-10-01",
        "periodEnd": "2026-10-31",
        "payDate": "2026-10-31",
        "currency": "THB",
        "mode": "preview"
    }))
    .unwrap();
    assert_eq!(input.period_end, "2026-10-31");
    let invalid = CreatePayrollRunInput::parse(&json!({
        "payGroupId": "group",
        "periodStart": "2026-10-31",
        "periodEnd": "2026-10-01",
        "payDate": "2026-10-31",
        "currency": "THB"
    }));
    assert_eq!(invalid.unwrap_err().code, "INVALID_PAYROLL_PERIOD");
    assert_eq!(decimal_to_minor("1000.5").unwrap(), 100_050);
    assert_eq!(
        decimal_to_minor("1000.555").unwrap_err().code,
        "INVALID_MONEY"
    );
}

#[test]
fn calculation_retains_components_and_readiness_prerequisites() {
    let input = calculation_input();
    let run = run();
    require_calculation_ready(
        &run,
        &erpx_server_people_payroll::public::payroll_run::PayrollCalculationContext {
            inputs: vec![input.clone()],
        },
    )
    .unwrap();
    let item = calculate_payroll_item(&input, &run).unwrap();
    assert_eq!(item.gross_minor, 3_100_000);
    assert_eq!(item.net_minor, 3_100_000);
    let snapshot: serde_json::Value = serde_json::from_str(&item.input_snapshot_json).unwrap();
    assert_eq!(snapshot["employment"], serde_json::Value::Null);
    assert_eq!(snapshot["engagement"]["employment_type"], "salaried");
    assert_eq!(
        snapshot["costAssignments"][0]["evidence_status"],
        "verified"
    );
    let result: serde_json::Value = serde_json::from_str(&item.result_json).unwrap();
    assert_eq!(
        result["explanation"]["components"][0]["amountMinor"],
        100_000
    );
}

fn benefit_input(
    id: &str,
    employee_deduction_minor: i64,
    employer_cost_minor: i64,
) -> BenefitPayrollInputRow {
    BenefitPayrollInputRow {
        id: id.into(),
        election_id: format!("election-{id}"),
        company_id: "company".into(),
        person_id: "person".into(),
        engagement_id: "engagement".into(),
        currency: "THB".into(),
        employee_deduction_minor,
        employer_cost_minor,
        effective_from: "2026-01-01".into(),
        effective_to: None,
        source_election_version: 1,
        status: "released".into(),
        released_by: "user".into(),
        created_at: 0,
        plan_name: "Health Plus".into(),
    }
}

#[test]
fn released_benefits_apply_components_totals_and_snapshot() {
    let mut input = calculation_input();
    input.benefits = vec![benefit_input("benefit-input-1", 150_000, 200_000)];
    let item = calculate_payroll_item(&input, &run()).unwrap();
    assert_eq!(item.gross_minor, 3_100_000);
    assert_eq!(item.deductions_minor, 150_000);
    assert_eq!(item.employer_cost_minor, 200_000);
    assert_eq!(item.net_minor, 2_950_000);
    let result: serde_json::Value = serde_json::from_str(&item.result_json).unwrap();
    let components = result["explanation"]["components"].as_array().unwrap();
    let deduction = components
        .iter()
        .find(|component| component["code"] == "benefit-deduction:benefit-input-1")
        .expect("benefit deduction component");
    assert_eq!(deduction["label"], "Benefit · Health Plus");
    assert_eq!(deduction["kind"], "deduction");
    assert_eq!(deduction["amount"], "1500.00");
    assert_eq!(deduction["amountMinor"], 150_000);
    let employer = components
        .iter()
        .find(|component| component["code"] == "benefit-employer:benefit-input-1")
        .expect("benefit employer component");
    assert_eq!(employer["label"], "Benefit employer cost · Health Plus");
    assert_eq!(employer["kind"], "employer");
    assert_eq!(employer["amount"], "2000.00");
    assert_eq!(employer["amountMinor"], 200_000);
    let snapshot: serde_json::Value = serde_json::from_str(&item.input_snapshot_json).unwrap();
    assert_eq!(snapshot["benefitInputs"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["benefitInputs"][0]["id"], "benefit-input-1");
    assert_eq!(
        snapshot["benefitInputs"][0]["employee_deduction_minor"],
        150_000
    );
    assert_eq!(snapshot["benefitInputs"][0]["employer_cost_minor"], 200_000);
    assert_eq!(snapshot["benefitInputs"][0]["engagement_id"], "engagement");
    assert_eq!(snapshot["benefitInputs"][0]["currency"], "THB");
    assert_eq!(snapshot["benefitInputs"][0]["status"], "released");
}

#[test]
fn zero_amount_benefit_adds_no_component_but_stays_in_snapshot() {
    let mut input = calculation_input();
    input.benefits = vec![benefit_input("benefit-input-zero", 0, 0)];
    let item = calculate_payroll_item(&input, &run()).unwrap();
    assert_eq!(item.deductions_minor, 0);
    assert_eq!(item.employer_cost_minor, 0);
    assert_eq!(item.net_minor, 3_100_000);
    let result: serde_json::Value = serde_json::from_str(&item.result_json).unwrap();
    assert!(
        result["explanation"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .all(|component| !component["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("benefit-")))
    );
    let snapshot: serde_json::Value = serde_json::from_str(&item.input_snapshot_json).unwrap();
    assert_eq!(snapshot["benefitInputs"][0]["id"], "benefit-input-zero");
}

#[test]
fn negative_benefit_amount_is_rejected() {
    let mut input = calculation_input();
    input.benefits = vec![benefit_input("benefit-input-negative", -1, 0)];
    assert_eq!(
        calculate_payroll_item(&input, &run()).unwrap_err().code,
        "PAYROLL_INPUT_INCOMPLETE"
    );
}

#[test]
fn thai_statutory_kernel_is_deterministic_and_effective_dated() {
    let rules: ThaiPayrollRules = serde_json::from_value(json!({
        "countryCode": "TH",
        "currency": "THB",
        "version": "TH-2026-10-02-r1",
        "effectiveFrom": "2026-01-01",
        "effectiveTo": "2026-12-31",
        "standardExpenseRateBps": 5000,
        "standardExpenseCapMinor": 10000000,
        "taxBands": [
            {"upToMinor": 15000000, "rateBps": 0},
            {"upToMinor": 30000000, "rateBps": 500},
            {"upToMinor": 50000000, "rateBps": 1000},
            {"upToMinor": 75000000, "rateBps": 1500},
            {"upToMinor": 100000000, "rateBps": 2000},
            {"upToMinor": 200000000, "rateBps": 2500},
            {"upToMinor": 500000000, "rateBps": 3000},
            {"upToMinor": null, "rateBps": 3500}
        ],
        "socialSecurity": {
            "employeeRateBps": 500,
            "employerRateBps": 500,
            "minimumWageBaseMinor": 165000,
            "maximumWageBaseMinor": 1750000,
            "roundingUnitMinor": 100
        }
    }))
    .unwrap();
    let input = ThaiPayrollCalculation {
        pay_date: "2026-10-31".into(),
        regular_taxable_minor: 3_000_000,
        irregular_taxable_minor: 0,
        irregular_paid_before_minor: Some(0),
        irregular_occurrences_in_year: Some(1),
        termination_severance_minor: Some(0),
        non_taxable_minor: 0,
        social_security_wage_minor: 3_000_000,
        social_security_eligible: true,
        social_security_exemption_evidence: None,
        other_employee_deductions_minor: 0,
        verified_annual_deductions_minor: 0,
        taxable_paid_before_minor: 0,
        tax_withheld_before_minor: 0,
        scheduled_pays_in_year: 12,
        last_pay_in_year: false,
        prior_income_complete: true,
    };
    let first = calculate_thai_payroll(&input, &rules).unwrap();
    let second = calculate_thai_payroll(&input, &rules).unwrap();
    assert_eq!(first.employee_sso_minor, 87_500);
    assert_eq!(first.employer_sso_minor, 87_500);
    assert_eq!(first.net_minor, second.net_minor);
    let mut outside = input;
    outside.pay_date = "2027-01-01".into();
    assert_eq!(
        calculate_thai_payroll(&outside, &rules).unwrap_err().code,
        "COUNTRY_PACK_NOT_EFFECTIVE"
    );
}

#[test]
fn cancellation_is_limited_to_unsubmitted_ordinary_runs() {
    let mut run = run();
    run.status = "preview".into();
    assert!(cancellable(&run).is_ok());
    run.submitted_by = Some("submitter".into());
    assert_eq!(
        cancellable(&run).unwrap_err().code,
        "PAYROLL_CANCELLATION_UNAVAILABLE"
    );
    run.submitted_by = None;
    run.mode = "final_pay".into();
    assert_eq!(
        cancellable(&run).unwrap_err().code,
        "PAYROLL_CANCELLATION_UNAVAILABLE"
    );
}

#[test]
fn post_input_and_actor_retain_company_scope_contract() {
    let input = PostPayrollInput::parse(&json!({
        "chartId": "chart",
        "periodId": "period",
        "entryDate": "2026-10-31",
        "accountMappingId": "mapping",
        "expenseAccountId": "expense",
        "netPayableAccountId": "net",
        "expectedVersion": 2
    }))
    .unwrap();
    assert_eq!(input.expected_version, 2);
    let actor = PayrollCommandActor {
        user_id: "owner".into(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment".into(),
        role: "payroll".into(),
        permissions: vec!["payroll.prepare".into(), "finance.approve".into()],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 60_000,
    };
    assert!(actor.require_prepare(NOW).is_ok());
    assert!(actor.require_post(NOW).is_ok());
}
