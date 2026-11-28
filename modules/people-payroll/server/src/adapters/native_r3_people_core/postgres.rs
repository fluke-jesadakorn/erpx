//! Real SQLx execution for the native R3 People core slices. Every statement
//! below is an owner-owned read: Company-scoped predicates, bind order and
//! ordering mirror the retired TypeScript adapters with only `?` renumbered to
//! `$1..$n`. No TypeScript fallback and no plan-only SQL are part of this file.

use super::application::{CoreFuture, PeopleCoreRead};
use super::domain::{
    HrAffectedPerson, HrAssignment, HrCalendar, HrCurrentPolicy, HrEligibility,
    HrEngagement, HrEngagementGroup, HrPersonTotal, HrPolicy, HrShiftTotal,
    HirePerson, HireReceipt, HireSource, PayGroupOption, unavailable, valid_id,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use sqlx::Row;

pub const SQL_TODAY: &str =
    "SELECT to_char(now() AT TIME ZONE 'Asia/Bangkok','YYYY-MM-DD') day";

pub const SQL_PAY_GROUPS: &str =
    "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=$1 AND status='active' ORDER BY name";

pub const SQL_ENGAGEMENT: &str =
    "SELECT id,person_id,branch_id,department_id FROM hr_engagements WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4)";

pub const SQL_ASSIGNMENTS: &str =
    "SELECT a.id,a.policy_version_id,a.holiday_calendar_id,a.effective_from,a.effective_to,a.version,p.name policy,p.workweek_json,p.timezone,c.name calendar FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id LEFT JOIN leave_holiday_calendars c ON c.id=a.holiday_calendar_id WHERE a.company_id=$1 AND a.engagement_id=$2 ORDER BY a.effective_from DESC";

pub const SQL_POLICIES: &str =
    "SELECT id,name,version,effective_from FROM leave_policy_versions WHERE company_id=$1 AND status='active' ORDER BY name,version DESC";

pub const SQL_CALENDARS: &str =
    "SELECT id,name FROM leave_holiday_calendars WHERE company_id=$1 AND active=true ORDER BY name";

pub const SQL_ELIGIBILITY: &str =
    "SELECT o.eligible,o.version,e.self_service_top_level top_level FROM hr_engagements e LEFT JOIN overtime_eligibility o ON o.engagement_id=e.id AND o.company_id=e.company_id WHERE e.id=$1 AND e.company_id=$2";

pub const SQL_AFFECTED_PEOPLE: &str =
    "SELECT e.id engagement_id,e.person_id,p.legal_name name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=$1 AND e.id IN (SELECT jsonb_array_elements_text($2::jsonb)) AND ($3::text IS NULL OR e.branch_id=$3) AND ($4::text IS NULL OR e.department_id=$4) ORDER BY p.legal_name,e.id";

pub const SQL_FUTURE_SHIFTS: &str =
    "SELECT a.engagement_id,count(*)::int total FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id WHERE w.company_id=$1 AND a.engagement_id IN (SELECT jsonb_array_elements_text($2::jsonb)) AND a.state='assigned' AND s.state='active' AND s.ends_at>$3 GROUP BY a.engagement_id";

pub const SQL_FUTURE_LEAVES: &str =
    "SELECT coalesce(l.person_id,e.person_id) person_id,count(*)::int total FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=$1 AND l.state='approved' AND coalesce(l.end_date::date,l.start_date::date+(l.days::int-1))>=$2::date AND coalesce(l.person_id,e.person_id) IN (SELECT person_id FROM hr_engagements WHERE company_id=$3 AND id IN (SELECT jsonb_array_elements_text($4::jsonb))) GROUP BY coalesce(l.person_id,e.person_id)";

pub const SQL_CURRENT_POLICIES: &str =
    "SELECT DISTINCT ON (a.engagement_id) a.engagement_id,p.name policy,c.name calendar FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id LEFT JOIN leave_holiday_calendars c ON c.id=a.holiday_calendar_id WHERE a.company_id=$1 AND a.engagement_id IN (SELECT jsonb_array_elements_text($2::jsonb)) AND a.effective_from<=$3 AND coalesce(a.effective_to,'9999-12-31')>=$4 ORDER BY a.engagement_id,a.effective_from DESC";

pub const SQL_ENGAGEMENT_GROUPS: &str =
    "SELECT e.id engagement_id,g.name FROM hr_engagements e LEFT JOIN hr_pay_groups g ON g.id=e.pay_group_id AND g.company_id=e.company_id WHERE e.company_id=$1 AND e.id IN (SELECT jsonb_array_elements_text($2::jsonb))";

/// Accepted hire source row. Date columns are cast to text so the owner DTO
/// carries the same `YYYY-MM-DD` strings the TypeScript adapter returned.
pub const SQL_HIRE_SOURCE: &str =
    "SELECT h.id,h.offer_id,o.version offer_version,o.status offer_status,c.version candidate_version,c.legal_name,c.email,r.job_title,o.currency,o.base_amount_minor,o.start_date::text start_date,o.source_offer_version,c.status candidate_status,c.retention_until::text retention_until,c.consent_evidence,o.accepted_evidence FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id JOIN talent_candidates c ON c.id=h.candidate_id AND c.id=a.candidate_id AND c.company_id=h.company_id WHERE h.id=$1 AND h.company_id=$2";

pub const SQL_HIRE_RECEIPT: &str =
    "SELECT id,person_id,engagement_id,compensation_id,source_offer_version,source_candidate_version,created_at FROM talent_hire_consumption_receipts WHERE handoff_id=$1 AND company_id=$2";

pub const SQL_SAME_CURRENCY_GROUPS: &str =
    "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=$1 AND currency=$2 AND status='active' ORDER BY name,id";

pub const SQL_MATCHING_PEOPLE: &str =
    "SELECT id,legal_name,version,status,merged_into_id FROM hr_people WHERE company_id=$1 AND lower(email)=lower($2) ORDER BY id";

pub const SQL_OFFER_APPLICATION: &str =
    "SELECT application_id FROM talent_offers WHERE id=$1 AND company_id=$2";

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    row.try_get::<String, _>(column).map_err(|_| unavailable())
}

fn opt_text(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>, BusinessError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|_| unavailable())
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        })
        .ok_or_else(unavailable)
}

fn opt_integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<i64>, BusinessError> {
    Ok(row
        .try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        }))
}

fn workweek(row: &sqlx::postgres::PgRow) -> Result<Vec<i64>, BusinessError> {
    if let Ok(value) = row.try_get::<serde_json::Value, _>("workweek_json") {
        match value {
            Value::Array(items) => {
                return items
                    .into_iter()
                    .map(|item| {
                        item.as_i64().ok_or_else(unavailable)
                    })
                    .collect();
            }
            Value::String(raw) => {
                return serde_json::from_str::<Vec<i64>>(&raw).map_err(|_| unavailable());
            }
            _ => return Err(unavailable()),
        }
    }
    row.try_get::<Vec<i32>, _>("workweek_json")
        .map(|values| values.into_iter().map(i64::from).collect())
        .or_else(|_| row.try_get::<Vec<i64>, _>("workweek_json"))
        .map_err(|_| unavailable())
}

fn group(row: &sqlx::postgres::PgRow) -> Result<PayGroupOption, BusinessError> {
    Ok(PayGroupOption {
        id: text(row, "id")?,
        name: text(row, "name")?,
        currency: text(row, "currency")?,
        frequency: text(row, "frequency")?,
    })
}

pub struct PgPeopleCoreRead {
    pool: DatabasePool,
}

impl PgPeopleCoreRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PeopleCoreRead for PgPeopleCoreRead {
    fn today(&self) -> CoreFuture<'_, String> {
        Box::pin(async move {
            sqlx::query(SQL_TODAY)
                .fetch_one(self.pool.raw())
                .await
                .map_err(|_| unavailable())
                .and_then(|row| text(&row, "day"))
        })
    }

    fn pay_group_options(&self, company: &str) -> CoreFuture<'_, Vec<PayGroupOption>> {
        let company = company.to_owned();
        Box::pin(async move {
            if !valid_id(&company) {
                return Err(unavailable());
            }
            let rows = sqlx::query(SQL_PAY_GROUPS)
                .bind(company.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter().map(group).collect()
        })
    }

    fn engagement(
        &self,
        company: &str,
        engagement: &str,
        branch: Option<&str>,
        department: Option<&str>,
    ) -> CoreFuture<'_, Option<HrEngagement>> {
        let (company, engagement) = (company.to_owned(), engagement.to_owned());
        let (branch, department) = (branch.map(str::to_owned), department.map(str::to_owned));
        Box::pin(async move {
            let row = sqlx::query(SQL_ENGAGEMENT)
                .bind(engagement.as_str())
                .bind(company.as_str())
                .bind(branch.as_deref())
                .bind(department.as_deref())
                .fetch_optional(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(HrEngagement {
                    id: text(&row, "id")?,
                    person_id: text(&row, "person_id")?,
                    branch_id: opt_text(&row, "branch_id")?,
                    department_id: opt_text(&row, "department_id")?,
                })
            })
            .transpose()
        })
    }

    fn assignments(
        &self,
        company: &str,
        engagement: &str,
    ) -> CoreFuture<'_, Vec<HrAssignment>> {
        let (company, engagement) = (company.to_owned(), engagement.to_owned());
        Box::pin(async move {
            let rows = sqlx::query(SQL_ASSIGNMENTS)
                .bind(company.as_str())
                .bind(engagement.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HrAssignment {
                        id: text(row, "id")?,
                        policy_version_id: text(row, "policy_version_id")?,
                        holiday_calendar_id: opt_text(row, "holiday_calendar_id")?,
                        effective_from: text(row, "effective_from")?,
                        effective_to: opt_text(row, "effective_to")?,
                        version: integer(row, "version")?,
                        policy: text(row, "policy")?,
                        workweek_json: workweek(row)?,
                        timezone: text(row, "timezone")?,
                        calendar: opt_text(row, "calendar")?,
                    })
                })
                .collect()
        })
    }

    fn policies(&self, company: &str) -> CoreFuture<'_, Vec<HrPolicy>> {
        let company = company.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQL_POLICIES)
                .bind(company.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HrPolicy {
                        id: text(row, "id")?,
                        name: text(row, "name")?,
                        version: integer(row, "version")?,
                        effective_from: text(row, "effective_from")?,
                    })
                })
                .collect()
        })
    }

    fn calendars(&self, company: &str) -> CoreFuture<'_, Vec<HrCalendar>> {
        let company = company.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQL_CALENDARS)
                .bind(company.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HrCalendar {
                        id: text(row, "id")?,
                        name: text(row, "name")?,
                    })
                })
                .collect()
        })
    }

    fn eligibility(
        &self,
        company: &str,
        engagement: &str,
    ) -> CoreFuture<'_, Option<HrEligibility>> {
        let (company, engagement) = (company.to_owned(), engagement.to_owned());
        Box::pin(async move {
            let row = sqlx::query(SQL_ELIGIBILITY)
                .bind(engagement.as_str())
                .bind(company.as_str())
                .fetch_optional(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(HrEligibility {
                    eligible: row
                        .try_get::<Option<bool>, _>("eligible")
                        .map_err(|_| unavailable())?,
                    version: opt_integer(&row, "version")?,
                    top_level: row
                        .try_get::<Option<bool>, _>("top_level")
                        .map_err(|_| unavailable())?
                        .unwrap_or(false),
                })
            })
            .transpose()
        })
    }

    fn affected_people(
        &self,
        company: &str,
        ids: &str,
        branch: Option<&str>,
        department: Option<&str>,
    ) -> CoreFuture<'_, Vec<HrAffectedPerson>> {
        let (company, ids) = (company.to_owned(), ids.to_owned());
        let (branch, department) = (branch.map(str::to_owned), department.map(str::to_owned));
        Box::pin(async move {
            let rows = sqlx::query(SQL_AFFECTED_PEOPLE)
                .bind(company.as_str())
                .bind(ids.as_str())
                .bind(branch.as_deref())
                .bind(department.as_deref())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HrAffectedPerson {
                        engagement_id: text(row, "engagement_id")?,
                        person_id: text(row, "person_id")?,
                        name: text(row, "name")?,
                    })
                })
                .collect()
        })
    }

    fn future_shifts(
        &self,
        company: &str,
        ids: &str,
        now_ms: i64,
    ) -> CoreFuture<'_, Vec<HrShiftTotal>> {
        let (company, ids) = (company.to_owned(), ids.to_owned());
        Box::pin(async move {
            let rows = sqlx::query(SQL_FUTURE_SHIFTS)
                .bind(company.as_str())
                .bind(ids.as_str())
                .bind(now_ms)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HrShiftTotal {
                        engagement_id: text(row, "engagement_id")?,
                        total: integer(row, "total")?,
                    })
                })
                .collect()
        })
    }

    fn future_leaves(
        &self,
        company: &str,
        today: &str,
        ids: &str,
    ) -> CoreFuture<'_, Vec<HrPersonTotal>> {
        let (company, today, ids) = (
            company.to_owned(),
            today.to_owned(),
            ids.to_owned(),
        );
        Box::pin(async move {
            let rows = sqlx::query(SQL_FUTURE_LEAVES)
                .bind(company.as_str())
                .bind(today.as_str())
                .bind(company.as_str())
                .bind(ids.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HrPersonTotal {
                        person_id: text(row, "person_id")?,
                        total: integer(row, "total")?,
                    })
                })
                .collect()
        })
    }

    fn current_policies(
        &self,
        company: &str,
        ids: &str,
        today: &str,
    ) -> CoreFuture<'_, Vec<HrCurrentPolicy>> {
        let (company, ids, today) = (
            company.to_owned(),
            ids.to_owned(),
            today.to_owned(),
        );
        Box::pin(async move {
            let rows = sqlx::query(SQL_CURRENT_POLICIES)
                .bind(company.as_str())
                .bind(ids.as_str())
                .bind(today.as_str())
                .bind(today.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HrCurrentPolicy {
                        engagement_id: text(row, "engagement_id")?,
                        policy: text(row, "policy")?,
                        calendar: opt_text(row, "calendar")?,
                    })
                })
                .collect()
        })
    }

    fn engagement_groups(
        &self,
        company: &str,
        ids: &str,
    ) -> CoreFuture<'_, Vec<HrEngagementGroup>> {
        let (company, ids) = (company.to_owned(), ids.to_owned());
        Box::pin(async move {
            let rows = sqlx::query(SQL_ENGAGEMENT_GROUPS)
                .bind(company.as_str())
                .bind(ids.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HrEngagementGroup {
                        engagement_id: text(row, "engagement_id")?,
                        name: opt_text(row, "name")?,
                    })
                })
                .collect()
        })
    }

    fn hire_source(
        &self,
        company: &str,
        handoff: &str,
    ) -> CoreFuture<'_, Option<HireSource>> {
        let (company, handoff) = (company.to_owned(), handoff.to_owned());
        Box::pin(async move {
            let row = sqlx::query(SQL_HIRE_SOURCE)
                .bind(handoff.as_str())
                .bind(company.as_str())
                .fetch_optional(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(HireSource {
                    id: text(&row, "id")?,
                    offer_id: text(&row, "offer_id")?,
                    application_id: None,
                    offer_version: integer(&row, "offer_version")?,
                    candidate_version: integer(&row, "candidate_version")?,
                    legal_name: text(&row, "legal_name")?,
                    email: text(&row, "email")?,
                    job_title: text(&row, "job_title")?,
                    currency: text(&row, "currency")?,
                    base_amount_minor: integer(&row, "base_amount_minor")?,
                    start_date: text(&row, "start_date")?,
                    offer_status: text(&row, "offer_status")?,
                    source_offer_version: integer(&row, "source_offer_version")?,
                    candidate_status: text(&row, "candidate_status")?,
                    retention_until: text(&row, "retention_until")?,
                    consent_evidence: text(&row, "consent_evidence")?,
                    accepted_evidence: opt_text(&row, "accepted_evidence")?,
                })
            })
            .transpose()
        })
    }

    fn hire_receipt(
        &self,
        company: &str,
        handoff: &str,
    ) -> CoreFuture<'_, Option<HireReceipt>> {
        let (company, handoff) = (company.to_owned(), handoff.to_owned());
        Box::pin(async move {
            let row = sqlx::query(SQL_HIRE_RECEIPT)
                .bind(handoff.as_str())
                .bind(company.as_str())
                .fetch_optional(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(HireReceipt {
                    id: text(&row, "id")?,
                    person_id: text(&row, "person_id")?,
                    engagement_id: text(&row, "engagement_id")?,
                    compensation_id: text(&row, "compensation_id")?,
                    source_offer_version: integer(&row, "source_offer_version")?,
                    source_candidate_version: integer(&row, "source_candidate_version")?,
                    created_at: integer(&row, "created_at")?,
                })
            })
            .transpose()
        })
    }

    fn same_currency_groups(
        &self,
        company: &str,
        currency: &str,
    ) -> CoreFuture<'_, Vec<PayGroupOption>> {
        let (company, currency) = (company.to_owned(), currency.to_owned());
        Box::pin(async move {
            let rows = sqlx::query(SQL_SAME_CURRENCY_GROUPS)
                .bind(company.as_str())
                .bind(currency.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter().map(group).collect()
        })
    }

    fn matching_people(
        &self,
        company: &str,
        email: &str,
    ) -> CoreFuture<'_, Vec<HirePerson>> {
        let (company, email) = (company.to_owned(), email.to_owned());
        Box::pin(async move {
            let rows = sqlx::query(SQL_MATCHING_PEOPLE)
                .bind(company.as_str())
                .bind(email.as_str())
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(HirePerson {
                        id: text(row, "id")?,
                        legal_name: text(row, "legal_name")?,
                        version: integer(row, "version")?,
                        status: text(row, "status")?,
                        merged_into_id: opt_text(row, "merged_into_id")?,
                    })
                })
                .collect()
        })
    }

    fn offer_application(&self, company: &str, offer: &str) -> CoreFuture<'_, Option<String>> {
        let (company, offer) = (company.to_owned(), offer.to_owned());
        Box::pin(async move {
            let row = sqlx::query(SQL_OFFER_APPLICATION)
                .bind(offer.as_str())
                .bind(company.as_str())
                .fetch_optional(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| text(&row, "application_id")).transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_are_company_scoped_and_fully_bound() {
        for sql in [
            SQL_PAY_GROUPS,
            SQL_ENGAGEMENT,
            SQL_ASSIGNMENTS,
            SQL_POLICIES,
            SQL_CALENDARS,
            SQL_ELIGIBILITY,
            SQL_AFFECTED_PEOPLE,
            SQL_FUTURE_SHIFTS,
            SQL_FUTURE_LEAVES,
            SQL_CURRENT_POLICIES,
            SQL_ENGAGEMENT_GROUPS,
            SQL_HIRE_SOURCE,
            SQL_HIRE_RECEIPT,
            SQL_SAME_CURRENCY_GROUPS,
            SQL_MATCHING_PEOPLE,
            SQL_OFFER_APPLICATION,
        ] {
            assert!(sql.starts_with("SELECT "));
            assert!(!sql.contains('?'), "owner SQL must use $n binds: {sql}");
            assert!(sql.contains("company_id"), "owner SQL must be Company scoped: {sql}");
        }
        assert!(SQL_HIRE_SOURCE.contains("o.status offer_status"));
        assert!(SQL_HIRE_SOURCE.contains("c.retention_until::text"));
        assert!(SQL_FUTURE_SHIFTS.contains("s.ends_at>$3"));
        assert!(SQL_CURRENT_POLICIES.contains("coalesce(a.effective_to,'9999-12-31')>=$4"));
    }

    #[test]
    fn workweek_parses_both_json_and_postgres_array_text() {
        assert_eq!(
            serde_json::from_str::<Vec<i64>>("[1,2,3]").unwrap(),
            vec![1, 2, 3]
        );
        assert!(serde_json::from_str::<Vec<i64>>("not-json").is_err());
    }
}
