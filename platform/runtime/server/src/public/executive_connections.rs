//! Native Executive external-connection producer.
//!
//! Create, list and revoke are owned here; the one-time secret is minted and
//! hashed by Runtime, token/audit/rate-limit rows stay on Control, and current
//! assignment/organization/engagement authority is read from the selected
//! Holding write unit.
pub use crate::adapters::executive_connection_secret::{hash_secret, mint_secret};
pub use crate::adapters::executive_connections_http::{
    ExecutiveConnectionsHttpBackend, ExecutiveConnectionsHttpCommandRequest,
    ExecutiveConnectionsHttpFuture, ExecutiveConnectionsHttpReadRequest,
    ExecutiveConnectionsHttpTarget, ExecutiveMcpRateLimitBackend, executive_connections_router,
    executive_mcp_rate_limit_router,
};
pub use crate::adapters::executive_connections_postgres::{PgExecutiveConnections, consume_rate};
pub use crate::application::executive_connections::{
    ExecutiveConnectionsStore, PreparedConnection, StoreFuture, commit_create, connections,
    is_revoke, list, prepare_create, revoke, revoke_failed,
};
pub use crate::domain::executive_connections::{
    ConnectionActor, ConnectionScope, ConnectionStatus, CreateConnectionInput, CreatedConnection,
    EXECUTIVE_CONNECTION_AUDIT_ACTION, EXECUTIVE_CONNECTION_DAYS_MAX, EXECUTIVE_CONNECTION_LIMIT,
    EXECUTIVE_CONNECTION_LIST_MAX, EXECUTIVE_CONNECTION_NAME_MAX, EXECUTIVE_CONNECTION_SCOPE_MAX,
    EXECUTIVE_CONNECTION_SECRET_LENGTH, EXECUTIVE_CONNECTION_SECRET_PREFIX, ExternalConnection,
    ExternalConnections, GrantRow, NewConnection, TokenRow, access_changed, already_created,
    connection_forbidden, connection_limit, connection_not_found, expires_at, external,
    parse_create, parse_revoke, rate_limited, rate_window, scope_forbidden, scope_label,
    selected_scope, status, token_secret, unavailable, validate_command_key,
};
