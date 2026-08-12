//! Console shell boundary: registry, navigation dispatch, and target-neutral shell context.
//!
//! This crate intentionally has no protobuf, Tonic, or domain-context dependencies.

use dioxus::prelude::Element;
use erpx_ui_core::{FeatureContext, FeatureRegistration, FeatureRegistry, FeatureRegistryError};

#[derive(Debug, Clone)]
pub struct ConsoleShell {
    registry: FeatureRegistry,
}

impl ConsoleShell {
    pub fn try_new(
        registrations: impl IntoIterator<Item = &'static FeatureRegistration>,
    ) -> Result<Self, FeatureRegistryError> {
        Ok(Self {
            registry: FeatureRegistry::try_new(registrations)?,
        })
    }

    pub fn registry(&self) -> &FeatureRegistry {
        &self.registry
    }

    pub fn render_registered_path(&self, context: &FeatureContext) -> Option<Element> {
        self.registry
            .resolve(&context.canonical_path)
            .map(|registration| registration.render(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_core::{
        FeatureRouteDescriptor, FeatureScope, FeatureSession, FeatureTransportPrimitive, Locale,
    };
    use std::sync::Arc;

    static ROUTES: &[FeatureRouteDescriptor] = &[FeatureRouteDescriptor {
        id: "finance.workspace",
        canonical_path: "/workspaces/finance",
        authorization: "authenticated",
        navigation_group: "finance",
    }];
    static FINANCE: FeatureRegistration = FeatureRegistration::new("finance", ROUTES);

    #[test]
    fn shell_registry_resolves_nested_canonical_paths() {
        let shell = ConsoleShell::try_new([&FINANCE]).expect("registry");
        assert_eq!(
            shell
                .registry()
                .resolve("/workspaces/finance/journals")
                .map(|registration| registration.id),
            Some("finance")
        );
    }

    #[test]
    fn shell_registry_invokes_the_registered_renderer() {
        let shell = ConsoleShell::try_new([&FINANCE]).expect("registry");
        let context = FeatureContext {
            generation: 1,
            locale: Locale::En,
            session: FeatureSession {
                authenticated: true,
                principal_ref: Some("principal-safe-ref".into()),
            },
            active_scope: Some(FeatureScope {
                kind: "holding".into(),
                reference: "holding-safe-ref".into(),
            }),
            transport: Arc::new(FeatureTransportPrimitive::web_same_origin()),
            navigate: Arc::new(|_| {}),
            canonical_path: "/workspaces/finance".into(),
        };
        assert!(shell.render_registered_path(&context).is_some());
    }
}
