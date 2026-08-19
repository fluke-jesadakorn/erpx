use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_integration_contracts::{customer, identity_access};
use erpx_proto::erpx::support as wire;
use erpx_support::{
    BroadcastAudience, CapabilityId, CapabilityRevocationReason, IncidentId, IncidentStatus,
    MessageAudience, MessageId, SessionId, SessionStatus, SupportCommand, SupportMode,
    SupportMutationResult, SupportQuery, SupportQueryResponse, SupportTicketId, TicketStatus,
    ports::{
        CustomerTicketMessageView, IncidentList, IncidentView, InternalTicketMessageView,
        SupportCapabilityView, SupportSessionView, SupportTicketList, SupportTicketView,
        TicketReadAudience,
    },
};
use erpx_ui_foundation::UiError;

use crate::{features::support::SupportProtoTransport, transport::typed_cqrs::GrpcChannel};

use super::common::{
    AuthenticatedRoute, Wave8Route, command_result, decode_scope, invalid_response, query_result,
    status_ui, timestamp, unsupported_ui, uuid,
};

type CommandClient = wire::support_command_service_client::SupportCommandServiceClient<GrpcChannel>;
type QueryClient = wire::support_query_service_client::SupportQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct SupportGrpcTransport {
    auth: AuthenticatedRoute,
    commands: CommandClient,
    queries: QueryClient,
}

impl SupportGrpcTransport {
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
impl SupportProtoTransport for SupportGrpcTransport {
    async fn command(
        &self,
        envelope: CommandEnvelope<SupportCommand>,
    ) -> Result<CommandReceipt<SupportMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation_id = envelope.correlation_id;
        let mut client = self.commands.clone();
        match envelope.command {
            SupportCommand::OpenTicket(value) => {
                let request = wire::OpenSupportTicketCommandRequest {
                    metadata: Some(metadata),
                    ticket_id: value.ticket_id.as_uuid().to_string(),
                    customer_id: value.customer.customer.as_uuid().to_string(),
                    customer_account_id: value.customer.account.as_uuid().to_string(),
                    case_code: value.case_code,
                    category: value.category,
                    severity: value.severity,
                    description: value.description,
                };
                let response = client
                    .open_ticket(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support open ticket", status))?
                    .into_inner();
                internal_ticket_command(response, command_id, correlation_id)
            }
            SupportCommand::TransitionTicket(value) => {
                let request = wire::TransitionSupportTicketCommandRequest {
                    metadata: Some(metadata),
                    ticket_id: value.ticket_id.as_uuid().to_string(),
                    next: encode_ticket_status(value.next),
                };
                let response = client
                    .transition_ticket(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support transition ticket", status))?
                    .into_inner();
                internal_ticket_command(response, command_id, correlation_id)
            }
            SupportCommand::AddTicketMessage(value) => {
                let request = wire::AddSupportTicketMessageCommandRequest {
                    metadata: Some(metadata),
                    ticket_id: value.ticket_id.as_uuid().to_string(),
                    message_id: value.message_id.as_uuid().to_string(),
                    body: value.body,
                };
                match value.audience {
                    MessageAudience::CustomerVisible => {
                        let response = client
                            .add_customer_reply(self.auth.request(request, &actor)?)
                            .await
                            .map_err(|status| status_ui("support add customer reply", status))?
                            .into_inner();
                        let result = SupportMutationResult::Ticket(decode_customer_ticket(
                            response.ticket.ok_or_else(|| {
                                invalid_response("decode support customer ticket")
                            })?,
                        )?);
                        command_result(response.receipt, result, command_id, correlation_id)
                    }
                    MessageAudience::Internal => {
                        let response = client
                            .add_internal_note(self.auth.request(request, &actor)?)
                            .await
                            .map_err(|status| status_ui("support add internal note", status))?
                            .into_inner();
                        internal_ticket_command(response, command_id, correlation_id)
                    }
                }
            }
            SupportCommand::RequestSession(value) => {
                let request = wire::RequestTypedSupportSessionCommandRequest {
                    metadata: Some(metadata),
                    session_id: value.session_id.as_uuid().to_string(),
                    ticket_id: value.ticket_id.as_uuid().to_string(),
                    represented_principal_id: value
                        .represented_principal
                        .map(|value| value.as_uuid().to_string()),
                    mode: encode_mode(value.mode),
                    permissions: value.permissions,
                    approved_scope: Some(super::common::encode_scope(&value.approved_scope)),
                    module: value.module,
                    reason: value.reason,
                    duration_seconds: value.duration_seconds,
                };
                let response = client
                    .request_session(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support request session", status))?
                    .into_inner();
                session_command(response, command_id, correlation_id)
            }
            SupportCommand::ApproveSession(value) => {
                let request = wire::SupportSessionIdentityCommandRequest {
                    metadata: Some(metadata),
                    session_id: value.session_id.as_uuid().to_string(),
                };
                let response = client
                    .approve_session(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support approve session", status))?
                    .into_inner();
                session_command(response, command_id, correlation_id)
            }
            SupportCommand::RevokeSession(value) => {
                let request = wire::RevokeTypedSupportSessionCommandRequest {
                    metadata: Some(metadata),
                    session_id: value.session_id.as_uuid().to_string(),
                    reason: value.reason,
                };
                let response = client
                    .revoke_session(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support revoke session", status))?
                    .into_inner();
                session_command(response, command_id, correlation_id)
            }
            SupportCommand::IssueCapability(_) => {
                // Capability issuance requires secure server-resolved material;
                // the Console intentionally has no constructor for it.
                Err(unsupported_ui("support issue capability"))
            }
            SupportCommand::RevokeCapability(value) => {
                let request = wire::RevokeSupportCapabilityCommandRequest {
                    metadata: Some(metadata),
                    capability_id: value.capability_id.as_uuid().to_string(),
                    reason: encode_revocation(value.reason),
                };
                let response = client
                    .revoke_capability(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support revoke capability", status))?
                    .into_inner();
                capability_command(response, command_id, correlation_id)
            }
            SupportCommand::OpenIncident(value) => {
                let request = wire::OpenSupportIncidentCommandRequest {
                    metadata: Some(metadata),
                    incident_id: value.incident_id.as_uuid().to_string(),
                    incident_code: value.incident_code,
                    severity: value.severity,
                    affected_services: value.affected_services,
                    affected_customer_account_ids: value
                        .affected_customer_accounts
                        .into_iter()
                        .map(|value| value.as_uuid().to_string())
                        .collect(),
                };
                let response = client
                    .open_incident(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support open incident", status))?
                    .into_inner();
                incident_command(response, command_id, correlation_id)
            }
            SupportCommand::TransitionIncident(value) => {
                let request = wire::TransitionSupportIncidentCommandRequest {
                    metadata: Some(metadata),
                    incident_id: value.incident_id.as_uuid().to_string(),
                    next: encode_incident_status(value.next),
                };
                let response = client
                    .transition_incident(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support transition incident", status))?
                    .into_inner();
                incident_command(response, command_id, correlation_id)
            }
            SupportCommand::AddIncidentTimeline(value) => {
                let request = wire::AddSupportIncidentTimelineCommandRequest {
                    metadata: Some(metadata),
                    incident_id: value.incident_id.as_uuid().to_string(),
                    message: value.message,
                };
                let response = client
                    .add_incident_timeline(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support add incident timeline", status))?
                    .into_inner();
                incident_command(response, command_id, correlation_id)
            }
            SupportCommand::BroadcastIncident(value) => {
                let request = wire::BroadcastSupportIncidentCommandRequest {
                    metadata: Some(metadata),
                    incident_id: value.incident_id.as_uuid().to_string(),
                    audience: encode_broadcast_audience(value.audience),
                    message: value.message,
                };
                let response = client
                    .broadcast_incident(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support broadcast incident", status))?
                    .into_inner();
                incident_command(response, command_id, correlation_id)
            }
        }
    }

    async fn query(
        &self,
        envelope: QueryEnvelope<SupportQuery>,
    ) -> Result<QueryResult<SupportQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            SupportQuery::GetTicket(value) => match value.audience {
                TicketReadAudience::Customer => {
                    let request = wire::GetSupportCustomerTicketRecordRequest {
                        metadata: Some(metadata),
                        ticket_id: value.ticket_id.as_uuid().to_string(),
                    };
                    let response = client
                        .get_customer_ticket(self.auth.request(request, &actor)?)
                        .await
                        .map_err(|status| status_ui("support get customer ticket", status))?
                        .into_inner();
                    let value = response.ticket.map(decode_customer_ticket).transpose()?;
                    query_result(
                        response.metadata,
                        SupportQueryResponse::Ticket(value),
                        correlation,
                        consistency,
                    )
                }
                TicketReadAudience::Internal => {
                    let request = wire::GetSupportInternalTicketRecordRequest {
                        metadata: Some(metadata),
                        ticket_id: value.ticket_id.as_uuid().to_string(),
                    };
                    let response = client
                        .get_internal_ticket(self.auth.request(request, &actor)?)
                        .await
                        .map_err(|status| status_ui("support get internal ticket", status))?
                        .into_inner();
                    let value = response.ticket.map(decode_internal_ticket).transpose()?;
                    query_result(
                        response.metadata,
                        SupportQueryResponse::Ticket(value),
                        correlation,
                        consistency,
                    )
                }
            },
            SupportQuery::ListTickets(value) => match value.filter.audience {
                TicketReadAudience::Customer => {
                    let request = wire::ListSupportCustomerTicketRecordsRequest {
                        metadata: Some(metadata),
                        status: value.filter.status.map(encode_ticket_status),
                        customer_id: value
                            .filter
                            .customer
                            .map(|value| value.as_uuid().to_string()),
                        page_size: u32::from(value.filter.page_size),
                        cursor: value.filter.cursor,
                    };
                    let response = client
                        .list_customer_tickets(self.auth.request(request, &actor)?)
                        .await
                        .map_err(|status| status_ui("support list customer tickets", status))?
                        .into_inner();
                    let value = SupportTicketList {
                        tickets: response
                            .tickets
                            .into_iter()
                            .map(decode_customer_ticket)
                            .collect::<Result<_, _>>()?,
                        next_cursor: response.next_cursor,
                    };
                    query_result(
                        response.metadata,
                        SupportQueryResponse::Tickets(value),
                        correlation,
                        consistency,
                    )
                }
                TicketReadAudience::Internal => {
                    let request = wire::ListSupportInternalTicketRecordsRequest {
                        metadata: Some(metadata),
                        status: value.filter.status.map(encode_ticket_status),
                        customer_id: value
                            .filter
                            .customer
                            .map(|value| value.as_uuid().to_string()),
                        page_size: u32::from(value.filter.page_size),
                        cursor: value.filter.cursor,
                    };
                    let response = client
                        .list_internal_tickets(self.auth.request(request, &actor)?)
                        .await
                        .map_err(|status| status_ui("support list internal tickets", status))?
                        .into_inner();
                    let value = SupportTicketList {
                        tickets: response
                            .tickets
                            .into_iter()
                            .map(decode_internal_ticket)
                            .collect::<Result<_, _>>()?,
                        next_cursor: response.next_cursor,
                    };
                    query_result(
                        response.metadata,
                        SupportQueryResponse::Tickets(value),
                        correlation,
                        consistency,
                    )
                }
            },
            SupportQuery::GetIncident(value) => {
                let request = wire::GetSupportIncidentRecordRequest {
                    metadata: Some(metadata),
                    incident_id: value.incident_id.as_uuid().to_string(),
                };
                let response = client
                    .get_incident(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support get incident", status))?
                    .into_inner();
                let value = response.incident.map(decode_incident).transpose()?;
                query_result(
                    response.metadata,
                    SupportQueryResponse::Incident(value),
                    correlation,
                    consistency,
                )
            }
            SupportQuery::ListIncidents(value) => {
                let request = wire::ListSupportIncidentRecordsRequest {
                    metadata: Some(metadata),
                    status: value.filter.status.map(encode_incident_status),
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_incidents(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support list incidents", status))?
                    .into_inner();
                let value = IncidentList {
                    incidents: response
                        .incidents
                        .into_iter()
                        .map(decode_incident)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    SupportQueryResponse::Incidents(value),
                    correlation,
                    consistency,
                )
            }
            SupportQuery::GetSession(value) => {
                let request = wire::GetSupportSessionRecordRequest {
                    metadata: Some(metadata),
                    session_id: value.session_id.as_uuid().to_string(),
                };
                let response = client
                    .get_session(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support get session", status))?
                    .into_inner();
                let value = response.session.map(decode_session).transpose()?;
                query_result(
                    response.metadata,
                    SupportQueryResponse::Session(value),
                    correlation,
                    consistency,
                )
            }
            SupportQuery::GetCapability(value) => {
                let request = wire::GetSupportCapabilityRecordRequest {
                    metadata: Some(metadata),
                    capability_id: value.capability_id.as_uuid().to_string(),
                };
                let response = client
                    .get_capability(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("support get capability", status))?
                    .into_inner();
                let value = response.capability.map(decode_capability).transpose()?;
                query_result(
                    response.metadata,
                    SupportQueryResponse::Capability(value),
                    correlation,
                    consistency,
                )
            }
        }
    }
}

fn internal_ticket_command(
    response: wire::SupportInternalTicketMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<SupportMutationResult>, UiError> {
    let value = SupportMutationResult::Ticket(decode_internal_ticket(
        response
            .ticket
            .ok_or_else(|| invalid_response("decode support internal ticket"))?,
    )?);
    command_result(response.receipt, value, command_id, correlation_id)
}

fn session_command(
    response: wire::SupportSessionMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<SupportMutationResult>, UiError> {
    let value = SupportMutationResult::Session(decode_session(
        response
            .session
            .ok_or_else(|| invalid_response("decode support session"))?,
    )?);
    command_result(response.receipt, value, command_id, correlation_id)
}

fn capability_command(
    response: wire::SupportCapabilityMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<SupportMutationResult>, UiError> {
    let value = SupportMutationResult::Capability(decode_capability(
        response
            .capability
            .ok_or_else(|| invalid_response("decode support capability"))?,
    )?);
    command_result(response.receipt, value, command_id, correlation_id)
}

fn incident_command(
    response: wire::SupportIncidentMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<SupportMutationResult>, UiError> {
    let value = SupportMutationResult::Incident(decode_incident(
        response
            .incident
            .ok_or_else(|| invalid_response("decode support incident"))?,
    )?);
    command_result(response.receipt, value, command_id, correlation_id)
}

fn decode_customer_ticket(
    value: wire::SupportCustomerTicketRecord,
) -> Result<SupportTicketView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode support customer ticket"));
    }
    Ok(SupportTicketView {
        ticket_id: ticket_id(&value.ticket_id)?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        customer: customer_ref(&value.customer_id)?,
        customer_account: customer_account_ref(&value.customer_account_id)?,
        case_code: value.case_code,
        category: value.category,
        severity: value.severity,
        description: value.description,
        status: decode_ticket_status(value.status)?,
        customer_messages: value
            .messages
            .into_iter()
            .map(|message| {
                Ok(CustomerTicketMessageView {
                    message_id: message_id(&message.message_id)?,
                    author_ref: message.author_ref,
                    body: message.body,
                    created_at: timestamp(&message.created_at)?,
                })
            })
            .collect::<Result<_, UiError>>()?,
        internal_messages: None,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_internal_ticket(
    value: wire::SupportInternalTicketRecord,
) -> Result<SupportTicketView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode support internal ticket"));
    }
    let messages = value
        .messages
        .into_iter()
        .map(|message| {
            Ok(InternalTicketMessageView {
                message_id: message_id(&message.message_id)?,
                author_ref: message.author_ref,
                audience: decode_message_audience(message.audience)?,
                body: message.body,
                created_at: timestamp(&message.created_at)?,
            })
        })
        .collect::<Result<Vec<_>, UiError>>()?;
    let customer_messages = messages
        .iter()
        .filter(|message| message.audience == MessageAudience::CustomerVisible)
        .map(|message| CustomerTicketMessageView {
            message_id: message.message_id,
            author_ref: message.author_ref.clone(),
            body: message.body.clone(),
            created_at: message.created_at,
        })
        .collect();
    Ok(SupportTicketView {
        ticket_id: ticket_id(&value.ticket_id)?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        customer: customer_ref(&value.customer_id)?,
        customer_account: customer_account_ref(&value.customer_account_id)?,
        case_code: value.case_code,
        category: value.category,
        severity: value.severity,
        description: value.description,
        status: decode_ticket_status(value.status)?,
        customer_messages,
        internal_messages: Some(messages),
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_incident(value: wire::SupportIncidentRecord) -> Result<IncidentView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode support incident"));
    }
    Ok(IncidentView {
        incident_id: incident_id(&value.incident_id)?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        incident_code: value.incident_code,
        severity: value.severity,
        commander_ref: value.commander_ref,
        affected_services: value.affected_services,
        affected_customer_accounts: value
            .affected_customer_accounts
            .iter()
            .map(|value| customer_account_ref(value))
            .collect::<Result<_, _>>()?,
        status: decode_incident_status(value.status)?,
        timeline: value
            .timeline
            .into_iter()
            .map(|entry| {
                Ok(erpx_support::domain::IncidentTimelineEntry {
                    at: timestamp(&entry.created_at)?,
                    actor_ref: entry.author_ref,
                    message: entry.message,
                })
            })
            .collect::<Result<_, UiError>>()?,
        broadcasts: value
            .broadcasts
            .into_iter()
            .map(|entry| {
                Ok(erpx_support::domain::IncidentBroadcast {
                    at: timestamp(&entry.created_at)?,
                    actor_ref: entry.author_ref,
                    audience: decode_broadcast_audience(entry.audience)?,
                    message: entry.message,
                })
            })
            .collect::<Result<_, UiError>>()?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_session(value: wire::SupportSessionRecord) -> Result<SupportSessionView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode support session"));
    }
    Ok(SupportSessionView {
        session_id: session_id(&value.session_id)?,
        version: value.source_version,
        scope: decode_scope(value.approved_scope)?,
        ticket_id: ticket_id(&value.ticket_id)?,
        customer: customer_ref(&value.customer_id)?,
        customer_account: customer_account_ref(&value.customer_account_id)?,
        agent: principal_ref(&value.agent_principal_id)?,
        represented_principal: value
            .represented_principal_id
            .map(|value| principal_ref(&value))
            .transpose()?,
        mode: decode_mode(value.mode)?,
        permissions: value.permissions,
        module: value.module,
        reason: value.reason,
        status: decode_session_status(value.status)?,
        requested_at: timestamp(&value.requested_at)?,
        expires_at: timestamp(&value.expires_at)?,
        approved_by: value.approved_by,
        approved_at: value
            .approved_at
            .map(|value| timestamp(&value))
            .transpose()?,
        revoked_at: value
            .revoked_at
            .map(|value| timestamp(&value))
            .transpose()?,
        revoked_reason: value.revoked_reason,
    })
}

fn decode_capability(
    value: wire::SupportCapabilityRecord,
) -> Result<SupportCapabilityView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode support capability"));
    }
    let revocation = match (value.revocation_reason, value.revoked_at) {
        (None, None) => None,
        (Some(reason), Some(at)) => Some((decode_revocation(reason)?, timestamp(&at)?)),
        _ => return Err(invalid_response("decode support capability revocation")),
    };
    Ok(SupportCapabilityView {
        capability_id: capability_id(&value.capability_id)?,
        version: value.source_version,
        session_id: session_id(&value.session_id)?,
        ticket_id: ticket_id(&value.ticket_id)?,
        customer: customer_ref(&value.customer_id)?,
        customer_account: customer_account_ref(&value.customer_account_id)?,
        agent: principal_ref(&value.agent_principal_id)?,
        device_ref: value.device_ref,
        mode: decode_mode(value.mode)?,
        permissions: value.permissions,
        scope: decode_scope(value.approved_scope)?,
        module: value.module,
        issued_at: timestamp(&value.issued_at)?,
        expires_at: timestamp(&value.expires_at)?,
        revocation,
    })
}

fn ticket_id(value: &str) -> Result<SupportTicketId, UiError> {
    SupportTicketId::try_from_uuid(uuid(value, "decode support ticket identity")?)
        .map_err(|_| invalid_response("decode support ticket identity"))
}
fn message_id(value: &str) -> Result<MessageId, UiError> {
    MessageId::try_from_uuid(uuid(value, "decode support message identity")?)
        .map_err(|_| invalid_response("decode support message identity"))
}
fn incident_id(value: &str) -> Result<IncidentId, UiError> {
    IncidentId::try_from_uuid(uuid(value, "decode support incident identity")?)
        .map_err(|_| invalid_response("decode support incident identity"))
}
fn session_id(value: &str) -> Result<SessionId, UiError> {
    SessionId::try_from_uuid(uuid(value, "decode support session identity")?)
        .map_err(|_| invalid_response("decode support session identity"))
}
fn capability_id(value: &str) -> Result<CapabilityId, UiError> {
    CapabilityId::try_from_uuid(uuid(value, "decode support capability identity")?)
        .map_err(|_| invalid_response("decode support capability identity"))
}
fn customer_ref(value: &str) -> Result<customer::CustomerRef, UiError> {
    customer::CustomerRef::try_from_uuid(uuid(value, "decode support customer identity")?)
        .map_err(|_| invalid_response("decode support customer identity"))
}
fn customer_account_ref(value: &str) -> Result<customer::CustomerAccountRef, UiError> {
    customer::CustomerAccountRef::try_from_uuid(uuid(
        value,
        "decode support customer account identity",
    )?)
    .map_err(|_| invalid_response("decode support customer account identity"))
}
fn principal_ref(value: &str) -> Result<identity_access::PrincipalRef, UiError> {
    identity_access::PrincipalRef::try_from_uuid(uuid(value, "decode support principal identity")?)
        .map_err(|_| invalid_response("decode support principal identity"))
}

macro_rules! enum_codec {
    ($decode:ident, $encode:ident, $wire:ty, $domain:ty, {$($variant:ident),+ $(,)?}) => {
        fn $decode(value: i32) -> Result<$domain, UiError> {
            match <$wire>::try_from(value).ok() {
                $(Some(<$wire>::$variant) => Ok(<$domain>::$variant),)+
                _ => Err(invalid_response("decode support enum")),
            }
        }
        fn $encode(value: $domain) -> i32 {
            match value {
                $(<$domain>::$variant => <$wire>::$variant as i32,)+
            }
        }
    };
}

enum_codec!(decode_ticket_status, encode_ticket_status, wire::SupportTicketStatus, TicketStatus, {New, Triaged, WaitingForCustomer, AccessRequested, Approved, InProgress, Resolved, CustomerConfirmed, Closed});
enum_codec!(decode_incident_status, encode_incident_status, wire::SupportIncidentStatus, IncidentStatus, {Investigating, Identified, Monitoring, Resolved, PostmortemComplete});
enum_codec!(decode_mode, encode_mode, wire::SupportMode, SupportMode, {DiagnosticRead, HoldingMutation, Impersonation});
enum_codec!(decode_revocation, encode_revocation, wire::SupportCapabilityRevocationReason, CapabilityRevocationReason, {CustomerRequested, AgentDisabled, DeviceRevoked, SessionRevoked, TicketClosed, Administrative});
enum_codec!(decode_broadcast_audience, encode_broadcast_audience, wire::SupportBroadcastAudience, BroadcastAudience, {CustomerVisible, Internal});

fn decode_message_audience(value: i32) -> Result<MessageAudience, UiError> {
    match wire::SupportMessageAudience::try_from(value).ok() {
        Some(wire::SupportMessageAudience::CustomerVisible) => Ok(MessageAudience::CustomerVisible),
        Some(wire::SupportMessageAudience::Internal) => Ok(MessageAudience::Internal),
        Some(wire::SupportMessageAudience::Unspecified) | None => {
            Err(invalid_response("decode support message audience"))
        }
    }
}

fn decode_session_status(value: i32) -> Result<SessionStatus, UiError> {
    match wire::SupportSessionStatus::try_from(value).ok() {
        Some(wire::SupportSessionStatus::Requested) => Ok(SessionStatus::Requested),
        Some(wire::SupportSessionStatus::Approved) => Ok(SessionStatus::Approved),
        Some(wire::SupportSessionStatus::Revoked) => Ok(SessionStatus::Revoked),
        Some(wire::SupportSessionStatus::Unspecified) | None => {
            Err(invalid_response("decode support session status"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_lifecycle_enum_is_exhaustively_mapped() {
        for status in [
            TicketStatus::New,
            TicketStatus::Triaged,
            TicketStatus::WaitingForCustomer,
            TicketStatus::AccessRequested,
            TicketStatus::Approved,
            TicketStatus::InProgress,
            TicketStatus::Resolved,
            TicketStatus::CustomerConfirmed,
            TicketStatus::Closed,
        ] {
            assert_eq!(
                decode_ticket_status(encode_ticket_status(status)),
                Ok(status)
            );
        }
    }

    #[test]
    fn partial_capability_revocation_is_rejected() {
        let record = wire::SupportCapabilityRecord {
            source_version: 1,
            revocation_reason: Some(wire::SupportCapabilityRevocationReason::Administrative as i32),
            revoked_at: None,
            ..Default::default()
        };
        assert!(decode_capability(record).is_err());
    }
}
