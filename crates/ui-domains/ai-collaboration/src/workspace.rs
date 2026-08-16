use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_ui_core::Locale;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ConnectivityState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientChannel {
    Web,
    Desktop,
    Ios,
    Android,
    CommercePortal,
    CandidatePortal,
    SignatoryPortal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LauncherPresentation {
    DockedResizablePanel,
    FloatingActionBottomSheet,
    Hidden,
}
pub const fn launcher_presentation(channel: ClientChannel) -> LauncherPresentation {
    match channel {
        ClientChannel::Web | ClientChannel::Desktop => LauncherPresentation::DockedResizablePanel,
        ClientChannel::Ios | ClientChannel::Android => {
            LauncherPresentation::FloatingActionBottomSheet
        }
        ClientChannel::CommercePortal
        | ClientChannel::CandidatePortal
        | ClientChannel::SignatoryPortal => LauncherPresentation::Hidden,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceLauncherPreference {
    pub device_id: String,
    pub open: bool,
    pub panel_width: u16,
}
impl DeviceLauncherPreference {
    pub fn default_closed(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            open: false,
            panel_width: 420,
        }
    }
    pub fn resize(&mut self, width: u16) {
        self.panel_width = width.clamp(320, 720);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentDisplayState {
    Queued,
    Running,
    WaitingForInput,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyState {
    Private,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBinding {
    pub record: StableRecordRef,
    pub route: String,
    pub attached_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionCard {
    pub session_id: Uuid,
    pub agent_label: String,
    pub pinned_agent_version: u64,
    pub privacy: PrivacyState,
    pub context: Option<ContextBinding>,
    pub assignment_state: AssignmentDisplayState,
    pub waiting_prompt: Option<String>,
    pub can_publish: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherBadge {
    pub waiting: u32,
    pub completed: u32,
    pub failed: u32,
}
impl LauncherBadge {
    pub const fn total(self) -> u32 {
        self.waiting
            .saturating_add(self.completed)
            .saturating_add(self.failed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloatingLauncherState {
    pub preference: DeviceLauncherPreference,
    pub presentation: LauncherPresentation,
    pub badge: LauncherBadge,
    pub active_session: Option<AgentSessionCard>,
    pub context_change_pending: Option<ContextBinding>,
    pub connectivity: ConnectivityState,
}

impl FloatingLauncherState {
    pub fn new(
        device_id: impl Into<String>,
        channel: ClientChannel,
        connectivity: ConnectivityState,
    ) -> Self {
        Self {
            preference: DeviceLauncherPreference::default_closed(device_id),
            presentation: launcher_presentation(channel),
            badge: LauncherBadge {
                waiting: 0,
                completed: 0,
                failed: 0,
            },
            active_session: None,
            context_change_pending: None,
            connectivity,
        }
    }
    pub fn close(&mut self) {
        self.preference.open = false;
    }
    pub fn open(&mut self) -> bool {
        if self.presentation == LauncherPresentation::Hidden {
            return false;
        }
        self.preference.open = true;
        true
    }
    pub fn navigate(&mut self, next: ContextBinding) -> ContextBoundaryDecision {
        if self
            .active_session
            .as_ref()
            .and_then(|value| value.context.as_ref())
            .is_some_and(|current| current.record != next.record)
        {
            self.context_change_pending = Some(next);
            ContextBoundaryDecision::ConfirmationRequired
        } else {
            ContextBoundaryDecision::NoChange
        }
    }
    pub fn confirm_new_session_context(&mut self) -> Option<ContextBinding> {
        self.context_change_pending.take()
    }
    pub fn mutation_enabled(&self) -> bool {
        self.connectivity == ConnectivityState::Online
            && !matches!(self.presentation, LauncherPresentation::Hidden)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextBoundaryDecision {
    NoChange,
    ConfirmationRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSection {
    Inbox,
    Assignments,
    Sessions,
    Registry,
    Pools,
    Budgets,
    Provenance,
    Moderation,
}
impl WorkspaceSection {
    pub const ALL: [Self; 8] = [
        Self::Inbox,
        Self::Assignments,
        Self::Sessions,
        Self::Registry,
        Self::Pools,
        Self::Budgets,
        Self::Provenance,
        Self::Moderation,
    ];
    pub const fn label(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Inbox, Locale::En) => "Inbox",
            (Self::Inbox, Locale::Th) => "กล่องข้อความ",
            (Self::Assignments, Locale::En) => "Assignments",
            (Self::Assignments, Locale::Th) => "งานที่มอบหมาย",
            (Self::Sessions, Locale::En) => "Sessions",
            (Self::Sessions, Locale::Th) => "เซสชัน",
            (Self::Registry, Locale::En) => "Agent registry",
            (Self::Registry, Locale::Th) => "ทะเบียนเอเจนต์",
            (Self::Pools, Locale::En) => "Agent pools",
            (Self::Pools, Locale::Th) => "กลุ่มเอเจนต์",
            (Self::Budgets, Locale::En) => "Budgets",
            (Self::Budgets, Locale::Th) => "งบประมาณ",
            (Self::Provenance, Locale::En) => "Provenance",
            (Self::Provenance, Locale::Th) => "ที่มาและหลักฐาน",
            (Self::Moderation, Locale::En) => "Moderation",
            (Self::Moderation, Locale::Th) => "การกำกับเนื้อหา",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRegistryRow {
    pub agent_id: Uuid,
    pub label: String,
    pub version: u64,
    pub approved: bool,
    pub suspended: bool,
    pub capability_labels: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPoolRow {
    pub pool_id: Uuid,
    pub label: String,
    pub version: u64,
    pub eligible_agents: u32,
    pub routing_summary: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRow {
    pub layer: String,
    pub limit_minor: i64,
    pub used_minor: i64,
    pub currency: String,
    pub concurrency_limit: u32,
    pub running: u32,
    pub approval_required: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceRow {
    pub assignment_id: Uuid,
    pub initiating_principal_label: String,
    pub effective_agent_label: String,
    pub agent_version: u64,
    pub model_label: String,
    pub policy_digest: String,
    pub delegation_depth: u16,
    pub published_by_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiWorkspaceState {
    Loading,
    Empty {
        section: WorkspaceSection,
    },
    Content {
        section: WorkspaceSection,
        assignments: Vec<AgentSessionCard>,
        registry: Vec<AgentRegistryRow>,
        pools: Vec<AgentPoolRow>,
        budgets: Vec<BudgetRow>,
        provenance: Vec<ProvenanceRow>,
    },
    Denied,
    Restricted,
    Stale {
        section: WorkspaceSection,
        reason: String,
    },
    Degraded {
        section: WorkspaceSection,
        reason: String,
        retryable: bool,
    },
    Offline {
        section: WorkspaceSection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LauncherAccessibility {
    pub trigger_label: &'static str,
    pub dialog_role: &'static str,
    pub badge_live: &'static str,
    pub auto_open_on_update: bool,
    pub steals_focus: bool,
}
pub const LAUNCHER_ACCESSIBILITY: LauncherAccessibility = LauncherAccessibility {
    trigger_label: "Open AI assistants",
    dialog_role: "dialog",
    badge_live: "polite",
    auto_open_on_update: false,
    steals_focus: false,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishDecision {
    pub allowed: bool,
    pub author_label: String,
    pub approved_by_label: Option<String>,
}
pub fn publish_decision(session: &AgentSessionCard, approver: Option<&str>) -> PublishDecision {
    let allowed =
        session.privacy == PrivacyState::Private && session.can_publish && approver.is_some();
    PublishDecision {
        allowed,
        author_label: session.agent_label.clone(),
        approved_by_label: approver.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launcher_is_default_closed_hidden_in_portals_and_adaptive() {
        let web =
            FloatingLauncherState::new("device", ClientChannel::Web, ConnectivityState::Online);
        assert!(!web.preference.open);
        assert_eq!(web.presentation, LauncherPresentation::DockedResizablePanel);
        assert_eq!(
            launcher_presentation(ClientChannel::Ios),
            LauncherPresentation::FloatingActionBottomSheet
        );
        assert_eq!(
            launcher_presentation(ClientChannel::CommercePortal),
            LauncherPresentation::Hidden
        );
    }
    #[test]
    fn close_does_not_cancel_and_badges_do_not_open_or_steal_focus() {
        let mut state =
            FloatingLauncherState::new("device", ClientChannel::Desktop, ConnectivityState::Online);
        state.active_session = Some(AgentSessionCard {
            session_id: Uuid::from_u128(1),
            agent_label: "Planner".into(),
            pinned_agent_version: 2,
            privacy: PrivacyState::Private,
            context: None,
            assignment_state: AssignmentDisplayState::Running,
            waiting_prompt: None,
            can_publish: true,
        });
        state.preference.open = true;
        state.close();
        assert_eq!(
            state.active_session.as_ref().unwrap().assignment_state,
            AssignmentDisplayState::Running
        );
        let accessibility = std::hint::black_box(LAUNCHER_ACCESSIBILITY);
        assert!(!accessibility.auto_open_on_update && !accessibility.steals_focus);
    }
    #[test]
    fn navigation_never_silently_rebinds_active_session() {
        let mut state =
            FloatingLauncherState::new("device", ClientChannel::Web, ConnectivityState::Online);
        let binding = |id| ContextBinding {
            record: fixture_record(id),
            route: format!("/item/{id}"),
            attached_at: Utc::now(),
        };
        state.active_session = Some(AgentSessionCard {
            session_id: Uuid::from_u128(2),
            agent_label: "Agent".into(),
            pinned_agent_version: 1,
            privacy: PrivacyState::Private,
            context: Some(binding(1)),
            assignment_state: AssignmentDisplayState::Running,
            waiting_prompt: None,
            can_publish: false,
        });
        assert_eq!(
            state.navigate(binding(2)),
            ContextBoundaryDecision::ConfirmationRequired
        );
        assert_eq!(
            state.active_session.unwrap().context.unwrap().record,
            fixture_record(1)
        );
    }
    #[test]
    fn offline_disables_ai_invocation_and_publication_keeps_agent_authorship() {
        let state = FloatingLauncherState::new(
            "device",
            ClientChannel::Android,
            ConnectivityState::Offline,
        );
        assert!(!state.mutation_enabled());
        let session = AgentSessionCard {
            session_id: Uuid::from_u128(3),
            agent_label: "Finance Agent".into(),
            pinned_agent_version: 4,
            privacy: PrivacyState::Private,
            context: None,
            assignment_state: AssignmentDisplayState::Completed,
            waiting_prompt: None,
            can_publish: true,
        };
        let decision = publish_decision(&session, Some("Anan"));
        assert!(decision.allowed);
        assert_eq!(decision.author_label, "Finance Agent");
        assert_eq!(decision.approved_by_label.as_deref(), Some("Anan"));
    }
    fn fixture_record(value: u128) -> StableRecordRef {
        use erpx_integration_contracts::{catalog::ContextId, reference::RecordTypeId};
        use erpx_kernel::RecordId;
        let mut bytes = value.to_be_bytes();
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::from_bytes(bytes)).unwrap(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap()
    }
}
