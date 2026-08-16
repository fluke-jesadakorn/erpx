use chrono::{DateTime, Utc};
use erpx_integration_contracts::{RecordVersionRef, StableRecordRef};
use erpx_kernel::{PrincipalId, TenantId};
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenIdentity {
    Principal { principal_id: PrincipalId },
    OpaqueGroup { group_id: Uuid, version: u64 },
    OpaqueAgentPool { pool_id: Uuid, version: u64 },
    Record { record: StableRecordRef },
    Restricted { opaque_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedToken {
    pub kind: TokenKind,
    pub identity: TokenIdentity,
    pub display_label: String,
    pub restricted: bool,
}

impl TypedToken {
    pub fn restricted_record(opaque_id: Uuid) -> Self {
        Self {
            kind: TokenKind::Record,
            identity: TokenIdentity::Restricted { opaque_id },
            display_label: "Restricted item".into(),
            restricted: true,
        }
    }

    pub fn validate_redaction(&self) -> bool {
        !self.restricted
            || (self.display_label == "Restricted item"
                && matches!(self.identity, TokenIdentity::Restricted { .. }))
    }
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
    pub anchor: AnchorDisplay,
    pub realm: ThreadRealm,
    pub lifecycle: ThreadLifecycle,
    pub pinned: bool,
    pub following: bool,
    pub comments: Vec<CommentCard>,
    pub capabilities: ThreadCapabilities,
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
        tenant_id: TenantId,
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
            tenant_id: erpx_kernel::TenantId::try_from_uuid(Uuid::new_v4()).unwrap(),
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
