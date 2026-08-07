use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_legal::{
    AccessEntryId, ContractParty, ContractStatus, ContractTransition, DocumentVersion,
    LegalCommand, LegalContractId, LegalDocumentId, LegalMutationResult, LegalQuery,
    LegalQueryResponse,
    ports::{
        AccessEntryView, AccessHistoryList, ContractList, ContractView, DocumentList, DocumentView,
    },
};
use erpx_proto::platform as wire;
use erpx_ui_foundation::UiError;

use crate::{features::legal::LegalProtoTransport, transport::typed_cqrs::GrpcChannel};

use super::common::{
    AuthenticatedRoute, Wave8Route, command_result, decode_scope, invalid_response, query_result,
    status_ui, timestamp, uuid,
};

type CommandClient = wire::legal_command_service_client::LegalCommandServiceClient<GrpcChannel>;
type QueryClient = wire::legal_query_service_client::LegalQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct LegalGrpcTransport {
    auth: AuthenticatedRoute,
    commands: CommandClient,
    queries: QueryClient,
}

impl LegalGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl Into<String>,
        route: Wave8Route,
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
impl LegalProtoTransport for LegalGrpcTransport {
    async fn command(
        &self,
        envelope: CommandEnvelope<LegalCommand>,
    ) -> Result<CommandReceipt<LegalMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation_id = envelope.correlation_id;
        let mut client = self.commands.clone();
        match envelope.command {
            LegalCommand::CreateContract(value) => {
                let request = wire::CreateLegalContractRequest {
                    metadata: Some(metadata),
                    contract_id: value.contract_id.as_uuid().to_string(),
                    target_scope: Some(super::common::encode_scope(&value.scope)),
                    reference: value.reference,
                    title: value.title,
                    object_ref: value.object_ref,
                    content_sha256: value.content_sha256,
                    retention_until: value.retention_until.to_rfc3339(),
                };
                let response = client
                    .create_contract(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal create contract", status))?
                    .into_inner();
                contract_command(response, command_id, correlation_id)
            }
            LegalCommand::AddContractParty(value) => {
                let request = wire::AddLegalContractPartyRequest {
                    metadata: Some(metadata),
                    contract_id: value.contract_id.as_uuid().to_string(),
                    party: Some(wire::LegalContractPartyRecord {
                        party_id: value.party.party_id.to_string(),
                        role: value.party.role,
                    }),
                };
                let response = client
                    .add_contract_party(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal add contract party", status))?
                    .into_inner();
                contract_command(response, command_id, correlation_id)
            }
            LegalCommand::TransitionContract(value) => {
                let request = wire::TransitionLegalContractRequest {
                    metadata: Some(metadata),
                    contract_id: value.contract_id.as_uuid().to_string(),
                    transition: encode_transition(value.transition),
                };
                let response = client
                    .transition_contract(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal transition contract", status))?
                    .into_inner();
                contract_command(response, command_id, correlation_id)
            }
            LegalCommand::CreateDocument(value) => {
                let request = wire::CreateLegalDocumentRequest {
                    metadata: Some(metadata),
                    document_id: value.document_id.as_uuid().to_string(),
                    target_scope: Some(super::common::encode_scope(&value.scope)),
                    title: value.title,
                };
                let response = client
                    .create_document(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal create document", status))?
                    .into_inner();
                document_command(response, command_id, correlation_id)
            }
            LegalCommand::AddDocumentVersion(value) => {
                let request = wire::AddLegalDocumentVersionRequest {
                    metadata: Some(metadata),
                    document_id: value.document_id.as_uuid().to_string(),
                    object_ref: value.object_ref,
                    content_sha256: value.content_sha256,
                };
                let response = client
                    .add_document_version(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal add document version", status))?
                    .into_inner();
                document_command(response, command_id, correlation_id)
            }
            LegalCommand::ArchiveDocument(value) => {
                let request = wire::ArchiveLegalDocumentRequest {
                    metadata: Some(metadata),
                    document_id: value.document_id.as_uuid().to_string(),
                };
                let response = client
                    .archive_document(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal archive document", status))?
                    .into_inner();
                document_command(response, command_id, correlation_id)
            }
            LegalCommand::RecordDocumentAccess(value) => {
                let request = wire::RecordLegalDocumentAccessRequest {
                    metadata: Some(metadata),
                    access_entry_id: value.access_entry_id.as_uuid().to_string(),
                    document_id: value.document_id.as_uuid().to_string(),
                    document_version: value.document_version,
                    action: value.action,
                    purpose: value.purpose,
                    observed_at: value.observed_at.to_rfc3339(),
                };
                let response = client
                    .record_document_access(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal record document access", status))?
                    .into_inner();
                let result = LegalMutationResult::AccessEntry(decode_access(
                    response
                        .access_entry
                        .ok_or_else(|| invalid_response("decode legal access entry"))?,
                )?);
                command_result(response.receipt, result, command_id, correlation_id)
            }
        }
    }

    async fn query(
        &self,
        envelope: QueryEnvelope<LegalQuery>,
    ) -> Result<QueryResult<LegalQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            LegalQuery::GetContract(value) => {
                let request = wire::GetLegalContractRequest {
                    metadata: Some(metadata),
                    contract_id: value.contract_id.as_uuid().to_string(),
                };
                let response = client
                    .get_contract(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal get contract", status))?
                    .into_inner();
                let value = response.contract.map(decode_contract).transpose()?;
                query_result(
                    response.metadata,
                    LegalQueryResponse::Contract(value),
                    correlation,
                    consistency,
                )
            }
            LegalQuery::ListContracts(value) => {
                let request = wire::ListTypedLegalContractsRequest {
                    metadata: Some(metadata),
                    status: value.filter.status.map(encode_status),
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_contracts(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal list contracts", status))?
                    .into_inner();
                let value = ContractList {
                    contracts: response
                        .contracts
                        .into_iter()
                        .map(decode_contract)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    LegalQueryResponse::Contracts(value),
                    correlation,
                    consistency,
                )
            }
            LegalQuery::GetDocument(value) => {
                let request = wire::GetLegalDocumentRequest {
                    metadata: Some(metadata),
                    document_id: value.document_id.as_uuid().to_string(),
                };
                let response = client
                    .get_document(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal get document", status))?
                    .into_inner();
                let value = response.document.map(decode_document).transpose()?;
                query_result(
                    response.metadata,
                    LegalQueryResponse::Document(value),
                    correlation,
                    consistency,
                )
            }
            LegalQuery::ListDocuments(value) => {
                let request = wire::ListTypedLegalDocumentsRequest {
                    metadata: Some(metadata),
                    archived: value.filter.archived,
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_documents(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal list documents", status))?
                    .into_inner();
                let value = DocumentList {
                    documents: response
                        .documents
                        .into_iter()
                        .map(decode_document)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    LegalQueryResponse::Documents(value),
                    correlation,
                    consistency,
                )
            }
            LegalQuery::ListAccessHistory(value) => {
                let request = wire::ListTypedLegalAccessHistoryRequest {
                    metadata: Some(metadata),
                    document_id: value
                        .filter
                        .document_id
                        .map(|value| value.as_uuid().to_string()),
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_access_history(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("legal list access history", status))?
                    .into_inner();
                let value = AccessHistoryList {
                    entries: response
                        .access_entries
                        .into_iter()
                        .map(decode_access)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    LegalQueryResponse::AccessHistory(value),
                    correlation,
                    consistency,
                )
            }
        }
    }
}

fn contract_command(
    response: wire::LegalContractMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<LegalMutationResult>, UiError> {
    let value = LegalMutationResult::Contract(decode_contract(
        response
            .contract
            .ok_or_else(|| invalid_response("decode legal contract"))?,
    )?);
    command_result(response.receipt, value, command_id, correlation_id)
}

fn document_command(
    response: wire::LegalDocumentMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<LegalMutationResult>, UiError> {
    let value = LegalMutationResult::Document(decode_document(
        response
            .document
            .ok_or_else(|| invalid_response("decode legal document"))?,
    )?);
    command_result(response.receipt, value, command_id, correlation_id)
}

fn decode_contract(value: wire::LegalContractRecord) -> Result<ContractView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode legal contract"));
    }
    Ok(ContractView {
        contract_id: LegalContractId::try_from_uuid(uuid(
            &value.contract_id,
            "decode legal contract",
        )?)
        .map_err(|_| invalid_response("decode legal contract"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        reference: value.reference,
        title: value.title,
        object_ref: value.object_ref,
        content_sha256: value.content_sha256,
        retention_until: timestamp(&value.retention_until)?,
        status: decode_status(value.status)?,
        parties: value
            .parties
            .into_iter()
            .map(|party| {
                ContractParty::try_new(uuid(&party.party_id, "decode legal contract")?, party.role)
                    .map_err(|_| invalid_response("decode legal contract"))
            })
            .collect::<Result<_, _>>()?,
        created_by: value.created_by,
        approved_by: value.approved_by,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_document(value: wire::LegalDocumentRecord) -> Result<DocumentView, UiError> {
    if value.source_version == 0 || value.current_version == 0 {
        return Err(invalid_response("decode legal document"));
    }
    Ok(DocumentView {
        document_id: LegalDocumentId::try_from_uuid(uuid(
            &value.document_id,
            "decode legal document",
        )?)
        .map_err(|_| invalid_response("decode legal document"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        title: value.title,
        versions: value
            .versions
            .into_iter()
            .map(|version| {
                if version.version == 0 {
                    return Err(invalid_response("decode legal document"));
                }
                Ok(DocumentVersion {
                    version: version.version,
                    object_ref: version.object_ref,
                    content_sha256: version.content_sha256,
                    created_at: timestamp(&version.created_at)?,
                    created_by: version.created_by,
                })
            })
            .collect::<Result<_, UiError>>()?,
        current_version: value.current_version,
        archived: value.archived,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_access(value: wire::LegalAccessEntryRecord) -> Result<AccessEntryView, UiError> {
    if value.source_version == 0 || value.document_version == 0 {
        return Err(invalid_response("decode legal access entry"));
    }
    Ok(AccessEntryView {
        access_entry_id: AccessEntryId::try_from_uuid(uuid(
            &value.access_entry_id,
            "decode legal access entry",
        )?)
        .map_err(|_| invalid_response("decode legal access entry"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        document_id: LegalDocumentId::try_from_uuid(uuid(
            &value.document_id,
            "decode legal access entry",
        )?)
        .map_err(|_| invalid_response("decode legal access entry"))?,
        document_version: value.document_version,
        actor_ref: value.actor_ref,
        action: value.action,
        purpose: value.purpose,
        observed_at: timestamp(&value.observed_at)?,
    })
}

fn encode_transition(value: ContractTransition) -> i32 {
    (match value {
        ContractTransition::SubmitForReview => wire::LegalContractTransition::SubmitForReview,
        ContractTransition::Approve => wire::LegalContractTransition::Approve,
        ContractTransition::Activate => wire::LegalContractTransition::Activate,
        ContractTransition::Expire => wire::LegalContractTransition::Expire,
        ContractTransition::Terminate => wire::LegalContractTransition::Terminate,
        ContractTransition::Archive => wire::LegalContractTransition::Archive,
    }) as i32
}

fn encode_status(value: ContractStatus) -> i32 {
    (match value {
        ContractStatus::Draft => wire::LegalContractStatus::Draft,
        ContractStatus::Review => wire::LegalContractStatus::Review,
        ContractStatus::Approved => wire::LegalContractStatus::Approved,
        ContractStatus::Active => wire::LegalContractStatus::Active,
        ContractStatus::Expired => wire::LegalContractStatus::Expired,
        ContractStatus::Terminated => wire::LegalContractStatus::Terminated,
        ContractStatus::Archived => wire::LegalContractStatus::Archived,
    }) as i32
}

fn decode_status(value: i32) -> Result<ContractStatus, UiError> {
    match wire::LegalContractStatus::try_from(value).ok() {
        Some(wire::LegalContractStatus::Draft) => Ok(ContractStatus::Draft),
        Some(wire::LegalContractStatus::Review) => Ok(ContractStatus::Review),
        Some(wire::LegalContractStatus::Approved) => Ok(ContractStatus::Approved),
        Some(wire::LegalContractStatus::Active) => Ok(ContractStatus::Active),
        Some(wire::LegalContractStatus::Expired) => Ok(ContractStatus::Expired),
        Some(wire::LegalContractStatus::Terminated) => Ok(ContractStatus::Terminated),
        Some(wire::LegalContractStatus::Archived) => Ok(ContractStatus::Archived),
        Some(wire::LegalContractStatus::Unspecified) | None => {
            Err(invalid_response("decode legal contract status"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_contract_status_round_trips() {
        for status in [
            ContractStatus::Draft,
            ContractStatus::Review,
            ContractStatus::Approved,
            ContractStatus::Active,
            ContractStatus::Expired,
            ContractStatus::Terminated,
            ContractStatus::Archived,
        ] {
            assert_eq!(decode_status(encode_status(status)), Ok(status));
        }
    }
}
