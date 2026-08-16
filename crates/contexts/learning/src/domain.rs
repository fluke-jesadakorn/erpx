use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{RecordVersionRef, StableRecordRef};
use erpx_kernel::{PrincipalId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

macro_rules! learning_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);
        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, LearningDomainError> {
                if value.is_nil() || value.get_version_num() != 4 {
                    return Err(LearningDomainError::InvalidIdentifier($field));
                }
                Ok(Self(value))
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}
learning_id!(CourseId, "course_id");
learning_id!(AssignmentId, "assignment_id");
learning_id!(EnrollmentId, "enrollment_id");
learning_id!(LearningSessionId, "session_id");
learning_id!(AttendanceFactId, "attendance_fact_id");
learning_id!(ProviderResultId, "provider_result_id");
learning_id!(WaiverId, "waiver_id");
learning_id!(CompletionId, "completion_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourseStatus {
    Draft,
    Published,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Course {
    id: CourseId,
    version: u64,
    scope: TenantScope,
    title: String,
    passing_basis_points: u16,
    required_attendance_minutes: u32,
    provider: Option<StableRecordRef>,
    status: CourseStatus,
    updated_at: DateTime<Utc>,
}

impl Course {
    pub fn draft(
        id: CourseId,
        scope: TenantScope,
        title: impl Into<String>,
        passing_basis_points: u16,
        required_attendance_minutes: u32,
        provider: Option<StableRecordRef>,
        at: DateTime<Utc>,
    ) -> Result<Self, LearningDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            title: title.into(),
            passing_basis_points,
            required_attendance_minutes,
            provider,
            status: CourseStatus::Draft,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn published(
        id: CourseId,
        scope: TenantScope,
        title: impl Into<String>,
        passing_basis_points: u16,
        required_attendance_minutes: u32,
        provider: Option<StableRecordRef>,
        at: DateTime<Utc>,
    ) -> Result<Self, LearningDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            title: title.into(),
            passing_basis_points,
            required_attendance_minutes,
            provider,
            status: CourseStatus::Published,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn publish(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        self.transition(CourseStatus::Draft, CourseStatus::Published, at)
    }
    pub fn retire(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        self.transition(CourseStatus::Published, CourseStatus::Retired, at)
    }
    fn transition(
        &mut self,
        from: CourseStatus,
        to: CourseStatus,
        at: DateTime<Utc>,
    ) -> Result<(), LearningDomainError> {
        if self.status != from || at < self.updated_at {
            return Err(LearningDomainError::InvalidTransition);
        }
        self.status = to;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(LearningDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), LearningDomainError> {
        self.scope.validate()?;
        if self.title.trim().is_empty()
            || self.title.len() > 255
            || self.passing_basis_points > 10_000
            || self.required_attendance_minutes == 0
            || self.version == 0
        {
            return Err(LearningDomainError::InvalidCourse);
        }
        if let Some(provider) = &self.provider {
            provider.validate()?;
        }
        Ok(())
    }
    pub const fn id(&self) -> CourseId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn status(&self) -> CourseStatus {
        self.status
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourseRef {
    pub course_id: CourseId,
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrollmentStatus {
    Assigned,
    InProgress,
    Completed,
    Waived,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttendanceFact {
    pub id: AttendanceFactId,
    pub session_id: LearningSessionId,
    pub attended_minutes: u32,
    pub recorded_by: PrincipalId,
    pub evidence: RecordVersionRef,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderResult {
    pub id: ProviderResultId,
    pub provider_attempt_id: String,
    pub score_basis_points: u16,
    pub passed: bool,
    pub evidence: RecordVersionRef,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiver {
    pub id: WaiverId,
    pub authorized_by: PrincipalId,
    pub reason: String,
    pub evidence: RecordVersionRef,
    pub granted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completion {
    pub id: CompletionId,
    pub completed_at: DateTime<Utc>,
    pub attendance_minutes: u32,
    pub passing_result_id: Option<ProviderResultId>,
    pub waiver_id: Option<WaiverId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enrollment {
    id: EnrollmentId,
    version: u64,
    scope: TenantScope,
    assignment_id: AssignmentId,
    learner: RecordVersionRef,
    learner_principal: PrincipalId,
    course: CourseRef,
    due_on: NaiveDate,
    status: EnrollmentStatus,
    attendance: BTreeMap<AttendanceFactId, AttendanceFact>,
    provider_results: BTreeMap<ProviderResultId, ProviderResult>,
    waiver: Option<Waiver>,
    completion: Option<Completion>,
    updated_at: DateTime<Utc>,
}

impl Enrollment {
    #[allow(clippy::too_many_arguments)]
    pub fn assign(
        id: EnrollmentId,
        assignment_id: AssignmentId,
        scope: TenantScope,
        learner: RecordVersionRef,
        learner_principal: PrincipalId,
        course: &Course,
        due_on: NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<Self, LearningDomainError> {
        if course.status != CourseStatus::Published
            || scope != *course.scope()
            || due_on < at.date_naive()
        {
            return Err(LearningDomainError::InvalidAssignment);
        }
        learner.validate()?;
        let value = Self {
            id,
            version: 1,
            scope,
            assignment_id,
            learner,
            learner_principal,
            course: CourseRef {
                course_id: course.id,
                version: course.version,
            },
            due_on,
            status: EnrollmentStatus::Assigned,
            attendance: BTreeMap::new(),
            provider_results: BTreeMap::new(),
            waiver: None,
            completion: None,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn record_attendance(
        &mut self,
        actor: PrincipalId,
        fact: AttendanceFact,
    ) -> Result<bool, LearningDomainError> {
        if actor == self.learner_principal || actor != fact.recorded_by {
            return Err(LearningDomainError::SelfAttendanceEdit);
        }
        if !matches!(
            self.status,
            EnrollmentStatus::Assigned | EnrollmentStatus::InProgress
        ) || fact.attended_minutes == 0
            || fact.at < self.updated_at
        {
            return Err(LearningDomainError::InvalidAttendance);
        }
        fact.evidence.validate()?;
        if self.attendance.contains_key(&fact.id) {
            return Ok(false);
        }
        self.status = EnrollmentStatus::InProgress;
        let at = fact.at;
        self.attendance.insert(fact.id, fact);
        self.advance(at)?;
        Ok(true)
    }
    pub fn record_provider_result(
        &mut self,
        value: ProviderResult,
    ) -> Result<bool, LearningDomainError> {
        if !matches!(
            self.status,
            EnrollmentStatus::Assigned | EnrollmentStatus::InProgress
        ) || value.provider_attempt_id.trim().is_empty()
            || value.provider_attempt_id.len() > 128
            || value.score_basis_points > 10_000
            || value.received_at < self.updated_at
        {
            return Err(LearningDomainError::InvalidProviderResult);
        }
        value.evidence.validate()?;
        if self.provider_results.contains_key(&value.id)
            || self
                .provider_results
                .values()
                .any(|existing| existing.provider_attempt_id == value.provider_attempt_id)
        {
            return Ok(false);
        }
        self.status = EnrollmentStatus::InProgress;
        let at = value.received_at;
        self.provider_results.insert(value.id, value);
        self.advance(at)?;
        Ok(true)
    }
    pub fn waive(&mut self, actor: PrincipalId, value: Waiver) -> Result<(), LearningDomainError> {
        if actor == self.learner_principal || actor != value.authorized_by {
            return Err(LearningDomainError::SelfWaiver);
        }
        if !matches!(
            self.status,
            EnrollmentStatus::Assigned | EnrollmentStatus::InProgress
        ) || value.reason.trim().len() < 8
            || value.reason.len() > 500
            || value.granted_at < self.updated_at
        {
            return Err(LearningDomainError::InvalidWaiver);
        }
        value.evidence.validate()?;
        let at = value.granted_at;
        self.waiver = Some(value);
        self.status = EnrollmentStatus::Waived;
        self.advance(at)
    }
    pub fn complete(
        &mut self,
        course: &Course,
        id: CompletionId,
        at: DateTime<Utc>,
    ) -> Result<(), LearningDomainError> {
        if self.status != EnrollmentStatus::InProgress
            || course.id != self.course.course_id
            || course.version != self.course.version
            || course.status != CourseStatus::Published
            || at < self.updated_at
        {
            return Err(LearningDomainError::InvalidCompletion);
        }
        let attendance = self.attendance.values().try_fold(0_u32, |total, value| {
            total
                .checked_add(value.attended_minutes)
                .ok_or(LearningDomainError::Overflow)
        })?;
        let result = self
            .provider_results
            .values()
            .find(|value| value.passed && value.score_basis_points >= course.passing_basis_points)
            .map(|value| value.id);
        if attendance < course.required_attendance_minutes || result.is_none() {
            return Err(LearningDomainError::CompletionRequirementsNotMet);
        }
        self.completion = Some(Completion {
            id,
            completed_at: at,
            attendance_minutes: attendance,
            passing_result_id: result,
            waiver_id: None,
        });
        self.status = EnrollmentStatus::Completed;
        self.advance(at)
    }
    pub fn completion_fact(&self) -> Result<LearningCompletionFact, LearningDomainError> {
        let completion = self
            .completion
            .as_ref()
            .ok_or(LearningDomainError::InvalidCompletion)?;
        Ok(LearningCompletionFact {
            enrollment_id: self.id,
            enrollment_version: self.version,
            employee_record_id: self.learner.record().id().as_uuid(),
            course_id: self.course.course_id,
            course_version: self.course.version,
            completed_at: completion.completed_at,
        })
    }
    pub fn validate(&self) -> Result<(), LearningDomainError> {
        self.scope.validate()?;
        self.learner.validate()?;
        if self.version == 0 || self.course.version == 0 {
            return Err(LearningDomainError::InvalidAssignment);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), LearningDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(LearningDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> EnrollmentId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn status(&self) -> EnrollmentStatus {
        self.status
    }
    pub const fn assignment_id(&self) -> AssignmentId {
        self.assignment_id
    }
    pub const fn learner(&self) -> &RecordVersionRef {
        &self.learner
    }
    pub const fn learner_principal(&self) -> PrincipalId {
        self.learner_principal
    }
    pub const fn course(&self) -> CourseRef {
        self.course
    }
    pub const fn due_on(&self) -> NaiveDate {
        self.due_on
    }
    pub const fn completion(&self) -> Option<&Completion> {
        self.completion.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearningCompletionFact {
    pub enrollment_id: EnrollmentId,
    pub enrollment_version: u64,
    pub employee_record_id: Uuid,
    pub course_id: CourseId,
    pub course_version: u64,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LearningDomainError {
    #[error("invalid learning identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid learning scope")]
    InvalidScope,
    #[error("invalid record reference")]
    InvalidRecord,
    #[error("invalid course")]
    InvalidCourse,
    #[error("invalid assignment or enrollment")]
    InvalidAssignment,
    #[error("invalid course lifecycle transition")]
    InvalidTransition,
    #[error("a learner cannot record or edit their own attendance")]
    SelfAttendanceEdit,
    #[error("attendance fact is invalid")]
    InvalidAttendance,
    #[error("provider result is invalid")]
    InvalidProviderResult,
    #[error("a learner cannot waive their own requirement")]
    SelfWaiver,
    #[error("waiver is invalid")]
    InvalidWaiver,
    #[error("completion is invalid")]
    InvalidCompletion,
    #[error("completion requirements are not met")]
    CompletionRequirementsNotMet,
    #[error("numeric overflow")]
    Overflow,
}
impl From<erpx_kernel::ScopeError> for LearningDomainError {
    fn from(_: erpx_kernel::ScopeError) -> Self {
        Self::InvalidScope
    }
}
impl From<erpx_integration_contracts::ReferenceValidationError> for LearningDomainError {
    fn from(_: erpx_integration_contracts::ReferenceValidationError) -> Self {
        Self::InvalidRecord
    }
}

#[cfg(test)]
mod tests;
