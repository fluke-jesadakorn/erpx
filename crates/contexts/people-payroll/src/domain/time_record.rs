use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AuthorityScope, RecordId};
use serde::{Deserialize, Serialize};

use super::{EmployeeId, PeoplePayrollDomainError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeRecordStatus {
    Draft,
    Submitted,
    Approved,
    Posted,
    Voided,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRecord {
    id: RecordId,
    version: u64,
    scope: AuthorityScope,
    employee_id: EmployeeId,
    employment_record: StableRecordRef,
    work_date: NaiveDate,
    minutes: u16,
    cost_center: String,
    evidence_sha256: String,
    status: TimeRecordStatus,
    submitted_by: String,
    approved_by: Option<String>,
    posted_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TimeRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: RecordId,
        scope: AuthorityScope,
        employee_id: EmployeeId,
        employment_record: StableRecordRef,
        work_date: NaiveDate,
        minutes: u16,
        cost_center: String,
        evidence_sha256: String,
        submitted_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PeoplePayrollDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            employee_id,
            employment_record,
            work_date,
            minutes,
            cost_center,
            evidence_sha256,
            status: TimeRecordStatus::Draft,
            submitted_by,
            approved_by: None,
            posted_by: None,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn transition(
        &mut self,
        next: TimeRecordStatus,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        let allowed = matches!(
            (self.status, next),
            (
                TimeRecordStatus::Draft,
                TimeRecordStatus::Submitted | TimeRecordStatus::Voided
            ) | (
                TimeRecordStatus::Submitted,
                TimeRecordStatus::Approved | TimeRecordStatus::Voided
            ) | (
                TimeRecordStatus::Approved,
                TimeRecordStatus::Posted | TimeRecordStatus::Voided
            )
        );
        if !allowed
            || actor.trim().is_empty()
            || at < self.updated_at
            || (next == TimeRecordStatus::Approved && actor == self.submitted_by)
            || (next == TimeRecordStatus::Posted
                && self.approved_by.as_deref() == Some(actor.as_str()))
        {
            return Err(PeoplePayrollDomainError::InvalidTimeRecord);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PeoplePayrollDomainError::VersionOverflow)?;
        self.updated_at = at;
        self.status = next;
        if next == TimeRecordStatus::Approved {
            self.approved_by = Some(actor);
        } else if next == TimeRecordStatus::Posted {
            self.posted_by = Some(actor);
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), PeoplePayrollDomainError> {
        if self.version == 0
            || self.scope.validate().is_err()
            || !matches!(
                self.scope,
                AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
            )
            || self.employment_record.owner_context() != ContextId::PeoplePayroll
            || self.employment_record.record_type().as_str() != "people.employment"
            || self.employment_record.validate().is_err()
            || self.minutes == 0
            || self.minutes > 1_440
            || self.cost_center.trim().is_empty()
            || self.cost_center.len() > 128
            || self.submitted_by.trim().is_empty()
            || !valid_digest(&self.evidence_sha256)
            || self.updated_at < self.created_at
        {
            return Err(PeoplePayrollDomainError::InvalidTimeRecord);
        }
        Ok(())
    }

    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        StableRecordRef::try_new(
            self.id,
            ContextId::PeoplePayroll,
            RecordTypeId::try_new("people.time-record")?,
        )
    }
    pub const fn id(&self) -> RecordId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn employee_id(&self) -> EmployeeId {
        self.employee_id
    }
    pub const fn employment_record(&self) -> &StableRecordRef {
        &self.employment_record
    }
    pub const fn work_date(&self) -> NaiveDate {
        self.work_date
    }
    pub const fn minutes(&self) -> u16 {
        self.minutes
    }
    pub fn cost_center(&self) -> &str {
        &self.cost_center
    }
    pub fn evidence_sha256(&self) -> &str {
        &self.evidence_sha256
    }
    pub const fn status(&self) -> TimeRecordStatus {
        self.status
    }
    pub fn submitted_by(&self) -> &str {
        &self.submitted_by
    }
    pub fn approved_by(&self) -> Option<&str> {
        self.approved_by.as_deref()
    }
    pub fn posted_by(&self) -> Option<&str> {
        self.posted_by.as_deref()
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use uuid::Uuid;
    #[test]
    fn time_record_enforces_submit_approve_post_separation() {
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 9, 0, 0).single().unwrap();
        let employment = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::PeoplePayroll,
            RecordTypeId::try_new("people.employment").unwrap(),
        )
        .unwrap();
        let mut record = TimeRecord::create(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            AuthorityScope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
            },
            EmployeeId::try_from_uuid(Uuid::from_u128(12)).unwrap(),
            employment,
            NaiveDate::from_ymd_opt(2026, 8, 16).unwrap(),
            480,
            "OPS".into(),
            format!("sha256:{}", "a".repeat(64)),
            "submitter".into(),
            at,
        )
        .unwrap();
        record
            .transition(
                TimeRecordStatus::Submitted,
                "submitter".into(),
                at + Duration::minutes(1),
            )
            .unwrap();
        assert!(
            record
                .transition(
                    TimeRecordStatus::Approved,
                    "submitter".into(),
                    at + Duration::minutes(2)
                )
                .is_err()
        );
        record
            .transition(
                TimeRecordStatus::Approved,
                "approver".into(),
                at + Duration::minutes(2),
            )
            .unwrap();
        assert!(
            record
                .transition(
                    TimeRecordStatus::Posted,
                    "approver".into(),
                    at + Duration::minutes(3)
                )
                .is_err()
        );
        record
            .transition(
                TimeRecordStatus::Posted,
                "payroll".into(),
                at + Duration::minutes(3),
            )
            .unwrap();
        assert_eq!(record.version(), 4);
    }
}
