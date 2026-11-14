//! Short owner transactions surround compute. No transaction-bound source or
//! permission-bearing context crosses the computation phase.
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, PgQueuedWorkAuthorizer, PgQueuedWorkSourcePort, WorkAuthorizationRequest,
};
use erpx_server_legal::public::{
    CurrentLegalProcessingSource, PdfVerifierProcess, PgLegalPdfVerificationBridge,
    PgLegalProcessingBridge, compute_prepared_legal_pdf, prepare_current_legal_pdf,
    retain_current_legal_pdf,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, IdentifierSource};
use std::sync::Arc;

/// Concrete composition must implement current record/purchase/Files/readiness
/// owner ports. There is no permissive or synthetic production implementation.
/// A new bridge (and all local witnesses) is built in each short transaction.
pub trait LegalPdfBridgeFactory: Clone + Send + Sync + 'static {
    type Bridge<'c>: PgLegalProcessingBridge + PgLegalPdfVerificationBridge + Send
    where
        Self: 'c;
    type Identifiers: IdentifierSource + Send;
    fn bridge<'c>(&'c self, current: &'c AuthorizedWorkContext) -> Self::Bridge<'c>;
    fn identifiers(&self) -> Self::Identifiers;
}

pub async fn run_legal_pdf_short_transactions<Factory, Verifier, ClockSource, Source>(
    authorizer: &PgQueuedWorkAuthorizer,
    source: Source,
    factory: Factory,
    verifier: &Verifier,
    clock: Arc<ClockSource>,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>>
where
    Factory: LegalPdfBridgeFactory,
    Verifier: PdfVerifierProcess + Sync,
    ClockSource: Clock + Send + Sync + 'static,
    Source: PgQueuedWorkSourcePort<Source = CurrentLegalProcessingSource>,
{
    let access = WorkAuthorizationRequest {
        permission: "legal.read".into(),
        module: Some("legal".into()),
        destination: "legal/processing".into(),
        api: true,
    };
    let prepare_factory = factory.clone();
    let prepare_clock = clock.clone();
    let prepared = authorizer
        .execute(
            source.clone(),
            access.clone(),
            move |connection, current, owned| {
                let factory = prepare_factory.clone();
                let clock = prepare_clock.clone();
                Box::pin(async move {
                    let mut identifiers = factory.identifiers();
                    let mut bridge = factory.bridge(current);
                    prepare_current_legal_pdf(
                        connection,
                        &mut bridge,
                        clock.as_ref(),
                        &mut identifiers,
                        owned,
                    )
                    .await
                })
            },
        )
        .await?;
    // The previous execute has returned and committed; no Control/Holding
    // lease or opaque source is retained while the bounded child runs.
    let computed = Arc::new(compute_prepared_legal_pdf(prepared, verifier, clock.as_ref()).await?);
    authorizer
        .execute(source, access, move |connection, current, owned| {
            let factory = factory.clone();
            let clock = clock.clone();
            let computed = computed.clone();
            Box::pin(async move {
                let mut identifiers = factory.identifiers();
                let mut bridge = factory.bridge(current);
                retain_current_legal_pdf(
                    connection,
                    &mut bridge,
                    clock.as_ref(),
                    &mut identifiers,
                    owned,
                    computed.as_ref(),
                )
                .await
            })
        })
        .await
}
