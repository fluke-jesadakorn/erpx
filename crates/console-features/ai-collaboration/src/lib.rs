#![doc = "Reusable Dioxus Collaboration and AI workspace surfaces."]
#![forbid(unsafe_code)]

pub mod gateway;
pub mod presenter;
pub mod view;

pub use erpx_ui_ai_collaboration::{
    ActivityRailState, ActivityReferenceGate, AiWorkspaceState, AutocompleteState, ClientChannel,
    ConnectivityState, FloatingLauncherState, WorkspaceSection,
};
pub use gateway::*;
pub use presenter::*;
pub use view::*;

use dioxus::prelude::*;
use erpx_ui_core::{FeatureContext, FeatureRegistration, FeatureRouteDescriptor};

const FEATURE_ROUTES: &[FeatureRouteDescriptor] = &[FeatureRouteDescriptor {
    id: "ai.workspace",
    canonical_path: "/ai",
    authorization: "authenticated",
    navigation_group: "ai-collaboration",
}];

#[component]
pub fn AiCollaborationFeatureSurface(context: FeatureContext) -> Element {
    let state = if !context.session.authenticated {
        AiWorkspaceState::Denied
    } else if context.active_scope.is_none() {
        AiWorkspaceState::Restricted
    } else {
        AiWorkspaceState::Degraded {
            section: WorkspaceSection::Inbox,
            reason: match context.locale {
                erpx_ui_core::Locale::En => {
                    "The authorized AI workspace transport is not connected yet."
                }
                erpx_ui_core::Locale::Th => "การเชื่อมต่อพื้นที่ทำงาน AI ที่ได้รับอนุญาตยังไม่พร้อม",
            }
            .to_owned(),
            retryable: true,
        }
    };
    rsx! { AiWorkspaceView { state, locale: context.locale } }
}

fn render_feature_surface(context: &FeatureContext) -> Element {
    rsx! { AiCollaborationFeatureSurface { context: context.clone() } }
}

pub static REGISTRATION: FeatureRegistration = FeatureRegistration::with_renderer_and_artifact(
    "ai-collaboration",
    FEATURE_ROUTES,
    "erpx-feature-ai-collaboration",
    render_feature_surface,
);

#[cfg(test)]
mod registration_tests {
    use super::*;

    #[test]
    fn ai_workspace_registration_uses_the_real_workspace_surface() {
        assert_eq!(REGISTRATION.routes[0].canonical_path, "/ai");
        let source = include_str!("lib.rs");
        assert!(source.contains("AiWorkspaceView"));
        assert!(source.contains("context.active_scope.is_none()"));
        assert!(!source.contains(concat!("/work", "spaces")));
    }
}
