# Support Phase A handoff

## Shared registrations

- Workspace member: `crates/contexts/support` (`erpx-support`).
- No foreign context-core dependency. Core dependencies are `erpx-kernel`,
  `erpx-cqrs`, and `erpx-integration-contracts` only.
- Frozen projection names: `support.tickets`, `support.incidents`,
  `support.sessions`, and `support.capabilities`.
- Frozen write aggregate types: `support_ticket`, `support_incident`,
  `support_session`, and `support_capability`.

## Integration-contract delta

The existing `support::SupportCaseFact` and `support::Event::SupportCaseChanged`
are sufficient for tickets. Add the following owner-qualified incident fact to
`crates/integration-contracts/src/support.rs`; no session, bearer, message,
timeline, approval, or capability data may be published.

```rust
owned_uuid_reference!(SupportIncidentRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportIncidentStatus {
    Investigating,
    Identified,
    Monitoring,
    Resolved,
    PostmortemComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportIncidentFact {
    pub incident: SupportIncidentRef,
    pub incident_code: ReferenceCode,
    pub status: SupportIncidentStatus,
    pub severity: ReferenceCode,
    pub affected_services: Vec<ReferenceCode>,
    pub affected_customer_accounts: Vec<crate::customer::CustomerAccountRef>,
    pub updated_at: DateTime<Utc>,
}
```

Append `IncidentChanged(SupportIncidentFact)` to `support::Event` and map it to
event type `support.incident_changed`. Update Support integration mapping after
the contract freezes. No Identity/Commercial integration-event delta is
required: current Identity session/device/MFA/SoD state must be checked
synchronously by the Support authorization adapter, and Commercial entitlement
must be checked by the entitlement adapter. Neither check may use a stale
Support snapshot or import a foreign core.

## Protobuf delta (`service.proto`, target package `erpx.support.v1`)

Keep the legacy `SupportService` only as a temporary cutover seam. Append typed
`SupportCommandService` and `SupportQueryService`; remove the legacy service and
messages at final coordinated cutover. All command requests start with
`erpx.common.v1.CommandRequestMetadata metadata = 1`; all query requests start
with `erpx.common.v1.QueryRequestMetadata metadata = 1`. Mutation responses have
`erpx.common.v1.CommandReceipt receipt = 1`; query responses end with
`erpx.common.v1.QueryMetadata metadata`.

Enums (use the exact domain variants plus `UNSPECIFIED = 0`):

- `SupportTicketStatus`: NEW, TRIAGED, WAITING_FOR_CUSTOMER, ACCESS_REQUESTED,
  APPROVED, IN_PROGRESS, RESOLVED, CUSTOMER_CONFIRMED, CLOSED.
- `SupportMessageAudience`: CUSTOMER_VISIBLE, INTERNAL.
- `SupportMode`: DIAGNOSTIC_READ, TENANT_MUTATION, IMPERSONATION.
- `SupportSessionStatus`: REQUESTED, APPROVED, REVOKED.
- `SupportCapabilityRevocationReason`: CUSTOMER_REQUESTED, AGENT_DISABLED,
  DEVICE_REVOKED, SESSION_REVOKED, TICKET_CLOSED, ADMINISTRATIVE.
- `SupportIncidentStatus`: INVESTIGATING, IDENTIFIED, MONITORING, RESOLVED,
  POSTMORTEM_COMPLETE.
- `SupportBroadcastAudience`: CUSTOMER_VISIBLE, INTERNAL.

Record field numbers:

```proto
message SupportCustomerMessageRecord {
  string message_id = 1; string author_ref = 2; string body = 3; string created_at = 4;
}
message SupportInternalMessageRecord {
  string message_id = 1; string author_ref = 2; SupportMessageAudience audience = 3;
  string body = 4; string created_at = 5;
}
message SupportCustomerTicketRecord {
  string ticket_id = 1; uint64 source_version = 2; erpx.common.v1.TenantScope scope = 3;
  string customer_id = 4; string customer_account_id = 5; string case_code = 6;
  string category = 7; string severity = 8; string description = 9;
  SupportTicketStatus status = 10; repeated SupportCustomerMessageRecord messages = 11;
  string created_at = 12; string updated_at = 13; string projected_at = 14;
}
message SupportInternalTicketRecord {
  string ticket_id = 1; uint64 source_version = 2; erpx.common.v1.TenantScope scope = 3;
  string customer_id = 4; string customer_account_id = 5; string case_code = 6;
  string category = 7; string severity = 8; string description = 9;
  SupportTicketStatus status = 10; repeated SupportInternalMessageRecord messages = 11;
  string created_at = 12; string updated_at = 13; string projected_at = 14;
}
message SupportIncidentRecord {
  string incident_id = 1; uint64 source_version = 2; erpx.common.v1.TenantScope scope = 3;
  string incident_code = 4; string severity = 5; string commander_ref = 6;
  repeated string affected_services = 7; repeated string affected_customer_accounts = 8;
  SupportIncidentStatus status = 9; string created_at = 10; string updated_at = 11;
  string projected_at = 12;
}
message SupportSessionRecord {
  string session_id = 1; uint64 source_version = 2; erpx.common.v1.TenantScope approved_scope = 3;
  string ticket_id = 4; string customer_id = 5; string customer_account_id = 6;
  string agent_principal_id = 7; optional string represented_principal_id = 8;
  SupportMode mode = 9; repeated string permissions = 10; string module = 11;
  string reason = 12; SupportSessionStatus status = 13; string requested_at = 14;
  string expires_at = 15; optional string approved_by = 16; optional string approved_at = 17;
  optional string revoked_at = 18; optional string revoked_reason = 19; string projected_at = 20;
}
message SupportCapabilityRecord {
  string capability_id = 1; uint64 source_version = 2; string session_id = 3;
  string ticket_id = 4; string customer_id = 5; string customer_account_id = 6;
  string agent_principal_id = 7; string device_ref = 8; SupportMode mode = 9;
  repeated string permissions = 10; erpx.common.v1.TenantScope approved_scope = 11;
  string module = 12; string issued_at = 13; string expires_at = 14;
  optional SupportCapabilityRevocationReason revocation_reason = 15;
  optional string revoked_at = 16; string projected_at = 17;
}
```

`SupportSessionRecord` and `SupportCapabilityRecord` deliberately have no
`capability_secret`, secret hash bytes, bearer, or credential field. The typed
transport does not return credential material. Hashing and any out-of-band
delivery belong exclusively to the secure Server/agent boundary; none of that
material is persisted in the receipt, snapshot, audit, outbox, or read model.

Typed command RPCs and payload fields after metadata:

- `OpenTicket`: ticket_id=2, customer_id=3, customer_account_id=4, case_code=5,
  category=6, severity=7, description=8 -> internal-ticket mutation.
- `TransitionTicket`: ticket_id=2, next=3 -> internal-ticket mutation.
- `AddCustomerReply`: ticket_id=2, message_id=3, body=4 -> customer-ticket mutation.
- `AddInternalNote`: ticket_id=2, message_id=3, body=4 -> internal-ticket mutation.
- `RequestSession`: session_id=2, ticket_id=3, agent_principal_id=4,
  optional represented_principal_id=5, mode=6, permissions=7,
  approved_scope=8, module=9, reason=10, duration_seconds=11 -> session mutation.
- `ApproveSession`: session_id=2 -> session mutation.
- `RevokeSession`: session_id=2, reason=3 -> session mutation.
- `IssueCapability`: capability_id=2, session_id=3, device_ref=4. Do not accept
  client-provided MFA/SOD evidence or hash on the wire; the Server adapter
  resolves current Identity evidence and creates the hash-only core command.
- `RevokeCapability`: capability_id=2, reason=3 -> capability mutation.
- `OpenIncident`: incident_id=2, incident_code=3, severity=4,
  affected_services=5, affected_customer_account_ids=6 -> incident mutation.
- `TransitionIncident`: incident_id=2, next=3 -> incident mutation.
- `AddIncidentTimeline`: incident_id=2, message=3 -> incident mutation.
- `BroadcastIncident`: incident_id=2, audience=3, message=4 -> incident mutation.

Use separate customer/internal query RPCs so a customer response schema cannot
encode an internal note:

- `GetCustomerTicket`, `ListCustomerTickets` -> customer record only.
- `GetInternalTicket`, `ListInternalTickets` -> internal record only.
- `GetIncident`, `ListIncidents`, `GetSession`, `GetCapability`.
- List requests use page_size then optional cursor; ticket lists additionally
  accept optional status/customer_id; incident lists accept optional status.

## Exact write DDL

Allocate the next tenant-write migration number and apply this schema.

```sql
CREATE SCHEMA IF NOT EXISTS erpx_support_write;
CREATE TABLE erpx_support_write.ticket_guards (
  ticket_id UUID PRIMARY KEY, scope_key TEXT NOT NULL, customer_id UUID NOT NULL,
  customer_account_id UUID NOT NULL, case_code TEXT NOT NULL, category TEXT NOT NULL,
  severity TEXT NOT NULL, description TEXT NOT NULL,
  ticket_status TEXT NOT NULL CHECK (ticket_status IN ('new','triaged','waiting_for_customer','access_requested','approved','in_progress','resolved','customer_confirmed','closed')),
  aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
  created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
  CHECK (updated_at >= created_at), UNIQUE (scope_key, case_code)
);
CREATE TABLE erpx_support_write.ticket_message_guards (
  ticket_id UUID NOT NULL, message_id UUID NOT NULL, author_ref TEXT NOT NULL,
  audience TEXT NOT NULL CHECK (audience IN ('customer_visible','internal')),
  body TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (ticket_id, message_id), FOREIGN KEY (ticket_id)
    REFERENCES erpx_support_write.ticket_guards(ticket_id) ON DELETE CASCADE
);
CREATE TABLE erpx_support_write.incident_guards (
  incident_id UUID PRIMARY KEY, scope_key TEXT NOT NULL, incident_code TEXT NOT NULL,
  severity TEXT NOT NULL, commander_ref TEXT NOT NULL, affected_services JSONB NOT NULL,
  affected_customer_accounts JSONB NOT NULL, timeline JSONB NOT NULL, broadcasts JSONB NOT NULL,
  incident_status TEXT NOT NULL CHECK (incident_status IN ('investigating','identified','monitoring','resolved','postmortem_complete')),
  aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
  created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
  CHECK (jsonb_typeof(affected_services)='array' AND jsonb_array_length(affected_services)>0),
  CHECK (jsonb_typeof(affected_customer_accounts)='array'), CHECK (jsonb_typeof(timeline)='array'),
  CHECK (jsonb_typeof(broadcasts)='array'), CHECK (updated_at >= created_at),
  UNIQUE (scope_key, incident_code)
);
CREATE TABLE erpx_support_write.session_guards (
  session_id UUID PRIMARY KEY, scope_key TEXT NOT NULL, ticket_id UUID NOT NULL,
  customer_id UUID NOT NULL, customer_account_id UUID NOT NULL, agent_principal_id UUID NOT NULL,
  represented_principal_id UUID, support_mode TEXT NOT NULL CHECK (support_mode IN ('diagnostic_read','tenant_mutation','impersonation')),
  permissions JSONB NOT NULL, module TEXT NOT NULL, reason TEXT NOT NULL,
  session_status TEXT NOT NULL CHECK (session_status IN ('requested','approved','revoked')),
  aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), requested_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL, approved_by TEXT, approved_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ, revoked_reason TEXT,
  CHECK (jsonb_typeof(permissions)='array' AND jsonb_array_length(permissions)>0),
  CHECK (expires_at > requested_at),
  CHECK ((support_mode='impersonation') = (represented_principal_id IS NOT NULL)),
  CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
  CHECK (approved_by IS NULL OR approved_by <> agent_principal_id::TEXT),
  CHECK (session_status<>'approved' OR approved_at IS NOT NULL),
  CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL)),
  CHECK (session_status<>'revoked' OR revoked_at IS NOT NULL),
  FOREIGN KEY (ticket_id) REFERENCES erpx_support_write.ticket_guards(ticket_id)
);
CREATE TABLE erpx_support_write.capability_guards (
  capability_id UUID PRIMARY KEY, session_id UUID NOT NULL, ticket_id UUID NOT NULL,
  customer_id UUID NOT NULL, customer_account_id UUID NOT NULL, agent_principal_id UUID NOT NULL,
  device_ref TEXT NOT NULL, support_mode TEXT NOT NULL CHECK (support_mode IN ('diagnostic_read','tenant_mutation','impersonation')),
  permissions JSONB NOT NULL, approved_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
  module TEXT NOT NULL, secret_hash_sha256 BYTEA NOT NULL CHECK (octet_length(secret_hash_sha256)=32),
  aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
  issued_at TIMESTAMPTZ NOT NULL, expires_at TIMESTAMPTZ NOT NULL,
  revocation_reason TEXT CHECK (revocation_reason IN ('customer_requested','agent_disabled','device_revoked','session_revoked','ticket_closed','administrative')),
  revoked_at TIMESTAMPTZ, CHECK (expires_at > issued_at),
  CHECK (jsonb_typeof(permissions)='array' AND jsonb_array_length(permissions)>0),
  CHECK (jsonb_typeof(approved_scope)='object'),
  CHECK ((revocation_reason IS NULL) = (revoked_at IS NULL)),
  FOREIGN KEY (session_id) REFERENCES erpx_support_write.session_guards(session_id),
  FOREIGN KEY (ticket_id) REFERENCES erpx_support_write.ticket_guards(ticket_id)
);
CREATE INDEX support_ticket_scope_customer_status ON erpx_support_write.ticket_guards(scope_key,customer_id,ticket_status,updated_at DESC,ticket_id DESC);
CREATE INDEX support_incident_scope_status ON erpx_support_write.incident_guards(scope_key,incident_status,updated_at DESC,incident_id DESC);
CREATE INDEX support_session_scope_ticket_status ON erpx_support_write.session_guards(scope_key,ticket_id,session_status,expires_at DESC,session_id DESC);
CREATE INDEX support_capability_scope_session_expiry ON erpx_support_write.capability_guards(scope_key,session_id,expires_at DESC,capability_id DESC);
```

## Exact disposable read DDL

Allocate the next tenant-read migration number. Every table below uses the
standard FK `(projection_name,generation) ->
erpx_cqrs_read.projection_generations` with `ON DELETE CASCADE`.

```sql
CREATE SCHEMA IF NOT EXISTS erpx_support_read;
CREATE TABLE erpx_support_read.ticket_projection (
  projection_name TEXT NOT NULL DEFAULT 'support.tickets' CHECK (projection_name='support.tickets'),
  generation UUID NOT NULL, partition_key TEXT NOT NULL, ticket_id UUID NOT NULL,
  source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0), source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
  tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL, customer_id UUID NOT NULL,
  customer_account_id UUID NOT NULL, case_code TEXT NOT NULL, category TEXT NOT NULL,
  severity TEXT NOT NULL, description TEXT NOT NULL, ticket_status TEXT NOT NULL,
  aggregate_created_at TIMESTAMPTZ NOT NULL, aggregate_updated_at TIMESTAMPTZ NOT NULL,
  projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(projection_name,generation,partition_key,ticket_id),
  FOREIGN KEY(projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(projection_name,generation) ON DELETE CASCADE,
  UNIQUE(generation,scope_key,case_code)
);
CREATE TABLE erpx_support_read.ticket_customer_message_projection (
  projection_name TEXT NOT NULL DEFAULT 'support.tickets' CHECK (projection_name='support.tickets'),
  generation UUID NOT NULL, partition_key TEXT NOT NULL, ticket_id UUID NOT NULL, message_id UUID NOT NULL,
  author_ref TEXT NOT NULL, body TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY(projection_name,generation,partition_key,ticket_id,message_id),
  FOREIGN KEY(projection_name,generation,partition_key,ticket_id) REFERENCES erpx_support_read.ticket_projection(projection_name,generation,partition_key,ticket_id) ON DELETE CASCADE
);
CREATE TABLE erpx_support_read.ticket_internal_message_projection (
  projection_name TEXT NOT NULL DEFAULT 'support.tickets' CHECK (projection_name='support.tickets'),
  generation UUID NOT NULL, partition_key TEXT NOT NULL, ticket_id UUID NOT NULL, message_id UUID NOT NULL,
  author_ref TEXT NOT NULL, audience TEXT NOT NULL CHECK(audience IN ('customer_visible','internal')),
  body TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY(projection_name,generation,partition_key,ticket_id,message_id),
  FOREIGN KEY(projection_name,generation,partition_key,ticket_id) REFERENCES erpx_support_read.ticket_projection(projection_name,generation,partition_key,ticket_id) ON DELETE CASCADE
);
CREATE TABLE erpx_support_read.incident_projection (
  projection_name TEXT NOT NULL DEFAULT 'support.incidents' CHECK (projection_name='support.incidents'),
  generation UUID NOT NULL, partition_key TEXT NOT NULL, incident_id UUID NOT NULL,
  source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0), source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0),
  tenant_scope JSONB NOT NULL, scope_key TEXT NOT NULL, incident_code TEXT NOT NULL,
  severity TEXT NOT NULL, commander_ref TEXT NOT NULL, affected_services JSONB NOT NULL,
  affected_customer_accounts JSONB NOT NULL, incident_status TEXT NOT NULL,
  state JSONB NOT NULL, aggregate_created_at TIMESTAMPTZ NOT NULL, aggregate_updated_at TIMESTAMPTZ NOT NULL,
  projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(projection_name,generation,partition_key,incident_id),
  FOREIGN KEY(projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(projection_name,generation) ON DELETE CASCADE,
  UNIQUE(generation,scope_key,incident_code)
);
CREATE TABLE erpx_support_read.session_projection (
  projection_name TEXT NOT NULL DEFAULT 'support.sessions' CHECK (projection_name='support.sessions'),
  generation UUID NOT NULL, partition_key TEXT NOT NULL, session_id UUID NOT NULL,
  source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0), source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0),
  approved_scope JSONB NOT NULL, scope_key TEXT NOT NULL, ticket_id UUID NOT NULL, customer_id UUID NOT NULL,
  customer_account_id UUID NOT NULL, agent_principal_id UUID NOT NULL, represented_principal_id UUID,
  support_mode TEXT NOT NULL, permissions JSONB NOT NULL, module TEXT NOT NULL, reason TEXT NOT NULL,
  session_status TEXT NOT NULL, requested_at TIMESTAMPTZ NOT NULL, expires_at TIMESTAMPTZ NOT NULL,
  approved_by TEXT, approved_at TIMESTAMPTZ, revoked_at TIMESTAMPTZ, revoked_reason TEXT,
  projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(projection_name,generation,partition_key,session_id),
  FOREIGN KEY(projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_support_read.capability_projection (
  projection_name TEXT NOT NULL DEFAULT 'support.capabilities' CHECK (projection_name='support.capabilities'),
  generation UUID NOT NULL, partition_key TEXT NOT NULL, capability_id UUID NOT NULL,
  source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0), source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0),
  session_id UUID NOT NULL, ticket_id UUID NOT NULL, customer_id UUID NOT NULL, customer_account_id UUID NOT NULL,
  agent_principal_id UUID NOT NULL, device_ref TEXT NOT NULL, support_mode TEXT NOT NULL,
  permissions JSONB NOT NULL, approved_scope JSONB NOT NULL, scope_key TEXT NOT NULL, module TEXT NOT NULL,
  issued_at TIMESTAMPTZ NOT NULL, expires_at TIMESTAMPTZ NOT NULL, revocation_reason TEXT, revoked_at TIMESTAMPTZ,
  projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(projection_name,generation,partition_key,capability_id),
  FOREIGN KEY(projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX support_read_ticket_scope_customer_status ON erpx_support_read.ticket_projection(generation,scope_key,customer_id,ticket_status,aggregate_updated_at DESC,ticket_id DESC);
CREATE INDEX support_read_incident_scope_status ON erpx_support_read.incident_projection(generation,scope_key,incident_status,aggregate_updated_at DESC,incident_id DESC);
CREATE INDEX support_read_session_scope_ticket_status ON erpx_support_read.session_projection(generation,scope_key,ticket_id,session_status,expires_at DESC,session_id DESC);
CREATE INDEX support_read_capability_scope_session_expiry ON erpx_support_read.capability_projection(generation,scope_key,session_id,expires_at DESC,capability_id DESC);
```

The capability read table deliberately has no `secret_hash_sha256` and no raw
credential column. Customer ticket queries must never join the internal message
table.

## Legacy removal map (after Phase B acceptance only)

- `crates/domain/src/support.rs`: ticket/session/capability/incident ownership
  moves to `erpx-support`; keep diagnostic redaction only if it is reassigned to
  Platform Administration, otherwise move it into a Support-specific adapter.
- `crates/application/src/legacy/support.rs`: replace `SupportStore`,
  `SupportService`, `AuthoritativeSupportService`, and ticket/session command
  orchestration with typed handlers from this core.
- `crates/application/src/support_workflow.rs`: replace
  `SupportWorkflowPort/Service` and grant/revoke DTOs with Support command
  handler plus SQLx unit of work.
- `crates/infrastructure/src/support_persistence.rs`: replace with
  `crates/adapters/support-sqlx` write/read repositories and UoW.
- `crates/infrastructure/src/support_workflow.rs`: replace with capability SQLx
  adapter plus current Identity authorization adapter; do not retain a second
  authority path.
- `crates/infrastructure/src/support_diagnostics.rs`: retain only as an inbound
  diagnostics provider behind Support authorization; remove aggregate/store
  construction from transport.
- `crates/infrastructure/tests/support_persistence.rs` and
  `support_workflow.rs`: port behaviors into adapter acceptance tests.
- `crates/proto/.../support.proto`: remove generic legacy `SupportService` and
  every legacy DTO after all three applications cut over; in particular remove
  `SupportSession.capability_secret`.
- Server/Worker/Console legacy Support modules and `crates/ui/src/support*.rs`
  are removed only after typed gRPC, projectors, gateways/presenters/views, and
  all-target tests pass.

The Phase B barrier is frozen at descriptor
`34da323cdfa1c092601a7494cdc865be94e1cb6c707f3c4a8b53b784f7bacd22`, tenant
write migration `0062`, and tenant read migration `0054`. Phase B proceeds
against those contracts without changing shared registrations.
