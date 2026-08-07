//! Target-neutral Support UI models.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerMessageItem {
    pub message_id: String,
    pub author_ref: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalMessageItem {
    pub message_id: String,
    pub author_ref: String,
    pub audience: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerTicketItem {
    pub ticket_id: String,
    pub version: u64,
    pub scope: String,
    pub customer_id: String,
    pub customer_account_id: String,
    pub case_code: String,
    pub category: String,
    pub severity: String,
    pub description: String,
    pub status: String,
    pub messages: Vec<CustomerMessageItem>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalTicketItem {
    pub ticket_id: String,
    pub version: u64,
    pub scope: String,
    pub customer_id: String,
    pub customer_account_id: String,
    pub case_code: String,
    pub category: String,
    pub severity: String,
    pub description: String,
    pub status: String,
    pub messages: Vec<InternalMessageItem>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerTicketItems {
    pub tickets: Vec<CustomerTicketItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalTicketItems {
    pub tickets: Vec<InternalTicketItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentTimelineItem {
    pub author_ref: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentBroadcastItem {
    pub author_ref: String,
    pub audience: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentItem {
    pub incident_id: String,
    pub version: u64,
    pub scope: String,
    pub incident_code: String,
    pub severity: String,
    pub commander_ref: String,
    pub affected_services: Vec<String>,
    pub affected_customer_accounts: Vec<String>,
    pub status: String,
    pub timeline: Vec<IncidentTimelineItem>,
    pub broadcasts: Vec<IncidentBroadcastItem>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentItems {
    pub incidents: Vec<IncidentItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionItem {
    pub session_id: String,
    pub version: u64,
    pub scope: String,
    pub ticket_id: String,
    pub customer_id: String,
    pub customer_account_id: String,
    pub agent_principal_id: String,
    pub represented_principal_id: Option<String>,
    pub mode: String,
    pub permissions: Vec<String>,
    pub module: String,
    pub reason: String,
    pub status: String,
    pub requested_at: String,
    pub expires_at: String,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub revoked_at: Option<String>,
    pub revoked_reason: Option<String>,
}

/// Display-safe capability metadata. Sensitive issuance material has no UI
/// representation and therefore cannot cross into presenter or view state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityMetadataItem {
    pub capability_id: String,
    pub version: u64,
    pub session_id: String,
    pub ticket_id: String,
    pub customer_id: String,
    pub customer_account_id: String,
    pub agent_principal_id: String,
    pub device_ref: String,
    pub mode: String,
    pub permissions: Vec<String>,
    pub scope: String,
    pub module: String,
    pub issued_at: String,
    pub expires_at: String,
    pub revocation_reason: Option<String>,
    pub revoked_at: Option<String>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn capability_ui_model_is_metadata_only() {
        let source = include_str!("models.rs");
        assert!(!source.contains(concat!("secret", "_hash")));
        assert!(!source.contains(concat!("one_time", "_credential")));
        assert!(!source.contains(concat!("bearer", "_token")));
    }
}
