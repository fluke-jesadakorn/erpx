//! Native eligibility handoff read use case.
//!
//! The caller keeps its serializable Holding transaction. This layer checks the
//! current Learning actor and selected scope, validates every bounded source
//! fact and never writes or consumes a certificate.
use crate::domain::eligibility_facts::{
    EligibilityCertificateRow, EligibilityQuery, EligibilityScope, eligibility_facts_json,
};
use crate::domain::learning::LearningActor;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type EligibilityFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait EligibilityReadRepository {
    type Error: From<BusinessError>;

    fn eligibility_certificates<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        person_id: &'a str,
        eligibility_code: &'a str,
    ) -> EligibilityFuture<'a, Vec<EligibilityCertificateRow>, Self::Error>;
}

pub async fn learning_eligibility_facts<R: EligibilityReadRepository>(
    repository: &mut R,
    actor: &LearningActor,
    query: &EligibilityQuery,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    let scope = EligibilityScope::from_learning_actor(actor);
    let query = query.validate()?;
    let rows = repository
        .eligibility_certificates(&scope, &query.person_id, &query.eligibility_code)
        .await?;
    eligibility_facts_json(&scope, &query, rows).map_err(Into::into)
}

/// Owner facts for a direct consumer such as Benefits inside its own
/// serializable transaction. The caller owns its business authority; this
/// function enforces only current Company/branch/department source scope.
pub async fn learning_eligibility_consumer_facts<R: EligibilityReadRepository>(
    repository: &mut R,
    scope: &EligibilityScope,
    query: &EligibilityQuery,
) -> Result<Value, R::Error> {
    let query = query.validate()?;
    let rows = repository
        .eligibility_certificates(scope, &query.person_id, &query.eligibility_code)
        .await?;
    eligibility_facts_json(scope, &query, rows).map_err(Into::into)
}
