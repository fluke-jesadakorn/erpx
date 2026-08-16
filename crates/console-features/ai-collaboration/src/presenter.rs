use erpx_ui_ai_collaboration::{
    ActivityRailState, AiWorkspaceState, AutocompleteState, ConnectivityState,
    FloatingLauncherState, WorkspaceSection,
};

use crate::GatewayError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCollaborationPresenter {
    pub activity: ActivityRailState,
    pub autocomplete: AutocompleteState,
    pub workspace: AiWorkspaceState,
    pub launcher: FloatingLauncherState,
    pub submitting: bool,
    pub last_error: Option<GatewayError>,
}

impl AiCollaborationPresenter {
    pub fn new(launcher: FloatingLauncherState) -> Self {
        Self {
            activity: ActivityRailState::Loading,
            autocomplete: AutocompleteState::Idle,
            workspace: AiWorkspaceState::Loading,
            launcher,
            submitting: false,
            last_error: None,
        }
    }
    pub fn activity_loaded(&mut self, state: ActivityRailState) {
        self.activity = state;
        self.last_error = None;
    }
    pub fn workspace_loaded(&mut self, state: AiWorkspaceState) {
        self.workspace = state;
        self.last_error = None;
    }
    pub fn autocomplete_loaded(&mut self, state: AutocompleteState) {
        self.autocomplete = state;
        self.last_error = None;
    }
    pub fn submit_started(&mut self) -> bool {
        if !self.launcher.mutation_enabled() || self.submitting {
            return false;
        }
        self.submitting = true;
        self.last_error = None;
        true
    }
    pub fn submit_finished(&mut self) {
        self.submitting = false;
    }
    pub fn failed(&mut self, error: GatewayError) {
        self.submitting = false;
        self.last_error = Some(error.clone());
        match error {
            GatewayError::Offline => self.set_connectivity(ConnectivityState::Offline),
            GatewayError::Denied => self.workspace = AiWorkspaceState::Denied,
            GatewayError::Restricted => self.workspace = AiWorkspaceState::Restricted,
            GatewayError::Stale => {
                self.workspace = AiWorkspaceState::Stale {
                    section: WorkspaceSection::Inbox,
                    reason: "Projection is stale".into(),
                }
            }
            GatewayError::Unavailable | GatewayError::Conflict | GatewayError::Failed { .. } => {
                self.workspace = AiWorkspaceState::Degraded {
                    section: WorkspaceSection::Inbox,
                    reason: "Workspace is temporarily unavailable".into(),
                    retryable: true,
                }
            }
        }
    }
    pub fn set_connectivity(&mut self, connectivity: ConnectivityState) {
        self.launcher.connectivity = connectivity;
        if connectivity == ConnectivityState::Offline {
            self.activity = ActivityRailState::Offline {
                cached_threads: cached_threads(&self.activity),
            };
            let section = workspace_section(&self.workspace);
            self.workspace = AiWorkspaceState::Offline { section };
            self.autocomplete = AutocompleteState::Offline;
        }
    }
}

fn cached_threads(state: &ActivityRailState) -> Vec<erpx_ui_ai_collaboration::ThreadCard> {
    match state {
        ActivityRailState::Content { threads, .. }
        | ActivityRailState::Stale { threads, .. }
        | ActivityRailState::Degraded { threads, .. } => threads.clone(),
        ActivityRailState::Offline { cached_threads } => cached_threads.clone(),
        _ => Vec::new(),
    }
}
fn workspace_section(state: &AiWorkspaceState) -> WorkspaceSection {
    match state {
        AiWorkspaceState::Empty { section }
        | AiWorkspaceState::Content { section, .. }
        | AiWorkspaceState::Stale { section, .. }
        | AiWorkspaceState::Degraded { section, .. }
        | AiWorkspaceState::Offline { section } => *section,
        AiWorkspaceState::Loading | AiWorkspaceState::Denied | AiWorkspaceState::Restricted => {
            WorkspaceSection::Inbox
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_ai_collaboration::{ClientChannel, LauncherPresentation};
    #[test]
    fn offline_transition_retains_reads_but_disables_mutation() {
        let launcher =
            FloatingLauncherState::new("device", ClientChannel::Web, ConnectivityState::Online);
        let mut presenter = AiCollaborationPresenter::new(launcher);
        presenter.set_connectivity(ConnectivityState::Offline);
        assert!(matches!(
            presenter.activity,
            ActivityRailState::Offline { .. }
        ));
        assert!(!presenter.submit_started());
    }
    #[test]
    fn denied_restricted_stale_and_retry_states_remain_distinct() {
        let launcher =
            FloatingLauncherState::new("device", ClientChannel::Desktop, ConnectivityState::Online);
        let mut presenter = AiCollaborationPresenter::new(launcher);
        presenter.failed(GatewayError::Denied);
        assert_eq!(presenter.workspace, AiWorkspaceState::Denied);
        presenter.failed(GatewayError::Restricted);
        assert_eq!(presenter.workspace, AiWorkspaceState::Restricted);
        presenter.failed(GatewayError::Stale);
        assert!(matches!(
            presenter.workspace,
            AiWorkspaceState::Stale { .. }
        ));
        presenter.failed(GatewayError::Unavailable);
        assert!(matches!(
            presenter.workspace,
            AiWorkspaceState::Degraded {
                retryable: true,
                ..
            }
        ));
        assert_eq!(
            presenter.launcher.presentation,
            LauncherPresentation::DockedResizablePanel
        );
    }
}
