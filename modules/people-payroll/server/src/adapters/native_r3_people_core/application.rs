//! Query ports and owner-side shaping for the native R3 People core slices.
//! No SQL lives here: the Postgres adapter owns every statement, and the owner
//! DTOs/invariants live in `domain`.

use super::domain::{PayGroupOption, forbidden, invalid, valid_id};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Map, Value, json};
use std::{future::Future, pin::Pin, time::{SystemTime, UNIX_EPOCH}};

pub type CoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner store for the SQL-backed People core reads. Composition supplies the
/// Pg implementation; owner-local tests inject a probe.
pub trait PeopleCoreRead: Send + Sync {
    fn today(&self) -> CoreFuture<'_, String>;
    fn pay_group_options(&self, company: &str) -> CoreFuture<'_, Vec<PayGroupOption>>;
    fn engagement(
        &self,
        company: &str,
        engagement: &str,
        branch: Option<&str>,
        department: Option<&str>,
    ) -> CoreFuture<'_, Option<super::domain::HrEngagement>>;
    fn assignments(
        &self,
        company: &str,
        engagement: &str,
    ) -> CoreFuture<'_, Vec<super::domain::HrAssignment>>;
    fn policies(&self, company: &str) -> CoreFuture<'_, Vec<super::domain::HrPolicy>>;
    fn calendars(&self, company: &str) -> CoreFuture<'_, Vec<super::domain::HrCalendar>>;
    fn eligibility(
        &self,
        company: &str,
        engagement: &str,
    ) -> CoreFuture<'_, Option<super::domain::HrEligibility>>;
    fn affected_people(
        &self,
        company: &str,
        ids: &str,
        branch: Option<&str>,
        department: Option<&str>,
    ) -> CoreFuture<'_, Vec<super::domain::HrAffectedPerson>>;
    fn future_shifts(
        &self,
        company: &str,
        ids: &str,
        now_ms: i64,
    ) -> CoreFuture<'_, Vec<super::domain::HrShiftTotal>>;
    fn future_leaves(
        &self,
        company: &str,
        today: &str,
        ids: &str,
    ) -> CoreFuture<'_, Vec<super::domain::HrPersonTotal>>;
    fn current_policies(
        &self,
        company: &str,
        ids: &str,
        today: &str,
    ) -> CoreFuture<'_, Vec<super::domain::HrCurrentPolicy>>;
    fn engagement_groups(
        &self,
        company: &str,
        ids: &str,
    ) -> CoreFuture<'_, Vec<super::domain::HrEngagementGroup>>;
    fn hire_source(
        &self,
        company: &str,
        handoff: &str,
    ) -> CoreFuture<'_, Option<super::domain::HireSource>>;
    fn hire_receipt(
        &self,
        company: &str,
        handoff: &str,
    ) -> CoreFuture<'_, Option<super::domain::HireReceipt>>;
    fn same_currency_groups(
        &self,
        company: &str,
        currency: &str,
    ) -> CoreFuture<'_, Vec<PayGroupOption>>;
    fn matching_people(
        &self,
        company: &str,
        email: &str,
    ) -> CoreFuture<'_, Vec<super::domain::HirePerson>>;
    fn offer_application(&self, company: &str, offer: &str) -> CoreFuture<'_, Option<String>>;
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

fn has(permissions: &[String], permission: &str) -> bool {
    permissions.iter().any(|item| item == permission)
}

fn not_ported(topic: &str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_R3_CORE_TOPIC_NOT_PORTED",
        match topic {
            "levels" | "templates" | "lifecycle" => {
                "This HR settings topic still needs its owner port."
            }
            "leave" | "calendars" => {
                "Leave administration is still owned by the TypeScript adapter."
            }
            _ => "This HR settings selection still needs its owner port.",
        },
    )
}

/// One validated HR settings selection. Mutually exclusive branches are
/// rejected by the HTTP layer before this value is constructed.
#[derive(Clone, Debug)]
pub struct HrInput {
    pub company: String,
    pub branch: Option<String>,
    pub department: Option<String>,
    pub permissions: Vec<String>,
    pub topic: Option<String>,
    pub editor: bool,
    pub engagement: Option<String>,
    pub position: Option<String>,
    pub impact: Vec<String>,
}

fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}

/// Exact `{topic, ...}` payloads for the owner-owned HR settings branches.
pub async fn hr_settings_data(
    store: &dyn PeopleCoreRead,
    input: &HrInput,
) -> Result<Value, BusinessError> {
    let company = input.company.as_str();
    if !valid_id(company) {
        return Err(invalid());
    }

    if let Some(topic) = input.topic.as_deref() {
        return match topic {
            "pay-groups" => Ok(json!({
                "topic": "pay-groups",
                "groups": store.pay_group_options(company).await?,
                "canManage": has(&input.permissions, "payroll.prepare"),
            })),
            "levels" | "leave" | "calendars" | "templates" | "lifecycle" => {
                if !has(&input.permissions, "people.manage") {
                    return Err(forbidden(
                        "HR_SETTINGS_FORBIDDEN",
                        "Your role cannot access this HR setting.",
                    ));
                }
                Err(not_ported(topic))
            }
            _ => Err(invalid()),
        };
    }

    if input.editor {
        if !has(&input.permissions, "people.manage") {
            return Err(forbidden(
                "HR_SETTINGS_FORBIDDEN",
                "Your role cannot access this HR setting.",
            ));
        }
        return Err(not_ported("editor"));
    }
    if let Some(position) = input.position.as_deref() {
        if !valid_id(position)
            || !has(&input.permissions, "people.manage")
            || !has(&input.permissions, "organization.manage")
        {
            return Err(if !valid_id(position) {
                invalid()
            } else {
                forbidden(
                    "HR_SETTINGS_FORBIDDEN",
                    "Your role cannot access this HR setting.",
                )
            });
        }
        return Err(not_ported("position"));
    }

    if let Some(engagement) = input.engagement.as_deref() {
        if !valid_id(engagement) || !has(&input.permissions, "people.manage") {
            return Err(if !valid_id(engagement) {
                invalid()
            } else {
                forbidden(
                    "HR_SETTINGS_FORBIDDEN",
                    "Your role cannot access this HR setting.",
                )
            });
        }
        let found = store
            .engagement(
                company,
                engagement,
                input.branch.as_deref(),
                input.department.as_deref(),
            )
            .await?
            .ok_or_else(|| not_found("ENGAGEMENT_NOT_FOUND", "This staff engagement is outside your workspace."))?;
        let _ = found;
        let assignments = store.assignments(company, engagement).await?;
        let policies = store.policies(company).await?;
        let calendars = store.calendars(company).await?;
        let eligibility = store.eligibility(company, engagement).await?;
        let today = store.today().await?;
        return Ok(json!({
            "assignments": assignments,
            "policies": policies,
            "calendars": calendars,
            "eligibility": eligibility,
            "canManage": input.branch.is_none()
                && input.department.is_none()
                && has(&input.permissions, "leave.policy.manage"),
            "asOf": today,
        }));
    }

    if !input.impact.is_empty() {
        if !has(&input.permissions, "people.manage") || !valid_impact_ids(&input.impact) {
            return Err(if !valid_impact_ids(&input.impact) {
                invalid()
            } else {
                forbidden(
                    "HR_SETTINGS_FORBIDDEN",
                    "Your role cannot access this HR setting.",
                )
            });
        }
        let ids = input.impact.join(",");
        let today = store.today().await?;
        let people = store
            .affected_people(
                company,
                &ids,
                input.branch.as_deref(),
                input.department.as_deref(),
            )
            .await?;
        if people.len() != input.impact.len() {
            return Err(not_found(
                "ENGAGEMENT_NOT_FOUND",
                "Affected staff are no longer available in this workspace. Refresh the move before confirming.",
            ));
        }
        let can_read_roster = ["roster.read", "roster.manage", "roster.approve"]
            .iter()
            .any(|permission| has(&input.permissions, permission));
        let can_read_pay = [
            "people.compensation.read",
            "people.compensation.payroll",
        ]
        .iter()
        .any(|permission| has(&input.permissions, permission));
        let shifts = store.future_shifts(company, &ids, now_ms()).await?;
        let leaves = store.future_leaves(company, &today, &ids).await?;
        let policies = store.current_policies(company, &ids, &today).await?;
        let groups = store.engagement_groups(company, &ids).await?;
        let rows = people
            .into_iter()
            .map(|person| {
                let policy = policies
                    .iter()
                    .find(|item| item.engagement_id == person.engagement_id);
                let mut row = Map::new();
                row.insert("engagementId".into(), json!(person.engagement_id));
                row.insert("name".into(), json!(person.name));
                row.insert(
                    "upcomingShifts".into(),
                    if can_read_roster {
                        json!(
                            shifts
                                .iter()
                                .find(|item| item.engagement_id == person.engagement_id)
                                .map(|item| item.total)
                                .unwrap_or(0)
                        )
                    } else {
                        Value::Null
                    },
                );
                row.insert(
                    "upcomingLeave".into(),
                    json!(
                        leaves
                            .iter()
                            .find(|item| item.person_id == person.person_id)
                            .map(|item| item.total)
                            .unwrap_or(0)
                    ),
                );
                row.insert(
                    "policy".into(),
                    policy
                        .map(|item| json!(item.policy))
                        .unwrap_or(Value::Null),
                );
                row.insert(
                    "calendar".into(),
                    policy
                        .and_then(|item| item.calendar.clone())
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
                if can_read_pay {
                    row.insert(
                        "payGroup".into(),
                        groups
                            .iter()
                            .find(|item| item.engagement_id == person.engagement_id)
                            .and_then(|item| item.name.clone())
                            .map(Value::String)
                            .unwrap_or(Value::Null),
                    );
                }
                Value::Object(row)
            })
            .collect::<Vec<_>>();
        return Ok(json!({ "people": rows }));
    }

    // No topic and no staff selection: the permission-filtered topic list.
    let topics = super::domain::HR_TOPICS
        .iter()
        .filter(|topic| match **topic {
            "pay-groups" => has(&input.permissions, "people.payroll.details"),
            "leave" | "calendars" => has(&input.permissions, "leave.policy.manage"),
            _ => {
                has(&input.permissions, "people.manage")
                    && has(&input.permissions, "organization.manage")
            }
        })
        .copied()
        .collect::<Vec<_>>();
    Ok(json!({ "topics": topics }))
}

fn valid_impact_ids(ids: &[String]) -> bool {
    !ids.is_empty()
        && ids.len() <= super::domain::MAX_IDS
        && ids.iter().all(|id| valid_id(id))
}

/// Exact `{data: ...}` payload of `GET .../hire-handoffs/{id}/pay-groups`.
/// Purchase gating and `canCreatePayGroup` stay with the owner/coordinator.
pub async fn hire_pay_group_workspace(
    store: &dyn PeopleCoreRead,
    company: &str,
    handoff: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[String],
) -> Result<Value, BusinessError> {
    if !valid_id(company) || !valid_id(handoff) {
        return Err(invalid());
    }
    if !has(permissions, "people.manage") {
        return Err(forbidden(
            "HR_SETTINGS_FORBIDDEN",
            "Your role cannot access this HR setting.",
        ));
    }
    if branch.is_some() || department.is_some() {
        return Err(forbidden(
            "COMPANY_HR_SCOPE_REQUIRED",
            "Use a Company HR assignment for the handoff.",
        ));
    }

    let source = store
        .hire_source(company, handoff)
        .await?
        .ok_or_else(|| not_found("HIRE_HANDOFF_NOT_FOUND", "Accepted hire handoff not found."))?;
    if source.offer_status != "accepted" || source.source_offer_version > source.offer_version {
        return Err(not_found(
            "HIRE_HANDOFF_NOT_FOUND",
            "Accepted hire handoff not found.",
        ));
    }
    let receipt = store.hire_receipt(company, handoff).await?;
    let consumed = receipt.is_some();
    let groups = if consumed {
        Vec::new()
    } else {
        store.same_currency_groups(company, &source.currency).await?
    };
    let people = if consumed {
        Vec::new()
    } else {
        store.matching_people(company, &source.email).await?
    };
    let today = store.today().await?;
    let eligible = source.offer_version == source.source_offer_version
        && source.candidate_status == "active"
        && source.retention_until >= today
        && !source.consent_evidence.trim().is_empty()
        && source
            .accepted_evidence
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());

    let mut data = Map::new();
    data.insert("companyId".into(), json!(company));
    let mut handoff_row = Map::new();
    handoff_row.insert("id".into(), json!(source.id));
    handoff_row.insert(
        "applicationId".into(),
        json!(
            store
                .offer_application(company, &source.offer_id)
                .await?
                .unwrap_or_default()
        ),
    );
    handoff_row.insert("offerVersion".into(), json!(source.offer_version));
    handoff_row.insert("candidateVersion".into(), json!(source.candidate_version));
    handoff_row.insert("legalName".into(), json!(source.legal_name));
    handoff_row.insert("email".into(), json!(source.email));
    handoff_row.insert("jobTitle".into(), json!(source.job_title));
    handoff_row.insert("currency".into(), json!(source.currency));
    handoff_row.insert("baseAmountMinor".into(), json!(source.base_amount_minor));
    handoff_row.insert("startDate".into(), json!(source.start_date));
    data.insert("handoff".into(), Value::Object(handoff_row));
    if let Some(receipt) = receipt {
        data.insert("receipt".into(), json!(receipt));
    }
    data.insert("payGroups".into(), json!(groups));
    data.insert("people".into(), json!(people));
    data.insert("canConsume".into(), json!(!consumed && eligible));
    Ok(Value::Object(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_r3_people_core::domain::{
        HrAffectedPerson, HrCalendar, HrEligibility, HrPolicy, HrShiftTotal, HrPersonTotal,
        PayGroupOption, unavailable,
    };

    struct Probe;

    impl PeopleCoreRead for Probe {
        fn today(&self) -> CoreFuture<'_, String> {
            Box::pin(async { Ok("2026-11-28".into()) })
        }
        fn pay_group_options(&self, _: &str) -> CoreFuture<'_, Vec<PayGroupOption>> {
            Box::pin(async {
                Ok(vec![PayGroupOption {
                    id: "pg-1".into(),
                    name: "Standard".into(),
                    currency: "THB".into(),
                    frequency: "monthly".into(),
                }])
            })
        }
        fn engagement(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
        ) -> CoreFuture<'_, Option<super::super::domain::HrEngagement>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn assignments(
            &self,
            _: &str,
            _: &str,
        ) -> CoreFuture<'_, Vec<super::super::domain::HrAssignment>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn policies(&self, _: &str) -> CoreFuture<'_, Vec<HrPolicy>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn calendars(&self, _: &str) -> CoreFuture<'_, Vec<HrCalendar>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn eligibility(&self, _: &str, _: &str) -> CoreFuture<'_, Option<HrEligibility>> {
            Box::pin(async { Ok(None) })
        }
        fn affected_people(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
        ) -> CoreFuture<'_, Vec<HrAffectedPerson>> {
            Box::pin(async {
                Ok(vec![HrAffectedPerson {
                    engagement_id: "e-1".into(),
                    person_id: "p-1".into(),
                    name: "Ada".into(),
                }])
            })
        }
        fn future_shifts(&self, _: &str, _: &str, _: i64) -> CoreFuture<'_, Vec<HrShiftTotal>> {
            Box::pin(async { Ok(vec![HrShiftTotal { engagement_id: "e-1".into(), total: 2 }]) })
        }
        fn future_leaves(&self, _: &str, _: &str, _: &str) -> CoreFuture<'_, Vec<HrPersonTotal>> {
            Box::pin(async { Ok(vec![HrPersonTotal { person_id: "p-1".into(), total: 1 }]) })
        }
        fn current_policies(
            &self,
            _: &str,
            _: &str,
            _: &str,
        ) -> CoreFuture<'_, Vec<super::super::domain::HrCurrentPolicy>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn engagement_groups(
            &self,
            _: &str,
            _: &str,
        ) -> CoreFuture<'_, Vec<super::super::domain::HrEngagementGroup>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn hire_source(&self, _: &str, _: &str) -> CoreFuture<'_, Option<super::super::domain::HireSource>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn hire_receipt(&self, _: &str, _: &str) -> CoreFuture<'_, Option<super::super::domain::HireReceipt>> {
            Box::pin(async { Err(unavailable()) })
        }
        fn same_currency_groups(&self, _: &str, _: &str) -> CoreFuture<'_, Vec<PayGroupOption>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn matching_people(&self, _: &str, _: &str) -> CoreFuture<'_, Vec<super::super::domain::HirePerson>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn offer_application(&self, _: &str, _: &str) -> CoreFuture<'_, Option<String>> {
            Box::pin(async { Ok(None) })
        }
    }

    fn input() -> HrInput {
        HrInput {
            company: "acme-co".into(),
            branch: None,
            department: None,
            permissions: vec![
                "people.manage".into(),
                "organization.manage".into(),
                "people.payroll.details".into(),
                "payroll.prepare".into(),
            ],
            topic: None,
            editor: false,
            engagement: None,
            position: None,
            impact: vec![],
        }
    }

    #[tokio::test]
    async fn topic_list_is_permission_filtered() {
        let data = hr_settings_data(&Probe, &input()).await.unwrap();
        assert_eq!(data["topics"][0], json!("levels"));
        assert!(data["topics"].as_array().is_some_and(|rows| rows.len() == 4));
    }

    #[tokio::test]
    async fn pay_groups_topic_keeps_exact_shape() {
        let mut value = input();
        value.topic = Some("pay-groups".into());
        let data = hr_settings_data(&Probe, &value).await.unwrap();
        assert_eq!(data["topic"], json!("pay-groups"));
        assert_eq!(data["groups"][0]["currency"], json!("THB"));
        assert_eq!(data["groups"][0]["id"], json!("pg-1"));
        assert_eq!(data["canManage"], json!(true));
    }

    #[tokio::test]
    async fn impact_requires_exact_staff_count() {
        let mut value = input();
        value.impact = vec!["e-1".into(), "e-2".into()];
        let error = hr_settings_data(&Probe, &value).await.unwrap_err();
        assert_eq!(error.code, "ENGAGEMENT_NOT_FOUND");
    }
}
