//! OCR uses the same short authorization topology as signature inspection.
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, PgQueuedWorkAuthorizer, PgQueuedWorkSourcePort, WorkAuthorizationRequest,
};
use erpx_server_legal::public::document_processing::{
    LegalExtractionRetention, PgLegalDocumentExtractionBridge, compute_prepared_legal_extraction,
    prepare_current_legal_extraction, retain_current_legal_extraction,
};
use erpx_server_legal::public::{
    CurrentLegalProcessingSource, DocumentExtractor, PgLegalProcessingBridge,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, IdentifierSource};
use std::sync::Arc;
pub trait LegalDocumentBridgeFactory: Clone + Send + Sync + 'static {
    type Bridge<'c>: PgLegalProcessingBridge + PgLegalDocumentExtractionBridge + Send
    where
        Self: 'c;
    type Identifiers: IdentifierSource + Send;
    fn bridge<'c>(&'c self, current: &'c AuthorizedWorkContext) -> Self::Bridge<'c>;
    fn identifiers(&self) -> Self::Identifiers;
}
pub async fn run_legal_extraction_short_transactions<Factory, Extractor, ClockSource, Source>(
    authorizer: &PgQueuedWorkAuthorizer,
    source: Source,
    factory: Factory,
    extractor: &Extractor,
    clock: Arc<ClockSource>,
) -> Result<LegalExtractionRetention, PgTransactionError<BusinessError>>
where
    Factory: LegalDocumentBridgeFactory,
    Extractor: DocumentExtractor + Sync,
    ClockSource: Clock + Send + Sync + 'static,
    Source: PgQueuedWorkSourcePort<Source = CurrentLegalProcessingSource>,
{
    let access = WorkAuthorizationRequest {
        permission: "legal.read".into(),
        module: Some("legal".into()),
        destination: "legal/processing".into(),
        api: true,
    };
    let preparing = factory.clone();
    let prepare_clock = clock.clone();
    let prepared = authorizer
        .execute(source.clone(), access.clone(), move |c, current, owned| {
            let factory = preparing.clone();
            let clock = prepare_clock.clone();
            Box::pin(async move {
                let mut ids = factory.identifiers();
                let mut bridge = factory.bridge(current);
                prepare_current_legal_extraction(c, &mut bridge, clock.as_ref(), &mut ids, owned)
                    .await
            })
        })
        .await?;
    let computed =
        Arc::new(compute_prepared_legal_extraction(prepared, extractor, clock.as_ref()).await?);
    authorizer
        .execute(source, access, move |c, current, owned| {
            let factory = factory.clone();
            let clock = clock.clone();
            let computed = computed.clone();
            Box::pin(async move {
                let mut ids = factory.identifiers();
                let mut bridge = factory.bridge(current);
                retain_current_legal_extraction(
                    c,
                    &mut bridge,
                    clock.as_ref(),
                    &mut ids,
                    owned,
                    computed.as_ref(),
                )
                .await
            })
        })
        .await
}
