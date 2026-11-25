//! Query port and kind dispatcher for the lane 8 read family. This module owns
//! no SQL: the owner reader executes the statements and this handler only
//! validates the bounded request, dispatches and shapes the response.

use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

use super::domain::{
    ApproverName, PayGroup, PersonOption, ReadKind, RosterShift, invalid, valid_id, value,
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
    if values.len() != kind.arity() || values.iter().any(|item| !valid_id(item)) {
        return Err(invalid());
    }
    match kind {
        ReadKind::PayGroups => value(reader.pay_groups(company).await?),
        ReadKind::People => value(reader.people(company).await?),
        ReadKind::ApproverName => value(reader.approver_name(company, &values[0]).await?),
        ReadKind::RosterShift => {
            value(reader.roster_shift(company, &values[0], &values[1]).await?)
        }
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
}
