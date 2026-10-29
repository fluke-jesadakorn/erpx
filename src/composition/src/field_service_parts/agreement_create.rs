//! Actual Agreement draft create composed across Service, Organization, Customer, Asset and Money.
use super::{
    Error, FieldServicePartsSourcePreparation, changed, purchases::FieldServicePurchases,
    request::actor,
};
use erpx_server_assets::public::field_service_reference::{
    CurrentAgreementLinkAsset, read_agreement_link_asset,
};
use erpx_server_field_service::public::{
    agreement_review::{
        NormalizedAgreementCreate, PreparedAgreementCreate, normalize_agreement_create,
    },
    parts::field_service_first_wire_message,
    reservation_read::{FieldServiceAgreementReviewEvent, FieldServiceReserveResult},
};
use erpx_server_finance::public::{
    field_service_agreement_money::{
        PreparedAgreementMoney, RetainedAgreementMoney, prepare_agreement_money,
        read_agreement_money,
    },
    field_service_customer::{CurrentAgreementCustomer, read_agreement_customer},
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_organization::public::field_service_agreement::{
    CurrentAgreementOrganization, read_agreement_organization,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
use std::sync::Arc;
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "FIELD_SERVICE_SOURCE_NOT_FOUND",
        "The Service source is not available in this workspace.",
    )
}
struct Sources {
    organization: CurrentAgreementOrganization,
    customer: CurrentAgreementCustomer,
    asset: Option<CurrentAgreementLinkAsset>,
}
impl Sources {
    async fn read(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        input: &NormalizedAgreementCreate,
        branch: &str,
        department: Option<&str>,
    ) -> Result<Self, Error> {
        let company = &current.selection().workspace().company_id;
        let organization =
            read_agreement_organization(c, company, branch, department.filter(|s| !s.is_empty()))
                .await?
                .ok_or_else(unavailable)?;
        let customer = read_agreement_customer(c, company, &input.customer_id, &input.currency)
            .await?
            .ok_or_else(unavailable)?;
        let asset = if let Some(id) = input.asset_id.as_deref().filter(|id| !id.is_empty()) {
            Some(
                read_agreement_link_asset(
                    c,
                    company,
                    id,
                    branch,
                    current
                        .selection()
                        .workspace()
                        .department_id
                        .as_ref()
                        .map(|d| d.as_str()),
                )
                .await?
                .ok_or_else(unavailable)?,
            )
        } else {
            None
        };
        Ok(Self {
            organization,
            customer,
            asset,
        })
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.organization.require_current(c).await?;
        self.customer.require_current(c).await?;
        if let Some(a) = &self.asset {
            a.require_current(c).await?
        }
        Ok(())
    }
}
struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    expected: &'a WorkflowRequest,
    clock: &'a C,
    purchase: Option<FieldServicePurchases>,
    asset: bool,
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
        if d == CommandDisposition::Replay {
            let source =
                erpx_server_field_service::public::reservation_read::read_agreement_create_by_key(
                    c,
                    self.current,
                    &r.key,
                )
                .await?;
            if source.agreement().id != r.record_id {
                return Err(changed().into());
            }
            let money = read_agreement_money(
                c,
                &self.current.selection().workspace().company_id,
                &r.record_id,
            )
            .await?
            .ok_or_else(unavailable)?;
            source.require_current(c, self.current).await?;
            money.require_current(c).await?;
        }
        if let Some(p) = &self.purchase {
            p.require_current(c, self.current, self.clock.now()).await?
        } else {
            self.purchase = Some(
                FieldServicePurchases::retain_agreement_create(
                    c,
                    self.current,
                    self.asset,
                    d,
                    self.clock.now(),
                )
                .await?,
            )
        }
        Ok(())
    }
}
struct Prepared {
    source: Sources,
    plan: PreparedAgreementCreate,
    money: PreparedAgreementMoney,
    purchase: FieldServicePurchases,
}
struct Written {
    source: Sources,
    plan: PreparedAgreementCreate,
    money: RetainedAgreementMoney,
    purchase: FieldServicePurchases,
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    input: &'a NormalizedAgreementCreate,
    key: &'a CommandKey,
    clock: &'a C,
    prepared: Option<Prepared>,
    written: Option<Written>,
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
        if self.input.end_date < self.input.start_date {
            return Err(BusinessError::invalid(
                "FIELD_SERVICE_DATE_RANGE",
                "Agreement end date must be on or after its start date.",
            )
            .into());
        }
        let scope = self.current.selection().workspace();
        let branch = scope
            .branch_id
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(&self.input.branch_id)
            .to_owned();
        let department = scope
            .department_id
            .as_ref()
            .map(|s| s.as_str().to_owned())
            .or_else(|| self.input.department_id.clone());
        let purchase = FieldServicePurchases::retain_agreement_create(
            c,
            self.current,
            self.input
                .asset_id
                .as_deref()
                .is_some_and(|s| !s.is_empty()),
            CommandDisposition::Execute,
            self.clock.now(),
        )
        .await?;
        let at = self.clock.now().get();
        let id = format!("service-agreement-{}", self.key.as_str());
        let money = prepare_agreement_money(
            c,
            &scope.company_id,
            &id,
            &self.input.start_date,
            &self.input.currency,
            self.input.visit_rate_minor,
            self.clock,
        )
        .await?;
        let source =
            Sources::read(c, self.current, self.input, &branch, department.as_deref()).await?;
        let plan = PreparedAgreementCreate::prepare(
            c,
            scope.company_id.clone(),
            (branch, department),
            self.current.user_id().as_str().into(),
            self.key.clone(),
            self.input.clone(),
            at,
        )
        .await?;
        let result = FieldServiceReserveResult {
            id: plan.id().into(),
            status: "draft".into(),
            version: 1,
        };
        let evidence =
            crate::field_service_reservation_read::FieldServiceAgreementCreateReadResponse {
                context: crate::field_service_reservation_read::context(self.current)?,
                receipt: result.clone(),
                agreement: result.clone(),
                event: FieldServiceAgreementReviewEvent {
                    version: 1,
                    action: "create".into(),
                    actor_id: self.current.user_id().as_str().into(),
                    reason: "Agreement drafted for independent review".into(),
                    status: "draft".into(),
                },
                money: money.evidence().clone(),
            };
        serde_json::to_writer(
            &mut crate::field_service_reservation_read::ReplyBudget(0),
            &evidence,
        )
        .map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "FIELD_SERVICE_READ_LIMIT",
                "The Service source exceeds this bounded read.",
            )
        })?;
        let output = CommandJson::from_serializable(&result)?;
        self.prepared = Some(Prepared {
            source,
            plan,
            money,
            purchase,
        });
        Ok(WorkflowOutput {
            version: 1,
            result: output,
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
        p.source.require_current(c).await?;
        p.plan.require_before(c).await?;
        let money = p.money.persist(c).await?;
        p.plan.apply(c).await?;
        self.written = Some(Written {
            source: p.source,
            plan: p.plan,
            money,
            purchase: p.purchase,
        });
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
        p.source.require_current(c).await?;
        p.plan.require_after(c).await?;
        p.money.require_current(c).await?;
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
pub struct AgreementCreateDispatcher<C> {
    pub clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> super::OtherPost for AgreementCreateDispatcher<C> {
    fn validate(&self, input: &super::OtherPostInput) -> Result<(), String> {
        if input.branch != crate::field_service_http_wire::OtherPostBranch::AgreementCreate {
            return Err("This action is not available.".into());
        }
        normalize_agreement_create(&input.body)
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
            let normalized = normalize_agreement_create(&input.body).map_err(|_| {
                BusinessError::invalid("FIELD_SERVICE_AGREEMENT_INVALID", "Invalid Agreement.")
            })?;
            if normalized.end_date < normalized.start_date {
                return Err(BusinessError::invalid(
                    "FIELD_SERVICE_DATE_RANGE",
                    "Agreement end date must be on or after its start date.",
                )
                .into());
            }
            let key = CommandKey::new(input.key)?;
            let id = format!("service-agreement-{}", key.as_str());
            let request = WorkflowRequest {
                key: key.as_str().into(),
                operation: "field.agreement.create".into(),
                record_id: id.clone(),
                input: normalized.receipt_input.clone(),
            };
            let ordered = SourceOrderedWorkflowRequest {
                key: request.key.clone(),
                operation: request.operation.clone(),
                record_id: WorkflowRecordIdentity::from_utf16(id.encode_utf16().collect()),
                input: request.input.clone(),
            };
            let mut authority = Authority {
                current,
                expected: &request,
                clock: self.clock.as_ref(),
                purchase: None,
                asset: normalized
                    .asset_id
                    .as_deref()
                    .is_some_and(|s| !s.is_empty()),
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
            let mut effects = Effects {
                current,
                input: &normalized,
                key: &key,
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
