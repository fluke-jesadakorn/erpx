use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_kernel::TenantScope;
use erpx_proto::platform as wire;
use erpx_quality::{
    CancelInspection, CreateInspection, CreateInspectionResult, GetInspection, GetInspectionResult,
    InspectionId, InspectionKind, InspectionList, InspectionMutationResult, InspectionStatus,
    InspectionView, ListInspections, RecordInspectionResult, ReleaseInspection, StartInspection,
    deterministic_reference,
};
use erpx_ui_foundation::UiError;

use crate::{features::quality::QualityProtoTransport, transport::typed_cqrs::GrpcChannel};

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, encode_scope, invalid_response,
    query_result, status_ui, timestamp, uuid,
};

type CommandClient = wire::quality_command_service_client::QualityCommandServiceClient<GrpcChannel>;
type QueryClient = wire::quality_query_service_client::QualityQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct QualityGrpcTransport {
    auth: AuthenticatedRoute,
    commands: CommandClient,
    queries: QueryClient,
}

impl QualityGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl Into<String>,
        route: Wave67Route,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_new(access_token, route)?,
            commands: CommandClient::new(channel.clone()),
            queries: QueryClient::new(channel),
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl QualityProtoTransport for QualityGrpcTransport {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreateInspection>,
    ) -> Result<CommandReceipt<CreateInspectionResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation_id = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .create(self.auth.request(
                wire::CreateQualityInspectionRequest {
                    metadata: Some(metadata),
                    inspection_id: value.inspection_id.as_uuid().to_string(),
                    scope: Some(encode_scope(&value.scope)),
                    kind: encode_kind(value.kind),
                    product_id: value.product_id.to_string(),
                    lot_reference: value.lot_reference,
                    quantity: value.quantity,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("quality create inspection", status))?
            .into_inner();
        let inspection = decode_inspection(
            response
                .inspection
                .ok_or_else(|| invalid_response("decode quality inspection"))?,
        )?;
        command_result(
            response.receipt,
            CreateInspectionResult { inspection },
            command_id,
            correlation_id,
        )
    }

    async fn start(
        &self,
        envelope: CommandEnvelope<StartInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation_id = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .start(self.auth.request(
                wire::StartQualityInspectionRequest {
                    metadata: Some(metadata),
                    inspection_id: envelope.command.inspection_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("quality start inspection", status))?
            .into_inner();
        mutation_result(
            response.receipt,
            response.inspection,
            command_id,
            correlation_id,
        )
    }

    async fn record_result(
        &self,
        envelope: CommandEnvelope<RecordInspectionResult>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation_id = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .record_result(self.auth.request(
                wire::RecordQualityInspectionResultRequest {
                    metadata: Some(metadata),
                    inspection_id: value.inspection_id.as_uuid().to_string(),
                    passed_quantity: value.passed_quantity,
                    failed_quantity: value.failed_quantity,
                    disposition_note: value.note,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("quality record inspection result", status))?
            .into_inner();
        mutation_result(
            response.receipt,
            response.inspection,
            command_id,
            correlation_id,
        )
    }

    async fn release(
        &self,
        envelope: CommandEnvelope<ReleaseInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation_id = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .release(self.auth.request(
                wire::ReleaseQualityInspectionRequest {
                    metadata: Some(metadata),
                    inspection_id: envelope.command.inspection_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("quality release inspection", status))?
            .into_inner();
        mutation_result(
            response.receipt,
            response.inspection,
            command_id,
            correlation_id,
        )
    }

    async fn cancel(
        &self,
        envelope: CommandEnvelope<CancelInspection>,
    ) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation_id = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .cancel(self.auth.request(
                wire::CancelQualityInspectionRequest {
                    metadata: Some(metadata),
                    inspection_id: envelope.command.inspection_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("quality cancel inspection", status))?
            .into_inner();
        mutation_result(
            response.receipt,
            response.inspection,
            command_id,
            correlation_id,
        )
    }

    async fn get(
        &self,
        envelope: QueryEnvelope<GetInspection>,
    ) -> Result<QueryResult<GetInspectionResult>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation_id = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let response = self
            .queries
            .clone()
            .get(self.auth.request(
                wire::GetQualityInspectionRequest {
                    metadata: Some(metadata),
                    inspection_id: envelope.query.inspection_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("quality get inspection", status))?
            .into_inner();
        let value = GetInspectionResult {
            inspection: response.inspection.map(decode_inspection).transpose()?,
        };
        query_result(response.metadata, value, correlation_id, consistency)
    }

    async fn list(
        &self,
        envelope: QueryEnvelope<ListInspections>,
    ) -> Result<QueryResult<InspectionList>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation_id = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let filter = envelope.query.filter;
        let response = self
            .queries
            .clone()
            .list(self.auth.request(
                wire::ListQualityInspectionsRequest {
                    metadata: Some(metadata),
                    filter: Some(wire::QualityInspectionFilter {
                        status: filter.status.map(encode_status),
                        product_id: filter.product_id.map(|value| value.to_string()),
                        page_size: u32::from(filter.page_size),
                        cursor: filter.cursor,
                    }),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("quality list inspections", status))?
            .into_inner();
        let value = InspectionList {
            inspections: response
                .inspections
                .into_iter()
                .map(decode_inspection)
                .collect::<Result<_, _>>()?,
            next_cursor: response.next_cursor,
        };
        query_result(response.metadata, value, correlation_id, consistency)
    }
}

fn mutation_result(
    receipt: Option<erpx_proto::common::v1::CommandReceipt>,
    inspection: Option<wire::QualityInspection>,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<InspectionMutationResult>, UiError> {
    let inspection = decode_inspection(
        inspection.ok_or_else(|| invalid_response("decode quality inspection"))?,
    )?;
    command_result(
        receipt,
        InspectionMutationResult { inspection },
        command_id,
        correlation_id,
    )
}

fn decode_inspection(value: wire::QualityInspection) -> Result<InspectionView, UiError> {
    let inspection_id =
        InspectionId::try_from_uuid(uuid(&value.inspection_id, "decode quality inspection")?)
            .map_err(|_| invalid_response("decode quality inspection"))?;
    let scope = decode_scope(value.scope)?;
    validate_redundant_scope(
        &scope,
        &value.holding_id,
        &value.company_id,
        value.branch_id.as_deref(),
    )?;
    let created_at = timestamp(&value.created_at)?;
    let updated_at = timestamp(&value.updated_at)?;
    if updated_at < created_at || value.source_version == 0 || value.quantity == 0 {
        return Err(invalid_response("decode quality inspection"));
    }
    if value.reference != deterministic_reference(inspection_id) {
        return Err(invalid_response("decode quality inspection"));
    }
    validate_optional_text(value.lot_reference.as_deref())?;
    validate_optional_text(value.disposition_note.as_deref())?;
    validate_result_quantities(
        value.status,
        value.quantity,
        value.passed_quantity,
        value.failed_quantity,
    )?;
    if !value.projected_at.is_empty() {
        timestamp(&value.projected_at)?;
    }
    Ok(InspectionView {
        inspection_id,
        version: value.source_version,
        reference: value.reference,
        scope,
        kind: decode_kind(value.kind)?,
        product_id: uuid(&value.product_id, "decode quality inspection")?,
        lot_reference: value.lot_reference,
        quantity: value.quantity,
        passed_quantity: value.passed_quantity,
        failed_quantity: value.failed_quantity,
        status: decode_status(value.status)?,
        disposition_note: value.disposition_note,
        created_at,
        updated_at,
    })
}

fn validate_redundant_scope(
    scope: &TenantScope,
    holding_id: &str,
    company_id: &str,
    branch_id: Option<&str>,
) -> Result<(), UiError> {
    let holding = uuid(holding_id, "decode quality scope")?;
    let company = uuid(company_id, "decode quality scope")?;
    let branch = branch_id
        .map(|value| uuid(value, "decode quality scope"))
        .transpose()?;
    let matches = match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => *holding_id == holding && *company_id == company && branch.is_none(),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => *holding_id == holding && *company_id == company && Some(*branch_id) == branch,
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(invalid_response("decode quality scope"))
    }
}

fn validate_optional_text(value: Option<&str>) -> Result<(), UiError> {
    if value.is_some_and(|value| {
        value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control)
    }) {
        Err(invalid_response("decode quality text"))
    } else {
        Ok(())
    }
}

fn validate_result_quantities(
    status: i32,
    quantity: u64,
    passed: u64,
    failed: u64,
) -> Result<(), UiError> {
    let status = decode_status(status)?;
    let accounted = passed
        .checked_add(failed)
        .ok_or_else(|| invalid_response("decode quality result"))?;
    let valid = match status {
        InspectionStatus::Draft | InspectionStatus::InProgress => passed == 0 && failed == 0,
        InspectionStatus::Passed | InspectionStatus::Released => passed == quantity && failed == 0,
        InspectionStatus::Failed => failed > 0 && accounted == quantity,
        // A failed inspection may be cancelled without clearing its recorded
        // result. The aggregate permits this transition and retains both
        // quantities, so cancellation only applies the global upper bound.
        InspectionStatus::Cancelled => true,
    };
    if valid && accounted <= quantity {
        Ok(())
    } else {
        Err(invalid_response("decode quality result"))
    }
}

fn encode_kind(value: InspectionKind) -> i32 {
    match value {
        InspectionKind::Incoming => wire::QualityInspectionKind::Incoming as i32,
        InspectionKind::InProcess => wire::QualityInspectionKind::InProcess as i32,
        InspectionKind::Final => wire::QualityInspectionKind::Final as i32,
    }
}

fn decode_kind(value: i32) -> Result<InspectionKind, UiError> {
    match wire::QualityInspectionKind::try_from(value).ok() {
        Some(wire::QualityInspectionKind::Incoming) => Ok(InspectionKind::Incoming),
        Some(wire::QualityInspectionKind::InProcess) => Ok(InspectionKind::InProcess),
        Some(wire::QualityInspectionKind::Final) => Ok(InspectionKind::Final),
        _ => Err(invalid_response("decode quality kind")),
    }
}

fn encode_status(value: InspectionStatus) -> i32 {
    match value {
        InspectionStatus::Draft => wire::QualityInspectionStatus::Draft as i32,
        InspectionStatus::InProgress => wire::QualityInspectionStatus::InProgress as i32,
        InspectionStatus::Passed => wire::QualityInspectionStatus::Passed as i32,
        InspectionStatus::Failed => wire::QualityInspectionStatus::Failed as i32,
        InspectionStatus::Released => wire::QualityInspectionStatus::Released as i32,
        InspectionStatus::Cancelled => wire::QualityInspectionStatus::Cancelled as i32,
    }
}

fn decode_status(value: i32) -> Result<InspectionStatus, UiError> {
    match wire::QualityInspectionStatus::try_from(value).ok() {
        Some(wire::QualityInspectionStatus::Draft) => Ok(InspectionStatus::Draft),
        Some(wire::QualityInspectionStatus::InProgress) => Ok(InspectionStatus::InProgress),
        Some(wire::QualityInspectionStatus::Passed) => Ok(InspectionStatus::Passed),
        Some(wire::QualityInspectionStatus::Failed) => Ok(InspectionStatus::Failed),
        Some(wire::QualityInspectionStatus::Released) => Ok(InspectionStatus::Released),
        Some(wire::QualityInspectionStatus::Cancelled) => Ok(InspectionStatus::Cancelled),
        _ => Err(invalid_response("decode quality status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_kernel::TenantScope;
    use uuid::Uuid;

    #[test]
    fn every_enum_round_trips_and_unspecified_fails_closed() {
        for kind in [
            InspectionKind::Incoming,
            InspectionKind::InProcess,
            InspectionKind::Final,
        ] {
            assert_eq!(decode_kind(encode_kind(kind)).unwrap(), kind);
        }
        for status in [
            InspectionStatus::Draft,
            InspectionStatus::InProgress,
            InspectionStatus::Passed,
            InspectionStatus::Failed,
            InspectionStatus::Released,
            InspectionStatus::Cancelled,
        ] {
            assert_eq!(decode_status(encode_status(status)).unwrap(), status);
        }
        assert!(decode_kind(wire::QualityInspectionKind::Unspecified as i32).is_err());
        assert!(decode_status(wire::QualityInspectionStatus::Unspecified as i32).is_err());
    }

    #[test]
    fn redundant_scope_must_match_typed_scope() {
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        let scope = TenantScope::Company {
            holding_id: holding,
            company_id: company,
        };
        assert!(
            validate_redundant_scope(&scope, &holding.to_string(), &company.to_string(), None)
                .is_ok()
        );
        assert!(
            validate_redundant_scope(
                &scope,
                &holding.to_string(),
                &Uuid::new_v4().to_string(),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn result_invariants_fail_closed() {
        assert!(
            validate_result_quantities(wire::QualityInspectionStatus::Passed as i32, 10, 10, 0)
                .is_ok()
        );
        assert!(
            validate_result_quantities(wire::QualityInspectionStatus::Passed as i32, 10, 9, 1)
                .is_err()
        );
        assert!(
            validate_result_quantities(wire::QualityInspectionStatus::Cancelled as i32, 10, 8, 2)
                .is_ok()
        );
        assert!(
            validate_result_quantities(wire::QualityInspectionStatus::Cancelled as i32, 10, 9, 2)
                .is_err()
        );
    }
}
