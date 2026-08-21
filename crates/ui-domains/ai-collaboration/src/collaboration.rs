use chrono::{DateTime, Utc};
use erpx_integration_contracts::{RecordVersionRef, StableRecordRef};
use erpx_kernel::{HoldingId, PrincipalId};
use erpx_ui_core::Locale;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectivityState {
    Online,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeState {
    Connected,
    Reconnecting,
    Degraded,
    Unavailable,
}

impl RealtimeState {
    pub const fn announcement(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Connected, Locale::En) => "Live updates connected",
            (Self::Connected, Locale::Th) => "เชื่อมต่อการอัปเดตสดแล้ว",
            (Self::Reconnecting, Locale::En) => "Reconnecting live updates",
            (Self::Reconnecting, Locale::Th) => "กำลังเชื่อมต่อการอัปเดตสดใหม่",
            (Self::Degraded, Locale::En) => "Live updates are delayed",
            (Self::Degraded, Locale::Th) => "การอัปเดตสดล่าช้า",
            (Self::Unavailable, Locale::En) => "Live updates unavailable; refresh to retry",
            (Self::Unavailable, Locale::Th) => "การอัปเดตสดไม่พร้อมใช้งาน โปรดรีเฟรชเพื่อลองใหม่",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorFreshness {
    Current,
    Stale,
    Orphaned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorDisplay {
    pub record: StableRecordRef,
    pub source: Option<RecordVersionRef>,
    pub location_label: Option<String>,
    pub freshness: AnchorFreshness,
    pub freshness_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadRealm {
    Internal,
    ExternalPortal { portal_label: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Human,
    HumanGroup,
    Agent,
    AgentPool,
    Record,
    RecordVersion,
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenIdentity {
    Principal {
        principal_id: PrincipalId,
    },
    OpaqueGroup {
        group_id: Uuid,
        version: u64,
        recipient_snapshot: Vec<PrincipalId>,
    },
    Agent {
        principal_id: PrincipalId,
        definition_id: Uuid,
        version_id: Uuid,
    },
    OpaqueAgentPool {
        pool_id: Uuid,
        version: u64,
        selected_agent: PrincipalId,
    },
    Record {
        record: StableRecordRef,
    },
    RecordVersion {
        record: StableRecordRef,
        version: u64,
    },
    Field {
        record: StableRecordRef,
        stable_field_id: String,
        schema_version: u64,
    },
    Restricted {
        opaque_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedToken {
    pub token_id: Uuid,
    pub kind: TokenKind,
    pub identity: TokenIdentity,
    pub display_label_snapshot: String,
    pub display_label: String,
    pub restricted: bool,
}

impl TypedToken {
    pub fn restricted_record(opaque_id: Uuid) -> Self {
        Self {
            token_id: Uuid::new_v4(),
            kind: TokenKind::Record,
            identity: TokenIdentity::Restricted { opaque_id },
            display_label_snapshot: "Restricted item".into(),
            display_label: "Restricted item".into(),
            restricted: true,
        }
    }

    pub fn validate_redaction(&self) -> bool {
        !self.token_id.is_nil()
            && self.token_id.get_version_num() == 4
            && (!self.restricted
                || (self.display_label == "Restricted item"
                    && self.display_label_snapshot == "Restricted item"
                    && matches!(self.identity, TokenIdentity::Restricted { .. })))
    }

    /// Refreshes presentation after an authorized lookup without changing the
    /// immutable token identity or the persisted label snapshot.
    pub fn refresh_display_label(&mut self, value: impl Into<String>) -> bool {
        let value = value.into();
        if self.restricted
            || value.trim().is_empty()
            || value.len() > 512
            || value.chars().any(char::is_control)
        {
            return false;
        }
        self.display_label = value;
        true
    }

    pub fn clipboard_token(&self) -> Option<String> {
        let (kind, stable_id, qualifier) = match &self.identity {
            TokenIdentity::Principal { principal_id } => ("user", principal_id.to_string(), None),
            TokenIdentity::OpaqueGroup {
                group_id, version, ..
            } => ("group", group_id.to_string(), Some(format!("v{version}"))),
            TokenIdentity::Agent {
                definition_id,
                version_id,
                ..
            } => (
                "agent",
                definition_id.to_string(),
                Some(version_id.to_string()),
            ),
            TokenIdentity::OpaqueAgentPool {
                pool_id, version, ..
            } => (
                "agent-pool",
                pool_id.to_string(),
                Some(format!("v{version}")),
            ),
            TokenIdentity::Record { record } => ("record", record.id().to_string(), None),
            TokenIdentity::RecordVersion { record, version } => (
                "record-version",
                record.id().to_string(),
                Some(format!("v{version}")),
            ),
            TokenIdentity::Field {
                record,
                stable_field_id,
                schema_version,
            } => (
                "field",
                record.id().to_string(),
                Some(format!("{stable_field_id}@v{schema_version}")),
            ),
            TokenIdentity::Restricted { .. } => return None,
        };
        Some(match qualifier {
            Some(qualifier) => format!("@{kind}:{stable_id}:{qualifier}"),
            None => format!("@{kind}:{stable_id}"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastedTypedToken {
    pub kind: TokenKind,
    pub stable_id: Uuid,
    pub qualifier: Option<String>,
}

/// Parses only canonical token syntax. The result is intentionally unresolved:
/// callers must reauthorize and resolve it before inserting a `TypedToken`.
pub fn parse_pasted_typed_token(value: &str) -> Option<PastedTypedToken> {
    let value = value.strip_prefix('@')?;
    let mut parts = value.splitn(3, ':');
    let kind = match parts.next()? {
        "user" => TokenKind::Human,
        "group" => TokenKind::HumanGroup,
        "agent" => TokenKind::Agent,
        "agent-pool" => TokenKind::AgentPool,
        "record" => TokenKind::Record,
        "record-version" => TokenKind::RecordVersion,
        "field" => TokenKind::Field,
        _ => return None,
    };
    let stable_id = Uuid::parse_str(parts.next()?)
        .ok()
        .filter(|id| !id.is_nil())?;
    let qualifier = parts.next().map(str::to_owned);
    let qualifier_valid = match kind {
        TokenKind::Human | TokenKind::Record => qualifier.is_none(),
        TokenKind::HumanGroup | TokenKind::AgentPool | TokenKind::RecordVersion => qualifier
            .as_deref()
            .and_then(|value| value.strip_prefix('v'))
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|version| version > 0),
        TokenKind::Agent => qualifier
            .as_deref()
            .and_then(|value| Uuid::parse_str(value).ok())
            .is_some_and(|id| !id.is_nil()),
        TokenKind::Field => qualifier.as_deref().is_some_and(|value| {
            let Some((field_id, version)) = value.rsplit_once("@v") else {
                return false;
            };
            !field_id.is_empty()
                && field_id.len() <= 128
                && field_id.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
                && version.parse::<u64>().is_ok_and(|version| version > 0)
        }),
    };
    qualifier_valid.then_some(PastedTypedToken {
        kind,
        stable_id,
        qualifier,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "block")]
pub enum EditorBlock {
    Paragraph {
        text: String,
    },
    Heading {
        level: u8,
        text: String,
    },
    List {
        ordered: bool,
        items: Vec<String>,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Code {
        language: Option<String>,
        code: String,
    },
    Callout {
        tone: String,
        text: String,
    },
    Diagram {
        description: String,
        source: String,
    },
    SafeEmbed {
        label: String,
        url: String,
    },
    Attachment {
        attachment_id: Uuid,
        filename: String,
    },
    Token(TypedToken),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentEditorDocument {
    pub schema_version: u16,
    pub blocks: Vec<EditorBlock>,
    pub accessible_plain_text: String,
}

impl CommentEditorDocument {
    pub fn validate(&self) -> Result<(), EditorValidationError> {
        if self.schema_version == 0 || self.blocks.is_empty() {
            return Err(EditorValidationError::Empty);
        }
        if self.accessible_plain_text.trim().is_empty()
            || self.accessible_plain_text.len() > 100_000
        {
            return Err(EditorValidationError::MissingAccessibleText);
        }
        for block in &self.blocks {
            match block {
                EditorBlock::Heading { level, text }
                    if !(1..=6).contains(level) || text.trim().is_empty() =>
                {
                    return Err(EditorValidationError::InvalidBlock);
                }
                EditorBlock::Paragraph { text } | EditorBlock::Callout { text, .. }
                    if text.trim().is_empty() =>
                {
                    return Err(EditorValidationError::InvalidBlock);
                }
                EditorBlock::SafeEmbed { url, .. }
                    if !(url.starts_with("https://") || url.starts_with('/')) =>
                {
                    return Err(EditorValidationError::UnsafeEmbed);
                }
                EditorBlock::Token(token) if !token.validate_redaction() => {
                    return Err(EditorValidationError::RedactionLeak);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorValidationError {
    Empty,
    MissingAccessibleText,
    InvalidBlock,
    UnsafeEmbed,
    RedactionLeak,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionSummary {
    pub revision: u64,
    pub editor_label: String,
    pub edited_at: DateTime<Utc>,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: u32,
    pub reacted_by_current_user: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentVisibility {
    Visible,
    Tombstone,
    Quarantined,
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentCard {
    pub comment_id: Uuid,
    pub author_label: String,
    pub author_is_agent: bool,
    pub published_by_label: Option<String>,
    pub current_revision: u64,
    pub document: Option<CommentEditorDocument>,
    pub revisions: Vec<RevisionSummary>,
    pub reactions: Vec<ReactionSummary>,
    pub visibility: CommentVisibility,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ThreadCapabilities {
    pub comment: bool,
    pub edit_own: bool,
    pub resolve: bool,
    pub reopen: bool,
    pub react: bool,
    pub follow: bool,
    pub moderate: bool,
    pub promote: bool,
    pub propose_action: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadLifecycle {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadCard {
    pub thread_id: Uuid,
    /// Aggregate version required by every optimistic thread command.
    pub aggregate_version: u64,
    pub anchor: AnchorDisplay,
    pub realm: ThreadRealm,
    pub lifecycle: ThreadLifecycle,
    pub pinned: bool,
    pub following: bool,
    pub comments: Vec<CommentCard>,
    pub capabilities: ThreadCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSuggestionDisplayStatus {
    Queued,
    Running,
    Ready,
    Conflicted,
    Applied,
    Rejected,
    Superseded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestedFieldChangeCard {
    pub field_id: String,
    pub proposed_value: String,
    pub confidence_basis_points: u16,
    pub evidence_count: u16,
}

/// Authorized, presentation-safe subset of a typed field suggestion. Digests,
/// protected material references, and raw owner payloads never enter the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSuggestionCard {
    pub suggestion_id: Uuid,
    pub aggregate_version: u64,
    pub status: FieldSuggestionDisplayStatus,
    pub owner_label: String,
    pub provider_label: String,
    pub changes: Vec<SuggestedFieldChangeCard>,
    pub conflict_count: u16,
    pub reviewed_by_human: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestrictedThreadCard {
    pub neutral_label: String,
    pub reveal_type: bool,
    pub reveal_count: bool,
}
impl Default for RestrictedThreadCard {
    fn default() -> Self {
        Self {
            neutral_label: "Restricted item".into(),
            reveal_type: false,
            reveal_count: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityRailState {
    Loading,
    Empty,
    Content {
        threads: Vec<ThreadCard>,
        suggestions: Vec<FieldSuggestionCard>,
        realtime: RealtimeState,
    },
    Denied,
    Restricted(RestrictedThreadCard),
    Stale {
        threads: Vec<ThreadCard>,
        reason: String,
    },
    Degraded {
        threads: Vec<ThreadCard>,
        reason: String,
        retryable: bool,
    },
    Offline {
        cached_threads: Vec<ThreadCard>,
    },
}

/// Result of resolving the opaque record identifier for the current canonical
/// detail route. Only `Authorized` may be used to construct an activity query;
/// every other state intentionally carries no record descriptor or count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityReferenceGate {
    NotRecordRoute,
    Resolving,
    Authorized {
        holding_id: HoldingId,
        record: StableRecordRef,
        canonical_route: String,
    },
    /// Authorized by the audited control-plane collaboration gateway. This is
    /// deliberately distinct from holding-local authority.
    AuthorizedPlatform {
        record: StableRecordRef,
        canonical_route: String,
    },
    Neutral,
    Stale,
    Degraded,
    Offline,
}

impl ActivityReferenceGate {
    pub fn authorized_record(&self, current_route: &str) -> Option<&StableRecordRef> {
        match self {
            Self::Authorized {
                record,
                canonical_route,
                ..
            }
            | Self::AuthorizedPlatform {
                record,
                canonical_route,
                ..
            } if canonical_route == current_route => Some(record),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutocompleteState {
    Idle,
    Searching {
        query: String,
    },
    Results {
        query: String,
        candidates: Vec<TypedToken>,
    },
    Empty {
        query: String,
    },
    Denied,
    Offline,
    Failed {
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedActionProposal {
    pub proposal_id: Uuid,
    pub owner_context: String,
    pub command_type: String,
    pub summary: String,
    pub requires_confirmation: bool,
    pub confirmed: bool,
}

impl TypedActionProposal {
    pub fn validate(&self) -> bool {
        !self.owner_context.trim().is_empty()
            && self.owner_context != "collaboration"
            && !self.command_type.trim().is_empty()
            && self.requires_confirmation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationAvailability {
    pub enabled: bool,
    pub reason: Option<MutationBlockReason>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationBlockReason {
    Offline,
    Denied,
    Restricted,
    StaleAnchor,
    OrphanedAnchor,
    ExternalIsolation,
}

pub fn collaboration_mutation_availability(
    connectivity: ConnectivityState,
    authorized: bool,
    restricted: bool,
    anchor: AnchorFreshness,
    external_scope_matches: bool,
) -> MutationAvailability {
    let reason = if connectivity == ConnectivityState::Offline {
        Some(MutationBlockReason::Offline)
    } else if !authorized {
        Some(MutationBlockReason::Denied)
    } else if restricted {
        Some(MutationBlockReason::Restricted)
    } else if anchor == AnchorFreshness::Stale {
        Some(MutationBlockReason::StaleAnchor)
    } else if anchor == AnchorFreshness::Orphaned {
        Some(MutationBlockReason::OrphanedAnchor)
    } else if !external_scope_matches {
        Some(MutationBlockReason::ExternalIsolation)
    } else {
        None
    };
    MutationAvailability {
        enabled: reason.is_none(),
        reason,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibleBehavior {
    pub role: &'static str,
    pub live: &'static str,
    pub steals_focus: bool,
    pub motion: MotionBehavior,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionBehavior {
    Standard,
    Reduced,
}
pub const fn activity_accessibility(reduced_motion: bool) -> AccessibleBehavior {
    AccessibleBehavior {
        role: "region",
        live: "polite",
        steals_focus: false,
        motion: if reduced_motion {
            MotionBehavior::Reduced
        } else {
            MotionBehavior::Standard
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restricted_tokens_never_expose_type_title_or_counts() {
        let token = TypedToken::restricted_record(Uuid::from_u128(1));
        assert!(token.validate_redaction());
        assert_eq!(token.display_label, "Restricted item");
        let card = RestrictedThreadCard::default();
        assert!(!card.reveal_type && !card.reveal_count);
    }
    #[test]
    fn offline_and_stale_anchors_disable_every_mutation() {
        assert_eq!(
            collaboration_mutation_availability(
                ConnectivityState::Offline,
                true,
                false,
                AnchorFreshness::Current,
                true
            )
            .reason,
            Some(MutationBlockReason::Offline)
        );
        assert_eq!(
            collaboration_mutation_availability(
                ConnectivityState::Online,
                true,
                false,
                AnchorFreshness::Orphaned,
                true
            )
            .reason,
            Some(MutationBlockReason::OrphanedAnchor)
        );
    }
    #[test]
    fn full_block_editor_requires_accessible_text_and_safe_embeds() {
        let document = CommentEditorDocument {
            schema_version: 1,
            blocks: vec![EditorBlock::SafeEmbed {
                label: "Evidence".into(),
                url: "javascript:alert(1)".into(),
            }],
            accessible_plain_text: "Evidence".into(),
        };
        assert_eq!(document.validate(), Err(EditorValidationError::UnsafeEmbed));
    }

    #[test]
    fn typed_token_clipboard_is_unresolved_and_label_refresh_preserves_identity() {
        let principal_id = PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap();
        let mut token = TypedToken {
            token_id: Uuid::new_v4(),
            kind: TokenKind::Human,
            identity: TokenIdentity::Principal { principal_id },
            display_label_snapshot: "Old label".into(),
            display_label: "Old label".into(),
            restricted: false,
        };
        let copied = token.clipboard_token().unwrap();
        let pasted = parse_pasted_typed_token(&copied).unwrap();
        assert_eq!(pasted.kind, TokenKind::Human);
        assert_eq!(pasted.stable_id, principal_id.as_uuid());
        assert!(pasted.qualifier.is_none());

        let identity = token.identity.clone();
        assert!(token.refresh_display_label("Current label"));
        assert_eq!(token.identity, identity);
        assert_eq!(token.display_label_snapshot, "Old label");
        assert_eq!(token.display_label, "Current label");
        assert!(!token.refresh_display_label("\u{7}"));
    }

    #[test]
    fn paste_parser_rejects_ambiguous_or_unversioned_typed_targets() {
        let id = Uuid::new_v4();
        assert!(parse_pasted_typed_token(&format!("@record:{id}")).is_some());
        assert!(parse_pasted_typed_token(&format!("@record:{id}:v2")).is_none());
        assert!(parse_pasted_typed_token(&format!("@group:{id}")).is_none());
        assert!(parse_pasted_typed_token(&format!("@group:{id}:v2")).is_some());
        assert!(parse_pasted_typed_token("@user:not-a-uuid").is_none());
        assert!(
            parse_pasted_typed_token("@unknown:00000000-0000-4000-8000-000000000000").is_none()
        );
    }
    #[test]
    fn action_cards_are_typed_confirmed_owner_commands_only() {
        let proposal = TypedActionProposal {
            proposal_id: Uuid::from_u128(1),
            owner_context: "finance".into(),
            command_type: "approve_invoice".into(),
            summary: "Approve invoice".into(),
            requires_confirmation: true,
            confirmed: false,
        };
        assert!(proposal.validate());
    }

    #[test]
    fn activity_gate_never_derives_authority_from_a_url() {
        let id = Uuid::new_v4();
        let record = StableRecordRef::try_new(
            erpx_kernel::RecordId::try_from_uuid(id).unwrap(),
            erpx_integration_contracts::catalog::ContextId::Finance,
            erpx_integration_contracts::RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let canonical_route = format!("/finance/invoice/{id}");
        let authorized = ActivityReferenceGate::Authorized {
            holding_id: erpx_kernel::HoldingId::try_from_uuid(Uuid::new_v4()).unwrap(),
            record,
            canonical_route: canonical_route.clone(),
        };
        assert!(authorized.authorized_record(&canonical_route).is_some());
        assert!(authorized.authorized_record("/finance/invoice/9").is_none());
        for denied in [
            ActivityReferenceGate::Neutral,
            ActivityReferenceGate::Stale,
            ActivityReferenceGate::Degraded,
            ActivityReferenceGate::Offline,
        ] {
            assert!(denied.authorized_record("/finance/invoice/9").is_none());
        }
    }
}
