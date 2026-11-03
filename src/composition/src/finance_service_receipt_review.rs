//! Native Field receipt approval/release, with actual IAM same-Person controls.
use crate::{
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
    finance_journal_review::{command_actor, finance_actor},
    finance_service_invoice_post::service_receipt_review_authority,
    finance_service_receipt_create::ReceiptSources,
};
use erpx_server_finance::public::service_receipt::*;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, UserId};
use erpx_server_workflow::public::{
    commands::*,
    finance_receipt_decision::*,
    record_origin::{
        PgRawRecordOrigin, read_finance_receipt_creation_origin, verify_raw_record_origin,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReceiptDecisionInput {
    pub expected_version: i64,
    pub proof: String,
}
/// Validated REST decision with the logical UTF-16 payload retained for replay.
/// Construction is restricted to the raw parser so effects cannot diverge from
/// the command fingerprint through an independently supplied typed DTO.
#[derive(Clone)]
pub struct ServiceReceiptDecisionWire {
    input: ServiceReceiptDecisionInput,
    normalized: CommandJson,
}
impl ServiceReceiptDecisionWire {
    pub fn input(&self) -> &ServiceReceiptDecisionInput {
        &self.input
    }
    pub fn normalized(&self) -> &CommandJson {
        &self.normalized
    }
}
pub fn normalize_service_receipt_decision(
    body: &[u8],
    approve: bool,
) -> Result<ServiceReceiptDecisionWire, BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "VALIDATION_FAILED",
            if approve {
                "Payment approval evidence is required."
            } else {
                "Payment release evidence is required."
            },
        )
    };
    let body = String::from_utf8_lossy(body);
    let body = body.strip_prefix('\u{feff}').unwrap_or(&body);
    let doc = CommandJson::parse(body)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    if doc.object_keys()?.is_none() {
        return Err(invalid());
    }
    let version = doc.property("expectedVersion")?.ok_or_else(invalid)?;
    let number: serde_json::Value =
        serde_json::from_str(version.as_str()).map_err(|_| invalid())?;
    let number = number
        .as_f64()
        .filter(|n| {
            n.is_finite() && n.fract() == 0.0 && (1.0..=9_007_199_254_740_991.0).contains(n)
        })
        .ok_or_else(invalid)? as i64;
    let proof = doc.property("proof")?.ok_or_else(invalid)?;
    let physical = proof.string_utf8_storage()?.ok_or_else(invalid)?;
    // The original REST schema does not trim proof; nonblank decision policy
    // remains inside the authorized command, after exact replay selection.
    let normalized = CommandJson::parse(&format!(
        "{{\"expectedVersion\":{number},\"proof\":{}}}",
        proof.as_str()
    ))?;
    Ok(ServiceReceiptDecisionWire {
        input: ServiceReceiptDecisionInput {
            expected_version: number,
            proof: physical,
        },
        normalized,
    })
}
#[derive(Clone)]
struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            ReceiptSources.prepare(c).await?;
            lock_finance_receipt_decision_evidence(c).await
        })
    }
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original Service invoice changed. Refresh its source workflow.",
    )
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: ServiceReceiptDecisionInput,
    approve: bool,
    source: Option<CurrentServiceReceiptReview>,
    retained: Option<RetainedServiceReceiptReview>,
    decision: Option<PlannedFinanceReceiptDecision>,
    same_creator: bool,
    same_approver: bool,
}
impl<C: Clock + Send + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Sources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        require_service_receipt_evidence(&self.input.proof, !self.approve)?;
        let source =
            read_service_receipt_review(c, &finance_actor(self.current), &r.record_id).await?;
        let row = source.row();
        let same = |key| -> Result<bool, BusinessError> {
            match row[key].as_str() {
                Some(id) => Ok(self.current.same_person_user(&UserId::new(id)?)),
                None => Ok(false),
            }
        };
        self.same_creator = same("created_by")?;
        self.same_approver = same("approved_by")?;
        let f = ServiceReceiptReviewFacts {
            status: row["status"].as_str().ok_or_else(changed)?,
            version: row["version"].as_i64().ok_or_else(changed)?,
            has_bank_account: row["bank_account"].as_str().is_some_and(|v| !v.is_empty()),
            same_creator: self.same_creator,
            same_approver: self.same_approver,
            same_releaser: false,
        };
        if self.approve {
            validate_service_receipt_approval(&f, self.input.expected_version)?;
        } else {
            validate_service_receipt_release(&f, self.input.expected_version)?;
        }
        self.decision = Some(
            PlannedFinanceReceiptDecision::prepare(
                c,
                actor,
                FinanceReceiptDecisionFacts {
                    id: uuid::Uuid::new_v4().to_string(),
                    command_id: r.key.clone(),
                    record_id: r.record_id.clone(),
                    action: r.operation.clone(),
                    reviewed_version: self.input.expected_version,
                    input_digest: hex::encode(Sha256::digest(r.input.as_str().as_bytes())),
                    created_at: self.clock.now().get(),
                },
            )
            .await?,
        );
        let version = self
            .input
            .expected_version
            .checked_add(1)
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(changed)?;
        let result = if self.approve {
            serde_json::json!({"id":r.record_id,"status":"approved","version":version})
        } else {
            serde_json::json!({"id":r.record_id,"status":"released","version":version,"bankEvidenceRequired":true})
        };
        self.source = Some(source);
        Ok(WorkflowOutput {
            version,
            result: CommandJson::from_serializable(&result)?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.retained = Some(
            self.source
                .take()
                .ok_or_else(changed)?
                .apply(
                    c,
                    &finance_actor(self.current),
                    self.approve,
                    self.input.expected_version,
                    self.same_creator,
                    self.same_approver,
                    self.clock.now().get(),
                )
                .await?,
        );
        self.decision.as_ref().ok_or_else(changed)?.apply(c).await
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.retained
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        self.decision
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await
    }
}
pub async fn execute_service_receipt_review<C, F, P>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    selection: F,
    config: FinanceJournalCreateConfiguration,
    clock: Arc<C>,
    approve: bool,
    prepare: P,
) -> Result<CommandJson, Error>
where
    C: Clock + Send + Sync + 'static,
    F: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    P: Fn() -> Result<(String, String, ServiceReceiptDecisionWire), BusinessError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let company = config.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut s = selection()?;
                if s.company_id != company {
                    return Err(changed());
                }
                s.mutation = true;
                Ok(s)
            },
            WorkAuthorizationRequest {
                permission: if approve {
                    "finance.write"
                } else {
                    "finance.pay"
                }
                .into(),
                module: Some("finance".into()),
                destination: "finance/payments".into(),
                api: true,
            },
            Sources,
            move |c, current| {
                let config = config.clone();
                let clock = clock.clone();
                let prepare = prepare.clone();
                Box::pin(async move {
                    let (key, id, wire) = prepare()?;
                    let input = wire.input;
                    validate_key(&key)?;
                    let payment =
                        read_service_receipt_review(c, &finance_actor(current), &id).await?;
                    let creator = payment.row()["created_by"].as_str().ok_or_else(changed)?;
                    let (origin, result): (PgRawRecordOrigin, CommandJson) =
                        read_finance_receipt_creation_origin(c, &config.company, &id, creator)
                            .await?
                            .ok_or_else(changed)?;
                    let result: serde_json::Value =
                        serde_json::from_str(result.as_str()).map_err(|_| changed())?;
                    if result["id"].as_str() != Some(id.as_str())
                        || result["status"] != "draft"
                        || result["version"] != 1
                    {
                        return Err(changed().into());
                    }
                    let invoice = result["settlementInvoiceId"]
                        .as_str()
                        .ok_or_else(changed)?
                        .to_owned();
                    let mut auth = service_receipt_review_authority(
                        current,
                        &config,
                        clock.as_ref(),
                        invoice,
                        id.clone(),
                        approve,
                    );
                    let request = WorkflowRequest {
                        key,
                        operation: if approve {
                            "finance.payment.approve"
                        } else {
                            "finance.payment.release"
                        }
                        .into(),
                        record_id: id,
                        input: wire.normalized,
                    };
                    let mut effects = Effects {
                        current,
                        clock: clock.as_ref(),
                        input,
                        approve,
                        source: None,
                        retained: None,
                        decision: None,
                        same_creator: false,
                        same_approver: false,
                    };
                    let mut ids = Ids;
                    let outcome = PgCommandLedger::new(
                        config.company.clone(),
                        c,
                        &mut auth,
                        clock.as_ref(),
                        &mut ids,
                    )
                    .execute(&request, &mut effects)
                    .await?;
                    if !outcome.replayed {
                        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                            .execute(&mut *c)
                            .await.map_err(|error| {
                                // A deferred effect cannot rewrite the retained
                                // native payment image. Preserve its original
                                // public source-denial code; the database fence
                                // and every unrelated persistence error remain.
                                if error.as_database_error().is_some_and(|db|db.code().as_deref()==Some("23514")&&db.constraint()==Some("native_service_payment_write_required")) {
                                    PgTransactionError::Business(changed())
                                }else{PgTransactionError::from(error)}
                            })?;
                        effects
                            .retain(c, &command_actor(current)?, &request)
                            .await?;
                        auth.verify_command(
                            c,
                            &command_actor(current)?,
                            &request,
                            CommandDisposition::Execute,
                        )
                        .await.map_err(|error|match error {
                            PgTransactionError::Business(e) if matches!(e.code,"WORK_ROLE_UNAVAILABLE"|"QUEUED_AUTHORITY_CHANGED") => BusinessError::new(FailureKind::Forbidden,"FINANCE_ACCESS_CHANGED","The current Finance authority changed before the receipt decision committed.").into(),
                            other=>other,
                        })?;
                    }
                    verify_raw_record_origin(c, &origin).await?;
                    Ok(outcome.result)
                })
            },
        )
        .await
}
