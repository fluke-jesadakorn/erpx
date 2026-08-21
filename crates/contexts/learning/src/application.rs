use chrono::{DateTime, Utc};
use erpx_kernel::HumanDecisionProof;
use thiserror::Error;

use crate::{
    domain::{CompletionId, Course, CourseId, Enrollment, EnrollmentId, LearningDomainError},
    ports::{
        ActorContext, AuthorizationPort, CommitReceipt, LearningAction, LearningAggregate,
        LearningCommit, LearningReadRepository, LearningWriteRepository, MutationIdentity,
        PortError,
    },
};

pub struct LearningCommandService<A, W> {
    authorization: A,
    write: W,
}
impl<A, W> LearningCommandService<A, W>
where
    A: AuthorizationPort,
    W: LearningWriteRepository,
{
    pub const fn new(authorization: A, write: W) -> Self {
        Self {
            authorization,
            write,
        }
    }
    pub async fn publish_course(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        course: Course,
        decision: HumanDecisionProof,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, LearningError> {
        identity.validate()?;
        if decision.principal_ref() != actor.principal_id.as_uuid().to_string() {
            return Err(LearningError::Neutral);
        }
        course.validate_publish_decision(&decision, identity.command_id, 0, at)?;
        if course.status() != crate::CourseStatus::Published || course.version() != 1 {
            return Err(LearningError::Invalid(
                "a newly published course must begin at version 1".into(),
            ));
        }
        let authority = self
            .authorization
            .authorize(actor, course.scope(), LearningAction::ManageCourse)
            .await?;
        if self
            .write
            .load_course(&authority, course.id())
            .await?
            .is_some()
        {
            return Err(LearningError::Conflict);
        }
        self.write
            .commit(LearningCommit {
                identity,
                authority,
                expected_version: None,
                event_type: "learning.course_published".into(),
                event_payload: serde_json::json!({"course_id": course.id().as_uuid()}),
                aggregate: LearningAggregate::Course(Box::new(course)),
            })
            .await
            .map_err(Into::into)
    }

    pub async fn retire_course(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        course_id: CourseId,
        scope: &erpx_kernel::AuthorityScope,
        requested_version: Option<u64>,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, LearningError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, LearningAction::ManageCourse)
            .await?;
        let mut course = self
            .write
            .load_course(&authority, course_id)
            .await?
            .ok_or(LearningError::Neutral)?;
        if course.scope() != scope || requested_version != Some(course.version()) {
            return Err(LearningError::Conflict);
        }
        let expected = course.version();
        course.retire(at)?;
        self.write
            .commit(LearningCommit {
                identity,
                authority,
                expected_version: Some(expected),
                event_type: "learning.course_retired".into(),
                event_payload: serde_json::json!({"course_id": course.id().as_uuid()}),
                aggregate: LearningAggregate::Course(Box::new(course)),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn assign_course(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        scope: &erpx_kernel::AuthorityScope,
        enrollment_id: EnrollmentId,
        assignment_id: crate::AssignmentId,
        learner: erpx_integration_contracts::RecordVersionRef,
        learner_principal: erpx_kernel::PrincipalId,
        course_id: CourseId,
        course_version: u64,
        due_on: chrono::NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, LearningError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, LearningAction::Assign)
            .await?;
        if self
            .write
            .load_enrollment(&authority, enrollment_id)
            .await?
            .is_some()
        {
            return Err(LearningError::Conflict);
        }
        let course = self
            .write
            .load_course(&authority, course_id)
            .await?
            .ok_or(LearningError::Neutral)?;
        if course.scope() != scope || course.version() != course_version {
            return Err(LearningError::Neutral);
        }
        let enrollment = Enrollment::assign(
            enrollment_id,
            assignment_id,
            scope.clone(),
            learner,
            learner_principal,
            &course,
            due_on,
            at,
        )?;
        self.write
            .commit(LearningCommit {
                identity,
                authority,
                expected_version: None,
                event_type: "learning.course_assigned".into(),
                event_payload: serde_json::json!({"course_id": course_id.as_uuid(), "course_version": course_version}),
                aggregate: LearningAggregate::Enrollment(Box::new(enrollment)),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mutate_enrollment<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        enrollment_id: EnrollmentId,
        scope: &erpx_kernel::AuthorityScope,
        requested_version: Option<u64>,
        action: LearningAction,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, LearningError>
    where
        F: FnOnce(&mut Enrollment, &Course) -> Result<(), LearningDomainError>,
    {
        identity.validate()?;
        let authority = self.authorization.authorize(actor, scope, action).await?;
        let mut enrollment = self
            .write
            .load_enrollment(&authority, enrollment_id)
            .await?
            .ok_or(LearningError::Neutral)?;
        if enrollment.scope() != scope || requested_version != Some(enrollment.version()) {
            return Err(LearningError::Conflict);
        }
        let course = self
            .write
            .load_course(&authority, enrollment.course().course_id)
            .await?
            .ok_or(LearningError::Neutral)?;
        let expected = enrollment.version();
        change(&mut enrollment, &course)?;
        self.write
            .commit(LearningCommit {
                identity,
                authority,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": enrollment.version()}),
                aggregate: LearningAggregate::Enrollment(Box::new(enrollment)),
            })
            .await
            .map_err(Into::into)
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn complete(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        enrollment_id: EnrollmentId,
        scope: &erpx_kernel::AuthorityScope,
        requested_version: Option<u64>,
        course_id: CourseId,
        course_version: u64,
        completion_id: CompletionId,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, LearningError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, LearningAction::Complete)
            .await?;
        let mut enrollment = self
            .write
            .load_enrollment(&authority, enrollment_id)
            .await?
            .ok_or(LearningError::Neutral)?;
        if enrollment.scope() != scope || requested_version != Some(enrollment.version()) {
            return Err(LearningError::Conflict);
        }
        let course = self
            .write
            .load_course(&authority, course_id)
            .await?
            .ok_or(LearningError::Neutral)?;
        if course.scope() != scope
            || course.version() != course_version
            || enrollment.course().course_id != course_id
            || enrollment.course().version != course_version
        {
            return Err(LearningError::Conflict);
        }
        let expected = enrollment.version();
        enrollment.complete(&course, completion_id, at)?;
        let fact = enrollment.completion_fact()?;
        // The minimal completion fact is committed atomically to the outbox.
        // A People-owned consumer publishes it idempotently after commit.
        self.write
            .commit(LearningCommit {
                identity,
                authority,
                expected_version: Some(expected),
                event_type: "learning.enrollment_completed".into(),
                event_payload: serde_json::to_value(&fact).map_err(|_| LearningError::Internal)?,
                aggregate: LearningAggregate::Enrollment(Box::new(enrollment)),
            })
            .await
            .map_err(Into::into)
    }

    pub async fn govern_record(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        record: crate::LearningRecord,
        expected_version: Option<u64>,
        event_type: &str,
    ) -> Result<CommitReceipt, LearningError> {
        identity.validate()?;
        record.validate()?;
        let action = match record.kind() {
            crate::LearningRecordKind::Assignment => LearningAction::Assign,
            crate::LearningRecordKind::Completion => LearningAction::Complete,
        };
        let authority = self
            .authorization
            .authorize(actor, record.scope(), action)
            .await?;
        let current = self.write.load_record(&authority, record.id()).await?;
        match (expected_version, current.as_ref()) {
            (None, None) if record.version() == 1 => {}
            (Some(expected), Some(current))
                if current.version() == expected
                    && current.kind() == record.kind()
                    && record.version()
                        == expected.checked_add(1).ok_or(LearningError::Conflict)? => {}
            _ => return Err(LearningError::Conflict),
        }
        self.write
            .commit(LearningCommit {
                identity,
                authority,
                expected_version,
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": record.version()}),
                aggregate: LearningAggregate::Record(Box::new(record)),
            })
            .await
            .map_err(Into::into)
    }
}

pub struct LearningQueryService<A, R> {
    authorization: A,
    read: R,
}
impl<A, R> LearningQueryService<A, R>
where
    A: AuthorizationPort,
    R: LearningReadRepository,
{
    pub const fn new(authorization: A, read: R) -> Self {
        Self {
            authorization,
            read,
        }
    }
    pub async fn get_enrollment(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::AuthorityScope,
        id: EnrollmentId,
    ) -> Result<crate::ports::EnrollmentProjection, LearningError> {
        let authority = self
            .authorization
            .authorize(actor, scope, LearningAction::View)
            .await?;
        self.read
            .get_enrollment(&authority, id)
            .await?
            .ok_or(LearningError::Neutral)
    }

    pub async fn get_record(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::AuthorityScope,
        id: uuid::Uuid,
    ) -> Result<crate::ports::LearningRecordProjection, LearningError> {
        let authority = self
            .authorization
            .authorize(actor, scope, LearningAction::View)
            .await?;
        self.read
            .get_record(&authority, id)
            .await?
            .ok_or(LearningError::Neutral)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LearningError {
    #[error("learning item unavailable")]
    Neutral,
    #[error("learning request conflict")]
    Conflict,
    #[error("invalid learning request: {0}")]
    Invalid(String),
    #[error("learning dependency unavailable")]
    Unavailable,
    #[error("learning operation failed")]
    Internal,
}
impl From<LearningDomainError> for LearningError {
    fn from(value: LearningDomainError) -> Self {
        Self::Invalid(value.to_string())
    }
}
impl From<PortError> for LearningError {
    fn from(value: PortError) -> Self {
        match value {
            PortError::Denied | PortError::NotFound => Self::Neutral,
            PortError::Conflict => Self::Conflict,
            PortError::Stale | PortError::Unavailable => Self::Unavailable,
            PortError::InvalidState => Self::Invalid("adapter rejected invalid state".into()),
            PortError::Internal => Self::Internal,
        }
    }
}

#[cfg(test)]
mod human_decision_order_tests {
    #[test]
    fn course_publication_rejects_human_proof_before_repository_access() {
        let source = include_str!("application.rs");
        let start = source.find("pub async fn publish_course").unwrap();
        let end = source[start..]
            .find("pub async fn retire_course")
            .map(|offset| start + offset)
            .unwrap();
        let publish = &source[start..end];
        assert!(
            publish.find("validate_publish_decision").unwrap()
                < publish.find(".load_course").unwrap()
        );
        assert!(
            publish.find("validate_publish_decision").unwrap() < publish.find(".commit").unwrap()
        );
    }
}
