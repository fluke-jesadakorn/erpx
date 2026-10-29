//! Agreement review composed from the owner and current IAM, People and purchase witnesses.
use super::{
    Error, FieldServicePartsSourcePreparation, changed,
    purchases::FieldServicePurchases,
    request::{actor, database_now},
};
use erpx_server_field_service::public::{
    agreement_review::*,
    parts::{FieldServiceRawOrderTarget, field_service_first_wire_message},
};
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    field_service_assignment::{
        CurrentFieldServiceRawAssignment, FieldServiceAssignmentPermission,
        FieldServiceRawAssignmentRequest, inspect_field_service_raw_assignment,
    },
    release_reviewers::PgSamePersonReader,
};
use erpx_server_people_payroll::public::field_service_worker::{
    CurrentFieldServicePeopleReference, inspect_field_service_people_reference,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, IdentifierSource, RecordId, UserId,
};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
use std::sync::Arc;

async fn source(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
) -> Result<CurrentAgreementReview, Error> {
    read_agreement_review(c, current, id).await?.ok_or_else(|| {
        BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "FIELD_SERVICE_AGREEMENT_NOT_FOUND",
            "Agreement not found.",
        )
        .into()
    })
}
struct Reviewer {
    assignment: CurrentFieldServiceRawAssignment,
    person: CurrentFieldServicePeopleReference,
    creator: UserId,
}
impl Reviewer {
    async fn inspect(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        source: &CurrentAgreementReview,
    ) -> Result<Self, Error> {
        current.require_permission("records.review")?;
        let f = source.facts();
        let now = database_now(c).await?;
        let assignment = inspect_field_service_raw_assignment(
            c,
            FieldServiceRawAssignmentRequest {
                company_id: f.company_id.clone(),
                user_id: current.user_id().as_str().into(),
                branch_id: f.branch_id.clone(),
                department_id: f.department_id.clone(),
                permission: FieldServiceAssignmentPermission::Review,
            },
            now,
        )
        .await?;
        let person =
            inspect_field_service_people_reference(c, &f.company_id, current.user_id().as_str())
                .await?;
        let reviewer = Self {
            assignment,
            person,
            creator: UserId::new(&f.creator_id)?,
        };
        reviewer.require_current(c, current, source).await?;
        Ok(reviewer)
    }
    async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        source: &CurrentAgreementReview,
    ) -> Result<(), Error> {
        current.require_permission("records.review")?;
        let now = database_now(c).await?;
        self.assignment.require_current(c, now).await?;
        self.person.require_current(c).await?;
        if !self.assignment.is_eligible()
            || !self.person.is_eligible()
            || PgSamePersonReader::new(c)
                .same_person(&source.facts().company_id, &self.creator, current.user_id())
                .await?
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Forbidden,
                "FIELD_SERVICE_REVIEW_REQUIRED",
                "An independent eligible reviewer is required.",
            )
            .into());
        }
        Ok(())
    }
}
struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    expected: &'a WorkflowRequest,
    clock: &'a C,
    purchase: Option<FieldServicePurchases>,
    action: AgreementReviewAction,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FieldServicePartsSourcePreparation.prepare(c).await
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.expected.key
            || r.operation != self.expected.operation
            || r.record_id != self.expected.record_id
            || r.input.as_str() != self.expected.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(actor(self.current, self.clock.now())?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        r: &WorkflowRequest,
        d: CommandDisposition,
    ) -> Result<(), Error> {
        if &actor(self.current, self.clock.now())? != a {
            return Err(changed().into());
        }
        let source = source(c, self.current, &r.record_id).await?;
        if let Some(p) = &self.purchase {
            p.require_current(c, self.current, self.clock.now()).await?
        } else {
            self.purchase = Some(
                FieldServicePurchases::retain_agreement_review(
                    c,
                    self.current,
                    source.facts().has_asset,
                    d,
                    self.action,
                    self.clock.now(),
                )
                .await?,
            )
        }
        Ok(())
    }
}
struct Prepared {
    source: CurrentAgreementReview,
    reviewer: Reviewer,
    plan: PreparedAgreementReview,
    purchase: FieldServicePurchases,
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    input: &'a NormalizedAgreementReview,
    key: &'a CommandKey,
    target: &'a str,
    logical: &'a WorkflowRecordIdentity,
    clock: &'a C,
    prepared: Option<Prepared>,
    written: Option<Prepared>,
}
impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FieldServicePartsSourcePreparation.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let source = source(c, self.current, self.target).await?;
        let reviewer = Reviewer::inspect(c, self.current, &source).await?;
        let purchase = FieldServicePurchases::retain_agreement_review(
            c,
            self.current,
            source.facts().has_asset,
            CommandDisposition::Execute,
            self.input.action,
            self.clock.now(),
        )
        .await?;
        let plan = PreparedAgreementReview::prepare(
            c,
            self.current,
            &source,
            self.input.clone(),
            self.key.clone(),
            self.clock.now().get(),
        )
        .await?;
        plan.require_before(c, self.current, &source).await?;
        let snapshot =
            erpx_server_field_service::public::reservation_read::FieldServiceReserveResult {
                id: self.target.into(),
                status: plan.status().into(),
                version: plan.version(),
            };
        let evidence = crate::field_service_reservation_read::FieldServiceAgreementReviewReadResponse {
            context: crate::field_service_reservation_read::context(self.current)?,
            agreement: snapshot.clone(), receipt: snapshot,
            event: erpx_server_field_service::public::reservation_read::FieldServiceAgreementReviewEvent {
                version: plan.version(), action: self.input.action.operation().strip_prefix("field.agreement.").ok_or_else(changed)?.into(),
                actor_id: self.current.user_id().as_str().into(), reason: self.input.reason.clone(), status: plan.status().into(),
            },
        };
        serde_json::to_writer(
            &mut crate::field_service_reservation_read::ReplyBudget(0),
            &evidence,
        )
        .map_err(|_| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "FIELD_SERVICE_READ_LIMIT",
                "The Service source exceeds this bounded read.",
            )
        })?;
        let result = CommandJson::parse(&format!(
            "{{\"id\":{},\"version\":{},\"status\":{}}}",
            self.logical.logical_json()?.as_str(),
            plan.version(),
            serde_json::to_string(plan.status()).map_err(|_| changed())?
        ))?;
        let version = plan.version();
        self.prepared = Some(Prepared {
            source,
            reviewer,
            plan,
            purchase,
        });
        Ok(WorkflowOutput {
            version,
            result,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let p = self.prepared.take().ok_or_else(changed)?;
        p.reviewer
            .require_current(c, self.current, &p.source)
            .await?;
        p.plan.apply(c, self.current, &p.source).await?;
        self.written = Some(p);
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        let p = self.written.as_ref().ok_or_else(changed)?;
        p.reviewer
            .require_current(c, self.current, &p.source)
            .await?;
        p.plan.require_after(c, self.current).await?;
        p.purchase
            .require_current(c, self.current, self.clock.now())
            .await
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
struct Replay;
impl PgOwnerCommandEffects for Replay {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FieldServicePartsSourcePreparation.prepare(c).await
    }
    async fn prepare(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        Err(changed().into())
    }
    async fn apply(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Err(changed().into())
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Err(changed().into())
    }
}
pub struct AgreementReviewDispatcher<C> {
    pub clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> super::OtherPost for AgreementReviewDispatcher<C> {
    fn validate(&self, input: &super::OtherPostInput) -> Result<(), String> {
        if input.branch != crate::field_service_http_wire::OtherPostBranch::AgreementChange {
            return Err("This action is not available.".into());
        }
        normalize_agreement_review(&input.body)
            .map(|_| ())
            .map_err(|e| {
                field_service_first_wire_message(&input.body, &e)
                    .unwrap_or_else(|| "Invalid request.".into())
            })
    }
    fn admission_error(
        &self,
        _: crate::field_service_http_wire::OtherPostBranch,
        e: BusinessError,
    ) -> BusinessError {
        e
    }
    fn execute<'a>(
        &'a self,
        c: &'a mut PgConnection,
        current: &'a AuthorizedWorkContext,
        input: super::OtherPostInput,
    ) -> PgWorkFuture<'a, CommandJson, BusinessError> {
        Box::pin(async move {
            let normalized = normalize_agreement_review(&input.body).map_err(|_| {
                BusinessError::invalid("FIELD_SERVICE_REVIEW_INVALID", "Invalid Agreement action.")
            })?;
            let raw = FieldServiceRawOrderTarget::from_utf16(input.id_utf16.ok_or_else(changed)?);
            let logical = WorkflowRecordIdentity::from_utf16(raw.original_utf16().to_vec());
            let request = WorkflowRequest {
                key: input.key.clone(),
                operation: normalized.action.operation().into(),
                record_id: logical.physical_text().into(),
                input: normalized.receipt_input.clone(),
            };
            let ordered = SourceOrderedWorkflowRequest {
                key: request.key.clone(),
                operation: request.operation.clone(),
                record_id: logical.clone(),
                input: request.input.clone(),
            };
            let mut authority = Authority {
                current,
                expected: &request,
                clock: self.clock.as_ref(),
                purchase: None,
                action: normalized.action,
            };
            let actor = actor(current, self.clock.now())?;
            let candidate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation=$2 AND record_id=$3 AND actor_id=$4 AND authority_json::jsonb->>'companyId'=$5)").bind(&request.key).bind(&request.operation).bind(&request.record_id).bind(current.user_id().as_str()).bind(current.selection().workspace().company_id.as_str()).fetch_one(&mut *c).await?;
            authority
                .verify_command(
                    c,
                    &actor,
                    &request,
                    if candidate {
                        CommandDisposition::Replay
                    } else {
                        CommandDisposition::Execute
                    },
                )
                .await?;
            let mut ids = Ids;
            let mut replay = Replay;
            if let Some(outcome) = PgCommandLedger::new(
                current.selection().workspace().company_id.clone(),
                c,
                &mut authority,
                self.clock.as_ref(),
                &mut ids,
            )
            .replay_source_ordered(&ordered, &mut replay)
            .await?
            {
                return Ok(outcome.result);
            }
            if candidate {
                authority.purchase = None;
                authority
                    .verify_command(c, &actor, &request, CommandDisposition::Execute)
                    .await?;
            }
            let key = CommandKey::new(input.key)?;
            let target = raw.storage_text();
            let mut effects = Effects {
                current,
                input: &normalized,
                key: &key,
                target: &target,
                logical: &logical,
                clock: self.clock.as_ref(),
                prepared: None,
                written: None,
            };
            Ok(PgCommandLedger::new(
                current.selection().workspace().company_id.clone(),
                c,
                &mut authority,
                self.clock.as_ref(),
                &mut ids,
            )
            .execute_source_ordered(&ordered, &mut effects)
            .await?
            .result)
        })
    }
}
