//! Query port and kind dispatcher for the lane 8 read family. This module owns
//! no SQL: the owner reader executes the statements and this handler only
//! validates the bounded request, dispatches and shapes the response.

use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

use super::domain::{
    ApproverName, CountryPackRow, ExportBytesRow, IssuableRun, OwnSlipRow, PayGroup, PayGroupFacts,
    PaymentRow, PayrollItemTotals, PersonOption, PayslipItemRow, ReadKind, RosterShift,
    SlipRow, StatutoryRun, invalid, unavailable, valid_id, valid_values, value,
};

pub type LaneReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader; the router transports bounded
/// identities only. The Company scope is the isolated context header.
pub trait AppEntrypointRead: Send + Sync {
    fn pay_groups<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<PayGroup>>;
    fn people<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<PersonOption>>;
    fn approver_name<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
    ) -> LaneReadFuture<'a, Option<ApproverName>>;
    fn roster_shift<'a>(
        &'a self,
        company: &'a str,
        shift: &'a str,
        engagement: &'a str,
    ) -> LaneReadFuture<'a, Option<RosterShift>>;

    // Payslip family. Default bodies keep alternate readers fail-closed; the
    // Pg owner reader overrides every method with its own bounded statement.
    fn issuable_run<'a>(
        &'a self, _company: &'a str, _run: &'a str,
    ) -> LaneReadFuture<'a, Option<IssuableRun>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn issuable_items<'a>(
        &'a self, _company: &'a str, _run: &'a str,
    ) -> LaneReadFuture<'a, Vec<PayslipItemRow>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn own_slips<'a>(
        &'a self,
        _company: &'a str,
        _run: Option<&'a str>,
        _user: Option<&'a str>,
    ) -> LaneReadFuture<'a, Vec<OwnSlipRow>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn slip<'a>(
        &'a self,
        _company: &'a str,
        _id: &'a str,
        _user: Option<&'a str>,
    ) -> LaneReadFuture<'a, Option<SlipRow>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn payment<'a>(
        &'a self,
        _company: &'a str,
        _run: &'a str,
        _person: &'a str,
    ) -> LaneReadFuture<'a, Option<PaymentRow>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn export_bytes<'a>(
        &'a self, _company: &'a str, _id: &'a str,
    ) -> LaneReadFuture<'a, Option<ExportBytesRow>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn country_pack<'a>(
        &'a self, _id: &'a str,
    ) -> LaneReadFuture<'a, Option<CountryPackRow>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn pay_group_facts<'a>(
        &'a self, _company: &'a str, _id: &'a str,
    ) -> LaneReadFuture<'a, Option<PayGroupFacts>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn statutory_run<'a>(&'a self, _run: &'a str) -> LaneReadFuture<'a, Option<StatutoryRun>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn statutory_items<'a>(
        &'a self, _run: &'a str,
    ) -> LaneReadFuture<'a, Vec<PayrollItemTotals>> {
        Box::pin(async { Err(unavailable()) })
    }
    fn retained_pack<'a>(&'a self, _id: &'a str) -> LaneReadFuture<'a, Option<CountryPackRow>> {
        Box::pin(async { Err(unavailable()) })
    }
}

/// Dispatch exactly one owner read. `company` comes from the validated context
/// header; `values` are the call-site identities and are never SQL.
pub async fn read<R: AppEntrypointRead + ?Sized>(
    reader: &R,
    company: &str,
    kind: &str,
    values: &[String],
) -> Result<Value, BusinessError> {
    if !valid_id(company) {
        return Err(invalid());
    }
    let kind = ReadKind::parse(kind)?;
    if !valid_values(kind, values) {
        return Err(invalid());
    }
    match kind {
        ReadKind::PayGroups => value(reader.pay_groups(company).await?),
        ReadKind::People => value(reader.people(company).await?),
        ReadKind::ApproverName => value(reader.approver_name(company, &values[0]).await?),
        ReadKind::RosterShift => {
            value(reader.roster_shift(company, &values[0], &values[1]).await?)
        }
        ReadKind::IssuableRun => value(reader.issuable_run(company, &values[0]).await?),
        ReadKind::IssuableItems => value(reader.issuable_items(company, &values[0]).await?),
        ReadKind::OwnSlips => {
            let run = (!values[0].is_empty()).then_some(values[0].as_str());
            let user = (!values[1].is_empty()).then_some(values[1].as_str());
            value(reader.own_slips(company, run, user).await?)
        }
        ReadKind::Slip => {
            let user = (!values[1].is_empty()).then_some(values[1].as_str());
            value(reader.slip(company, &values[0], user).await?)
        }
        ReadKind::Payment => {
            value(reader.payment(company, &values[0], &values[1]).await?)
        }
        ReadKind::ExportBytes => value(reader.export_bytes(company, &values[0]).await?),
        ReadKind::CountryPack => value(reader.country_pack(&values[0]).await?),
        ReadKind::PayGroupFacts => value(reader.pay_group_facts(company, &values[0]).await?),
        ReadKind::StatutoryRun => value(reader.statutory_run(&values[0]).await?),
        ReadKind::StatutoryItems => value(reader.statutory_items(&values[0]).await?),
        ReadKind::RetainedPack => value(reader.retained_pack(&values[0]).await?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fake {
        calls: std::sync::Mutex<BTreeMap<String, usize>>,
    }
    impl Fake {
        fn new() -> Self {
            Self { calls: std::sync::Mutex::new(BTreeMap::new()) }
        }
        fn mark(&self, key: &str) {
            *self.calls.lock().unwrap().entry(key.to_owned()).or_insert(0) += 1;
        }
    }
    impl AppEntrypointRead for Fake {
        fn pay_groups<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<PayGroup>> {
            let company = company.to_owned();
            self.mark("pay_groups");
            Box::pin(async move { Ok(vec![PayGroup {
                id: "g1".into(), name: format!("Group {company}"), currency: "THB".into(), frequency: "monthly".into(),
            }]) })
        }
        fn people<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<PersonOption>> {
            let company = company.to_owned();
            self.mark("people");
            Box::pin(async move { Ok(vec![PersonOption {
                id: "p1".into(), legal_name: format!("Person {company}"), email: None, version: 1,
            }]) })
        }
        fn approver_name<'a>(
            &'a self, company: &'a str, user: &'a str,
        ) -> LaneReadFuture<'a, Option<ApproverName>> {
            let (company, user) = (company.to_owned(), user.to_owned());
            self.mark("approver_name");
            Box::pin(async move { Ok(Some(ApproverName { legal_name: format!("{company}:{user}") })) })
        }
        fn roster_shift<'a>(
            &'a self, company: &'a str, shift: &'a str, engagement: &'a str,
        ) -> LaneReadFuture<'a, Option<RosterShift>> {
            let (company, shift, engagement) = (company.to_owned(), shift.to_owned(), engagement.to_owned());
            self.mark("roster_shift");
            Box::pin(async move { Ok(Some(RosterShift {
                id: shift, local_date: company, start_time: "09:00".into(), end_time: None,
                end_local: "2026-11-25T17:00".into(), timezone: engagement,
            })) })
        }
        fn issuable_run<'a>(
            &'a self, company: &'a str, run: &'a str,
        ) -> LaneReadFuture<'a, Option<IssuableRun>> {
            self.mark("issuable_run");
            let (company, run) = (company.to_owned(), run.to_owned());
            Box::pin(async move { Ok(Some(IssuableRun {
                id: run, input_digest: "d".into(), revision: 1, version: 2,
                mode: "production".into(), period_start: "2026-11-01".into(),
                period_end: "2026-11-30".into(), pay_date: "2026-11-30".into(), currency: company,
            })) })
        }
        fn issuable_items<'a>(
            &'a self, company: &'a str, run: &'a str,
        ) -> LaneReadFuture<'a, Vec<PayslipItemRow>> {
            self.mark("issuable_items");
            let (company, run) = (company.to_owned(), run.to_owned());
            Box::pin(async move { Ok(vec![PayslipItemRow {
                id: "item".into(), engagement_id: "eng".into(), person_id: run,
                recipient_user_id: None, input_snapshot_json: "{}".into(), result_json: "{}".into(),
                gross_minor: 1000, deductions_minor: 10, employer_cost_minor: 5, net_minor: 990,
                currency: company,
            }]) })
        }
        fn own_slips<'a>(
            &'a self, company: &'a str, run: Option<&'a str>, user: Option<&'a str>,
        ) -> LaneReadFuture<'a, Vec<OwnSlipRow>> {
            self.mark("own_slips");
            let (company, run, user) = (company.to_owned(), run.map(str::to_owned), user.map(str::to_owned));
            Box::pin(async move { Ok(vec![OwnSlipRow {
                id: "slip".into(), run_id: run.unwrap_or_default(), run_revision: 1,
                pay_date: "2026-11-30".into(), currency: company, gross_minor: 1000,
                deductions_minor: 10, net_minor: 990, source_digest: "s".into(),
                export_sha256: "e".into(), issued_at: 100, snapshot_json: user.unwrap_or_default(),
            }]) })
        }
        fn slip<'a>(
            &'a self, company: &'a str, id: &'a str, user: Option<&'a str>,
        ) -> LaneReadFuture<'a, Option<SlipRow>> {
            self.mark("slip");
            let (company, id, user) = (company.to_owned(), id.to_owned(), user.map(str::to_owned));
            Box::pin(async move { Ok(Some(SlipRow {
                id, run_id: company, person_id: user.unwrap_or_default(),
                recipient_user_id: None, snapshot_json: "{}".into(),
                export_html: "<p>x</p>".into(), export_sha256: "e".into(), issued_at: 100,
            })) })
        }
        fn payment<'a>(
            &'a self, company: &'a str, run: &'a str, person: &'a str,
        ) -> LaneReadFuture<'a, Option<PaymentRow>> {
            self.mark("payment");
            let (company, run, person) = (company.to_owned(), run.to_owned(), person.to_owned());
            Box::pin(async move { Ok(Some(PaymentRow {
                status: "settled".into(), settled_at: None, batch_status: format!("{company}:{run}:{person}"),
            })) })
        }
        fn export_bytes<'a>(
            &'a self, company: &'a str, id: &'a str,
        ) -> LaneReadFuture<'a, Option<ExportBytesRow>> {
            self.mark("export_bytes");
            let (company, id) = (company.to_owned(), id.to_owned());
            Box::pin(async move { Ok(Some(ExportBytesRow {
                export_html: format!("{company}:{id}"), export_sha256: "e".into(),
            })) })
        }
        fn country_pack<'a>(&'a self, id: &'a str) -> LaneReadFuture<'a, Option<CountryPackRow>> {
            self.mark("country_pack");
            let id = id.to_owned();
            Box::pin(async move { Ok(Some(CountryPackRow {
                id, country_code: "TH".into(), version: 1, status: "active".into(),
                rules_json: "{}".into(), adapter_id: "thai".into(), source_digest: "s".into(),
                record_version: 1,
            })) })
        }
        fn pay_group_facts<'a>(
            &'a self, company: &'a str, id: &'a str,
        ) -> LaneReadFuture<'a, Option<PayGroupFacts>> {
            self.mark("pay_group_facts");
            let (company, id) = (company.to_owned(), id.to_owned());
            Box::pin(async move { Ok(Some(PayGroupFacts {
                country_code: "TH".into(), currency: company, frequency: id,
            })) })
        }
        fn statutory_run<'a>(&'a self, run: &'a str) -> LaneReadFuture<'a, Option<StatutoryRun>> {
            self.mark("statutory_run");
            let run = run.to_owned();
            Box::pin(async move { Ok(Some(StatutoryRun {
                mode: run, country_pack_id: None, currency: "THB".into(),
            })) })
        }
        fn statutory_items<'a>(
            &'a self, run: &'a str,
        ) -> LaneReadFuture<'a, Vec<PayrollItemTotals>> {
            self.mark("statutory_items");
            let run = run.to_owned();
            Box::pin(async move { Ok(vec![PayrollItemTotals {
                gross_minor: 1000, deductions_minor: 10, net_minor: 990, result_json: run,
            }]) })
        }
        fn retained_pack<'a>(&'a self, id: &'a str) -> LaneReadFuture<'a, Option<CountryPackRow>> {
            self.mark("retained_pack");
            let id = id.to_owned();
            Box::pin(async move { Ok(Some(CountryPackRow {
                id, country_code: "TH".into(), version: 1, status: "superseded".into(),
                rules_json: "{}".into(), adapter_id: "thai".into(), source_digest: "s".into(),
                record_version: 2,
            })) })
        }
    }

    #[tokio::test]
    async fn dispatcher_maps_each_kind_without_sql() {
        let reader = Fake::new();
        assert!(read(&reader, "company", "hire_handoff_pay_groups", &[])
            .await.unwrap()[0].get("id").is_some());
        assert!(read(&reader, "company", "hire_handoff_people", &[])
            .await.unwrap()[0].get("id").is_some());
        assert_eq!(
            read(&reader, "company", "request_approver_legal_name", &["user-1".to_owned()]).await.unwrap(),
            serde_json::json!({"legal_name":"company:user-1"})
        );
        assert_eq!(
            read(&reader, "company", "overtime_roster_shift", &["shift-1".to_owned(), "eng-1".to_owned()]).await.unwrap(),
            serde_json::json!({"id":"shift-1","local_date":"company","start_time":"09:00","end_time":null,"end_local":"2026-11-25T17:00","timezone":"eng-1"})
        );
        let calls = reader.calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
    }

    #[tokio::test]
    async fn dispatcher_rejects_unknown_kind_wrong_arity_and_unsafe_company() {
        let reader = Fake::new();
        for (kind, values) in [
            ("SELECT 1", vec![]),
            ("hire_handoff_pay_groups", vec!["x".to_owned()]),
            ("overtime_roster_shift", vec!["shift".to_owned()]),
            ("request_approver_legal_name", vec!["../other".to_owned()]),
        ] {
            assert_eq!(
                read(&reader, "company", kind, &values).await.unwrap_err().code,
                "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID"
            );
        }
        assert_eq!(
            read(&reader, "../other", "hire_handoff_people", &[]).await.unwrap_err().code,
            "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID"
        );
        assert!(reader.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatcher_maps_payslip_reads_with_absent_filter_sentinels() {
        let reader = Fake::new();
        assert_eq!(
            read(&reader, "company", "payslip_issuable_run", &["run".to_owned()])
                .await.unwrap()["id"], "run"
        );
        assert!(read(&reader, "company", "payslip_issuable_items", &["run".to_owned()])
            .await.unwrap()[0].get("id").is_some());
        // Broad own-slips: empty run + empty user sentinels.
        let broad = read(&reader, "company", "payslip_own_slips",
            &[String::new(), String::new(), String::new()]).await.unwrap();
        assert_eq!(broad[0]["run_id"], "");
        // Own-slips filtered by run and user; both user slots paired.
        let scoped = read(&reader, "company", "payslip_own_slips",
            &["run".to_owned(), "user".to_owned(), "user".to_owned()]).await.unwrap();
        assert_eq!(scoped[0]["run_id"], "run");
        assert_eq!(scoped[0]["snapshot_json"], "user");
        assert_eq!(
            read(&reader, "company", "payslip_slip",
                &["slip".to_owned(), String::new(), String::new()]).await.unwrap()["id"], "slip"
        );
        assert_eq!(
            read(&reader, "company", "payslip_payment",
                &["run".to_owned(), "person".to_owned()]).await.unwrap()["batch_status"],
            "company:run:person"
        );
        assert_eq!(
            read(&reader, "company", "payslip_export_bytes", &["slip".to_owned()])
                .await.unwrap()["export_html"], "company:slip"
        );
        assert_eq!(reader.calls.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn dispatcher_rejects_unpaired_or_unsafe_payslip_filters() {
        let reader = Fake::new();
        for values in [
            vec!["run".to_owned(), "user".to_owned(), String::new()],
            vec!["../run".to_owned(), String::new(), String::new()],
            vec!["run".to_owned()],
        ] {
            assert_eq!(
                read(&reader, "company", "payslip_own_slips", &values).await.unwrap_err().code,
                "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID"
            );
        }
        assert_eq!(
            read(&reader, "company", "payslip_slip", &[String::new(), String::new(), String::new()])
                .await.unwrap_err().code, "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID"
        );
        assert!(reader.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatcher_maps_statutory_evidence_reads() {
        let reader = Fake::new();
        assert_eq!(read(&reader, "company", "statutory_country_pack", &["pack".to_owned()])
            .await.unwrap()["country_code"], "TH");
        assert_eq!(read(&reader, "company", "statutory_pay_group", &["group".to_owned()])
            .await.unwrap()["currency"], "company");
        let run = read(&reader, "company", "statutory_run", &["run".to_owned()]).await.unwrap();
        assert_eq!(run["mode"], "run");
        assert!(run["country_pack_id"].is_null());
        assert_eq!(
            read(&reader, "company", "statutory_items", &["run".to_owned()])
                .await.unwrap()[0]["result_json"], "run"
        );
        assert_eq!(read(&reader, "company", "statutory_retained_pack", &["pack".to_owned()])
            .await.unwrap()["status"], "superseded");
        assert_eq!(reader.calls.lock().unwrap().len(), 5);
    }
}
