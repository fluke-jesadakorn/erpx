//! Learning-owned adapter for the Benefits election consumption port.
//!
//! Benefits supplies the required eligibility code, person and effective date.
//! Learning selects and locks the current certificate, writes its own
//! consumption receipt after the Benefits election row exists, then re-reads
//! that receipt and certificate in the caller's serializable unit. No Benefits
//! row or Learning authority is created here.
use crate::adapters::learning::PgLearning;
use crate::application::eligibility_facts::{
    finalize_benefit_consumption, prepare_benefit_consumption, validate_benefit_consumption,
};
use crate::domain::eligibility_facts::{BenefitConsumptionRequest, PreparedBenefitConsumption};
use erpx_server_benefits::public::command::{
    BenefitLearningRequest, BenefitsCommandFuture, BenefitsLearningPort, PreparedBenefitLearning,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use sqlx::PgConnection;

fn unit_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_LEARNING_UNIT_REQUIRED",
        "Benefits eligibility consumption requires the caller's retained serializable unit.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_LEARNING_UNAVAILABLE",
        "The Learning eligibility consumption is temporarily unavailable.",
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "BENEFITS_LEARNING_RECEIPT_CHANGED",
        "The Learning benefit consumption receipt changed while the election was being retained.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Map the Benefits selection onto the Learning consumption request. The
/// required code, person, effective date and command identity are validated
/// before any owner row is read.
pub fn benefit_consumption_request(
    actor_id: &str,
    request: &BenefitLearningRequest,
) -> Result<BenefitConsumptionRequest, BusinessError> {
    BenefitConsumptionRequest {
        company_id: request.company_id.clone(),
        person_id: request.person_id.clone(),
        required_code: request.required_code.clone(),
        effective_date: request.effective_date.clone(),
        election_id: request.election_id.clone(),
        command_id: request.command_id.clone(),
        today: request.today.clone(),
        actor_id: actor_id.to_owned(),
    }
    .validate()
}

/// The opaque Benefits-facing identity retained for the post-effect readback.
pub fn benefit_learning_identity(
    prepared: &PreparedBenefitConsumption,
) -> Result<PreparedBenefitLearning, BusinessError> {
    let identity = PreparedBenefitLearning {
        certificate_id: prepared.plan.certificate_id.clone(),
        election_id: prepared.plan.election_id.clone(),
        command_id: prepared.plan.command_id.clone(),
    };
    identity.validate()?;
    Ok(identity)
}

/// Learning-owned Benefits consumption adapter. The caller supplies the
/// current Benefits actor and the retained transaction clock; each call uses
/// the caller's lent unit and never opens or commits a transaction.
pub struct PgBenefitLearning {
    actor_id: String,
    now: TimestampMillis,
    prepared: Option<PreparedBenefitConsumption>,
}

impl PgBenefitLearning {
    pub fn new(actor_id: String, now: TimestampMillis) -> Self {
        Self {
            actor_id,
            now,
            prepared: None,
        }
    }
}

impl BenefitsLearningPort for PgBenefitLearning {
    type Prepared = PreparedBenefitLearning;

    fn prepare<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request: BenefitLearningRequest,
    ) -> BenefitsCommandFuture<'a, Self::Prepared, BusinessError> {
        Box::pin(async move {
            let request = benefit_consumption_request(&self.actor_id, &request)?;
            let Some(connection) = unit else {
                return Err(unit_required());
            };
            let prepared = prepare_benefit_consumption(
                &mut PgLearning::new(connection),
                &request,
                self.now.get(),
            )
            .await
            .map_err(business)?;
            let identity = benefit_learning_identity(&prepared)?;
            self.prepared = Some(prepared);
            Ok(identity)
        })
    }

    fn validate<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        request: BenefitLearningRequest,
    ) -> BenefitsCommandFuture<'a, Self::Prepared, BusinessError> {
        Box::pin(async move {
            let request = benefit_consumption_request(&self.actor_id, &request)?;
            let Some(connection) = unit else {
                return Err(unit_required());
            };
            let prepared = validate_benefit_consumption(&mut PgLearning::new(connection), &request)
                .await
                .map_err(business)?;
            let identity = benefit_learning_identity(&prepared)?;
            self.prepared = Some(prepared);
            Ok(identity)
        })
    }

    fn require_current<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        prepared: &'a Self::Prepared,
    ) -> BenefitsCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let Some(connection) = unit else {
                return Err(unit_required());
            };
            let Some(current) = self.prepared.clone() else {
                return Err(changed());
            };
            if current.plan.certificate_id != prepared.certificate_id
                || current.plan.election_id != prepared.election_id
                || current.plan.command_id != prepared.command_id
            {
                return Err(changed());
            }
            let receipt = finalize_benefit_consumption(&mut PgLearning::new(connection), &current)
                .await
                .map_err(business)?;
            receipt.validate_source()?;
            if receipt.certificate_id != prepared.certificate_id
                || receipt.election_id != prepared.election_id
                || receipt.command_id != prepared.command_id
                || !receipt.matches_source(&current.plan)
            {
                return Err(changed());
            }
            self.prepared = None;
            Ok(())
        })
    }
}
