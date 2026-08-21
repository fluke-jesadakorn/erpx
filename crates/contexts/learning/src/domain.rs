use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{RecordVersionRef, StableRecordRef};
use erpx_kernel::{
    ActorKind, AuthorityScope, CommandId, HumanDecisionProof, PrincipalId, SessionId,
};
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
pub struct LearningHumanDecisionEvidence {
    principal_ref: String,
    actor_kind: ActorKind,
    session_id: SessionId,
    authority_scope: AuthorityScope,
    decision_ref: String,
    decided_at: DateTime<Utc>,
}

impl LearningHumanDecisionEvidence {
    fn from_proof(
        decision: &HumanDecisionProof,
        scope: &AuthorityScope,
        expected_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<Self, LearningDomainError> {
        validate_learning_human_decision(decision, scope, expected_ref, at)?;
        Ok(Self {
            principal_ref: decision.principal_ref().to_owned(),
            actor_kind: decision.actor().kind,
            session_id: decision
                .actor()
                .session_id
                .ok_or(LearningDomainError::HumanDecisionRequired)?,
            authority_scope: decision.authority_scope().clone(),
            decision_ref: decision.decision_ref().to_owned(),
            decided_at: decision.decided_at(),
        })
    }

    pub fn decision_ref(&self) -> &str {
        &self.decision_ref
    }
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Course {
    id: CourseId,
    version: u64,
    scope: AuthorityScope,
    title: String,
    passing_basis_points: u16,
    required_attendance_minutes: u32,
    provider: Option<StableRecordRef>,
    status: CourseStatus,
    #[serde(default)]
    publish_decision: Option<LearningHumanDecisionEvidence>,
    updated_at: DateTime<Utc>,
}

impl Course {
    pub fn draft(
        id: CourseId,
        scope: AuthorityScope,
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
            publish_decision: None,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn published(
        id: CourseId,
        scope: AuthorityScope,
        title: impl Into<String>,
        passing_basis_points: u16,
        required_attendance_minutes: u32,
        provider: Option<StableRecordRef>,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<Self, LearningDomainError> {
        let publish_decision = LearningHumanDecisionEvidence::from_proof(
            decision,
            &scope,
            &course_publish_decision_ref(command_id, id, 0),
            at,
        )?;
        let value = Self {
            id,
            version: 1,
            scope,
            title: title.into(),
            passing_basis_points,
            required_attendance_minutes,
            provider,
            status: CourseStatus::Published,
            publish_decision: Some(publish_decision),
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn publish(
        &mut self,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), LearningDomainError> {
        let evidence = LearningHumanDecisionEvidence::from_proof(
            decision,
            &self.scope,
            &course_publish_decision_ref(command_id, self.id, self.version),
            at,
        )?;
        self.transition(CourseStatus::Draft, CourseStatus::Published, at)?;
        self.publish_decision = Some(evidence);
        Ok(())
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
        if self.status == CourseStatus::Published && self.publish_decision.is_none() {
            return Err(LearningDomainError::HumanDecisionRequired);
        }
        Ok(())
    }
    pub const fn id(&self) -> CourseId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn status(&self) -> CourseStatus {
        self.status
    }
    pub fn publish_decision(&self) -> Option<&LearningHumanDecisionEvidence> {
        self.publish_decision.as_ref()
    }
    pub fn validate_publish_decision(
        &self,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), LearningDomainError> {
        let expected = LearningHumanDecisionEvidence::from_proof(
            decision,
            &self.scope,
            &course_publish_decision_ref(command_id, self.id, expected_version),
            at,
        )?;
        if self.publish_decision.as_ref() != Some(&expected) {
            return Err(LearningDomainError::HumanDecisionRequired);
        }
        Ok(())
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
    scope: AuthorityScope,
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
        scope: AuthorityScope,
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
    pub fn scope(&self) -> &AuthorityScope {
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

pub fn validate_learning_human_decision(
    decision: &HumanDecisionProof,
    scope: &AuthorityScope,
    expected_ref: &str,
    at: DateTime<Utc>,
) -> Result<(), LearningDomainError> {
    let actor = decision.actor();
    if !actor.is_human()
        || actor.session_id.is_none()
        || actor.represented_principal_ref.is_some()
        || actor.support_capability_id.is_some()
        || decision.authority_scope() != scope
        || decision.decision_ref() != expected_ref
        || decision.decided_at() != at
    {
        return Err(LearningDomainError::HumanDecisionRequired);
    }
    Ok(())
}

pub fn course_publish_decision_ref(
    command_id: CommandId,
    course_id: CourseId,
    expected_version: u64,
) -> String {
    format!(
        "learning.course.publish:{}:{}:{expected_version}",
        command_id.as_uuid(),
        course_id.as_uuid()
    )
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
    #[error("an exact authenticated human decision is required")]
    HumanDecisionRequired,
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
