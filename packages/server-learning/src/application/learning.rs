//! Learning use cases.
//!
//! Ports supply the owning serializable transaction, current course/People/IAM
//! facts, replay and committed readback. This layer never opens a transaction,
//! reads HTTP, checks a package purchase or writes another owner's rows.
use crate::domain::eligibility_facts::learning_benefit_continuation;
use crate::domain::learning::{
    AssignmentChange, AssignmentChangePlan, AssignmentCreatePlan, AssignmentRequest, AssignmentRow,
    CertificateRevocation, CertificateRevokePlan, CertificateRow, CourseChange, CourseChangePlan,
    CourseCreatePlan, CourseRequest, CourseRow, EventRow, HandoffRow, LearningActor,
    LearningEvidenceRequest, LearningReceipt, MAX_ASSIGNMENT_ROWS, MAX_COURSE_ROWS, MAX_EVENT_ROWS,
    MAX_PEOPLE_ROWS, PersonRow, PlacementRow, PreparedLearningEvidence, ReceiptSnapshot,
    assignment_change_identity, assignment_change_plan, assignment_create_id,
    assignment_create_identity, assignment_create_plan, assignment_evidence_request,
    assignment_not_found, certificate_not_found, certificate_revoke_plan, course_change_identity,
    course_change_plan, course_create_id, course_create_identity, course_create_plan,
    course_not_found, evidence_port_required, idempotency_conflict, learner_ineligible,
    readback_invalid, revocation_identity, source_changed, source_invalid, state_changed,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

pub type LearningFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Current Files/Records business-evidence port bound to the same serializable
/// owner unit. The exact Files binding implementation is assembled by
/// composition; this layer only states the Learning target/policy context.
pub trait LearningEvidencePort: Send {
    fn prepare<'a>(
        &'a mut self,
        connection: &'a mut sqlx::PgConnection,
        request: LearningEvidenceRequest,
    ) -> LearningFuture<'a, Option<PreparedLearningEvidence>, BusinessError>;

    fn require_current<'a>(
        &'a mut self,
        connection: &'a mut sqlx::PgConnection,
        prepared: &'a PreparedLearningEvidence,
    ) -> LearningFuture<'a, (), BusinessError>;
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn receipt_value(
    actor: &LearningActor,
    id: &str,
    operation: &str,
    request_hash: &str,
    receipt: Option<ReceiptSnapshot>,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash
        || receipt.operation != operation
        || receipt.actor_id != actor.user_id.as_str()
        || receipt.record_id != id
    {
        return Err(idempotency_conflict());
    }
    let authority: Value =
        serde_json::from_str(&receipt.authority_json).map_err(|_| readback_invalid())?;
    if authority["companyId"].as_str() != Some(actor.company_id.as_str())
        || authority["assignmentId"].as_str() != Some(actor.assignment_id.as_str())
    {
        return Err(idempotency_conflict());
    }
    let parsed: LearningReceipt =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed.id != id
        || parsed.version < 1
        || !matches!(
            parsed.status.as_str(),
            "draft"
                | "published"
                | "rejected"
                | "retired"
                | "assigned"
                | "in_progress"
                | "completed"
                | "waived"
                | "cancelled"
                | "revoked"
        )
    {
        return Err(readback_invalid());
    }
    Ok(Some(parsed.to_wire()))
}

fn validate_courses(
    actor: &LearningActor,
    rows: Vec<CourseRow>,
) -> Result<Vec<CourseRow>, BusinessError> {
    if rows.len() > MAX_COURSE_ROWS {
        return Err(source_invalid());
    }
    let mut seen = BTreeSet::new();
    for row in &rows {
        row.validate_source()?;
        if row.company_id != actor.company_id.as_str() || !seen.insert(row.id.clone()) {
            return Err(source_invalid());
        }
    }
    Ok(rows)
}

fn validate_assignments(
    actor: &LearningActor,
    rows: Vec<AssignmentRow>,
    personal: bool,
) -> Result<Vec<AssignmentRow>, BusinessError> {
    if rows.len() > MAX_ASSIGNMENT_ROWS {
        return Err(source_invalid());
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        row.validate_source()?;
        if !actor.in_scope(
            &row.company_id,
            Some(&row.branch_id),
            row.department_id.as_deref(),
        ) {
            continue;
        }
        if personal && row.learner_id != actor.user_id.as_str() {
            continue;
        }
        if !seen.insert(row.id.clone()) {
            return Err(source_invalid());
        }
        out.push(row);
    }
    Ok(out)
}

fn validate_people(
    actor: &LearningActor,
    rows: Vec<PersonRow>,
) -> Result<Vec<PersonRow>, BusinessError> {
    if rows.len() > MAX_PEOPLE_ROWS {
        return Err(source_invalid());
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        row.validate_source()?;
        if !actor.in_scope(
            actor.company_id.as_str(),
            row.branch_id.as_deref(),
            row.department_id.as_deref(),
        ) || !seen.insert(row.id.clone())
        {
            continue;
        }
        out.push(row);
    }
    Ok(out)
}

fn validate_events(
    actor: &LearningActor,
    assignment: &AssignmentRow,
    rows: Vec<EventRow>,
) -> Result<Vec<EventRow>, BusinessError> {
    if rows.len() > MAX_EVENT_ROWS {
        return Err(source_invalid());
    }
    let mut previous = 0;
    for row in &rows {
        row.validate_source()?;
        if row.assignment_id != assignment.id
            || row.version <= previous
            || row.version > assignment.version
        {
            return Err(source_invalid());
        }
        previous = row.version;
    }
    if actor.user_id.as_str().is_empty() {
        return Err(source_invalid());
    }
    Ok(rows)
}

fn validate_certificate_read(
    assignment: &AssignmentRow,
    handoff: &Option<HandoffRow>,
    certificate: &Option<CertificateRow>,
) -> Result<(), BusinessError> {
    if let Some(handoff) = handoff {
        handoff.validate_source()?;
        if handoff.assignment_id != assignment.id
            || handoff.company_id != assignment.company_id
            || handoff.source_assignment_version > assignment.version
        {
            return Err(source_invalid());
        }
    }
    if let Some(certificate) = certificate {
        certificate.validate_source()?;
        if certificate.assignment_id != assignment.id
            || certificate.company_id != assignment.company_id
            || certificate.source_assignment_version > assignment.version
        {
            return Err(source_invalid());
        }
        if handoff
            .as_ref()
            .is_none_or(|row| row.id != certificate.handoff_id)
        {
            return Err(source_invalid());
        }
    }
    Ok(())
}

struct WorkspaceRows {
    courses: Vec<CourseRow>,
    assignments: Vec<AssignmentRow>,
    people: Vec<PersonRow>,
}

fn workspace_json(actor: &LearningActor, rows: WorkspaceRows) -> Value {
    Value::Object(
        [
            (
                "courses".to_owned(),
                Value::Array(rows.courses.iter().map(CourseRow::to_json).collect()),
            ),
            (
                "assignments".to_owned(),
                Value::Array(
                    rows.assignments
                        .iter()
                        .map(AssignmentRow::to_json)
                        .collect(),
                ),
            ),
            (
                "people".to_owned(),
                Value::Array(rows.people.iter().map(PersonRow::to_json).collect()),
            ),
            ("canWrite".to_owned(), Value::Bool(actor.can_write())),
            ("canReview".to_owned(), Value::Bool(actor.can_review())),
            (
                "actorId".to_owned(),
                Value::String(actor.user_id.as_str().to_owned()),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

pub trait LearningCommandRepository {
    type Error: From<BusinessError>;

    fn digest<'a>(&'a mut self, value: &'a str) -> LearningFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> LearningFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn course_for_update<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<CourseRow>, Self::Error>;
    fn course_source<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<CourseRow>, Self::Error>;
    fn placement<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        learner_id: &'a str,
    ) -> LearningFuture<'a, Option<PlacementRow>, Self::Error>;
    fn assignment_for_update<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<AssignmentRow>, Self::Error>;
    fn certificate_for_update<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<CertificateRow>, Self::Error>;
    fn prepare_evidence<'a>(
        &'a mut self,
        request: LearningEvidenceRequest,
    ) -> LearningFuture<'a, Option<PreparedLearningEvidence>, Self::Error>;
    fn require_evidence_current<'a>(
        &'a mut self,
        prepared: &'a PreparedLearningEvidence,
    ) -> LearningFuture<'a, (), Self::Error>;
    fn commit_course_create<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a CourseCreatePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error>;
    fn commit_course_change<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a CourseChangePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error>;
    fn commit_assignment_create<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a AssignmentCreatePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error>;
    fn commit_assignment_change<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a AssignmentChangePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error>;
    fn commit_certificate_revoke<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a CertificateRevokePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error>;
}

pub trait LearningReadRepository {
    type Error: From<BusinessError>;

    fn courses<'a>(
        &'a mut self,
        actor: &'a LearningActor,
    ) -> LearningFuture<'a, Vec<CourseRow>, Self::Error>;
    fn assignments<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        personal: bool,
    ) -> LearningFuture<'a, Vec<AssignmentRow>, Self::Error>;
    fn people<'a>(
        &'a mut self,
        actor: &'a LearningActor,
    ) -> LearningFuture<'a, Vec<PersonRow>, Self::Error>;
    fn assignment<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<AssignmentRow>, Self::Error>;
    fn events<'a>(&'a mut self, id: &'a str) -> LearningFuture<'a, Vec<EventRow>, Self::Error>;
    fn handoff<'a>(
        &'a mut self,
        id: &'a str,
    ) -> LearningFuture<'a, Option<HandoffRow>, Self::Error>;
    fn certificate<'a>(
        &'a mut self,
        id: &'a str,
    ) -> LearningFuture<'a, Option<CertificateRow>, Self::Error>;
}

pub async fn create_course<R: LearningCommandRepository>(
    repository: &mut R,
    actor: &LearningActor,
    key: &CommandKey,
    input: &CourseRequest,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    let id = course_create_id(key.as_str())?;
    let identity = course_create_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        &id,
        crate::domain::learning::COURSE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let plan = course_create_plan(actor, key.as_str(), &input, now)?;
    repository
        .commit_course_create(actor, key.as_str(), &plan, &hash)
        .await
}

pub async fn change_course<R: LearningCommandRepository>(
    repository: &mut R,
    actor: &LearningActor,
    key: &CommandKey,
    id: &str,
    input: &CourseChange,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    actor.require_review(now)?;
    let input = input.validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(course_not_found().into());
    }
    let identity = course_change_identity(actor, key.as_str(), id, &input);
    let operation = input.action.operation();
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        id,
        &operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .course_for_update(actor, id)
        .await?
        .ok_or_else(course_not_found)?;
    let plan = course_change_plan(actor, &row, &input, key.as_str(), now)?;
    repository
        .commit_course_change(actor, key.as_str(), &plan, &hash)
        .await
}

pub async fn create_assignment<R: LearningCommandRepository>(
    repository: &mut R,
    actor: &LearningActor,
    key: &CommandKey,
    input: &AssignmentRequest,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    actor.require_review(now)?;
    let input = input.validate()?;
    let id = assignment_create_id(key.as_str())?;
    let identity = assignment_create_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        &id,
        "learning.assignment.create",
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let course = repository
        .course_source(actor, &input.course_id)
        .await?
        .ok_or_else(source_changed)?;
    let placement = repository
        .placement(actor, &input.learner_id)
        .await?
        .ok_or_else(learner_ineligible)?;
    if !placement.eligible {
        return Err(learner_ineligible().into());
    }
    let plan = assignment_create_plan(actor, key.as_str(), &input, &course, &placement, now)?;
    repository
        .commit_assignment_create(actor, key.as_str(), &plan, &hash)
        .await
}

pub async fn change_assignment<R: LearningCommandRepository>(
    repository: &mut R,
    actor: &LearningActor,
    key: &CommandKey,
    id: &str,
    input: &AssignmentChange,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    let input = input.validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(assignment_not_found().into());
    }
    let identity = assignment_change_identity(actor, key.as_str(), id, &input);
    let operation = input.action.operation();
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if input.action.is_reviewer_action() {
        actor.require_review(now)?;
    }
    if let Some(prior) = receipt_value(
        actor,
        id,
        &operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let evidence_request = assignment_evidence_request(id, &input, key.as_str())?;
    let evidence = match evidence_request {
        Some(request) => Some(
            repository
                .prepare_evidence(request)
                .await?
                .ok_or_else(evidence_port_required)?,
        ),
        None => None,
    };
    let row = repository
        .assignment_for_update(actor, id)
        .await?
        .ok_or_else(assignment_not_found)?;
    if !input.action.allowed_from(&row.status) {
        return Err(state_changed().into());
    }
    if matches!(
        input.action,
        crate::domain::learning::AssignmentAction::Start
            | crate::domain::learning::AssignmentAction::Complete
    ) && (row.learner_id != actor.user_id.as_str() || !row.learner_eligible)
    {
        return Err(crate::domain::learning::learner_required().into());
    }
    let plan = assignment_change_plan(actor, &row, &input, evidence, key.as_str(), now)?;
    let result = repository
        .commit_assignment_change(actor, key.as_str(), &plan, &hash)
        .await?;
    if let Some(prepared) = &plan.evidence {
        repository.require_evidence_current(prepared).await?;
    }
    Ok(result)
}

pub async fn revoke_certificate<R: LearningCommandRepository>(
    repository: &mut R,
    actor: &LearningActor,
    key: &CommandKey,
    id: &str,
    input: &CertificateRevocation,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_write(now)?;
    actor.require_review(now)?;
    let input = input.validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(certificate_not_found().into());
    }
    let identity = revocation_identity(actor, key.as_str(), id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        id,
        "learning.certificate.revoke",
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .certificate_for_update(actor, id)
        .await?
        .ok_or_else(certificate_not_found)?;
    let plan = certificate_revoke_plan(actor, &row, &input, key.as_str(), now)?;
    repository
        .commit_certificate_revoke(actor, key.as_str(), &plan, &hash)
        .await
}

async fn load_workspace<R: LearningReadRepository>(
    repository: &mut R,
    actor: &LearningActor,
    personal: bool,
    now: i64,
) -> Result<WorkspaceRows, R::Error> {
    actor.require_read(now)?;
    let courses = validate_courses(actor, repository.courses(actor).await?)?;
    let assignments = validate_assignments(
        actor,
        repository.assignments(actor, personal).await?,
        personal,
    )?;
    let people = if actor.can_review() {
        validate_people(actor, repository.people(actor).await?)?
    } else {
        Vec::new()
    };
    Ok(WorkspaceRows {
        courses,
        assignments,
        people,
    })
}

pub async fn learning_workspace<R: LearningReadRepository>(
    repository: &mut R,
    actor: &LearningActor,
    personal: bool,
    now: i64,
) -> Result<Value, R::Error> {
    Ok(workspace_json(
        actor,
        load_workspace(repository, actor, personal, now).await?,
    ))
}

pub async fn learning_detail<R: LearningReadRepository>(
    repository: &mut R,
    actor: &LearningActor,
    id: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(assignment_not_found().into());
    }
    let rows = load_workspace(repository, actor, false, now).await?;
    let assignment = rows
        .assignments
        .iter()
        .find(|row| row.id == id)
        .cloned()
        .ok_or_else(assignment_not_found)?;
    let events = validate_events(actor, &assignment, repository.events(id).await?)?;
    let handoff = repository.handoff(id).await?;
    let certificate = repository.certificate(id).await?;
    validate_certificate_read(&assignment, &handoff, &certificate)?;
    let workspace = workspace_json(actor, rows);
    let mut object = workspace.as_object().cloned().ok_or_else(source_invalid)?;
    object.insert("assignment".to_owned(), assignment.to_json());
    object.insert(
        "history".to_owned(),
        Value::Array(events.iter().map(EventRow::to_json).collect()),
    );
    object.insert(
        "handoff".to_owned(),
        handoff.as_ref().map_or(Value::Null, HandoffRow::to_json),
    );
    object.insert(
        "certificate".to_owned(),
        certificate
            .as_ref()
            .map_or(Value::Null, CertificateRow::to_json),
    );
    object.insert(
        "benefitContinuation".to_owned(),
        learning_benefit_continuation(
            actor.user_id.as_str(),
            actor.company_id.as_str(),
            &actor.modules,
            &actor.permissions,
            None,
            &assignment,
            handoff.as_ref(),
            certificate.as_ref(),
        )
        .unwrap_or(Value::Null),
    );
    Ok(Value::Object(object))
}
