use dioxus::prelude::*;

use erpx_ui_core::{corporate_identity_style, use_corporate_identity, use_locale, use_theme};

use super::toast::ToastProvider;

#[css_module("/src/kit/provider.css")]
struct Styles;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

impl Density {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Comfortable => "comfortable",
            Self::Spacious => "spacious",
        }
    }
}

/// Runtime provider for the unified ERPX component catalog. Theme and locale
/// remain owned by their existing typed contexts; this component exposes them
/// as stable DOM state and owns toast/overlay stacking for every product route.
#[component]
pub fn KitProvider(#[props(default)] density: Density, children: Element) -> Element {
    let locale = use_locale().current();
    let theme = use_theme().current();
    let identity = use_corporate_identity().current();
    let identity_style = corporate_identity_style(&identity);
    rsx! {
        div {
            class: Styles::erpx_kit_root,
            style: "{identity_style}",
            "data-erpx-component": "provider",
            "data-density": density.as_str(),
            "data-locale": locale.language_tag(),
            "data-theme": theme.css_token(),
            ToastProvider {
                {children}
            }
        }
    }
}
