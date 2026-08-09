use dioxus::prelude::*;
use erpx_domain::corporate_identity::{
    BrandDensity, BrandElevation, BrandMotion, BrandNavigation, BrandRadius,
    ResolvedCorporateIdentity,
};

/// Theme modes supported by the shared shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// Follow the operating system where supported.
    System,
    /// Light theme.
    #[default]
    Light,
    /// Dark theme.
    Dark,
}

impl ThemeMode {
    /// Cycle to the next user-selectable mode.
    pub const fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }

    /// Stable DOM token used by CSS selectors.
    pub const fn css_token(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Client-local theme state. It only changes presentation tokens.
#[derive(Clone, Copy)]
pub struct ThemeContext {
    mode: Signal<ThemeMode>,
}

impl ThemeContext {
    /// Create theme state.
    pub fn new(mode: ThemeMode) -> Self {
        Self {
            mode: Signal::new(mode),
        }
    }

    /// Read the active mode.
    pub fn current(self) -> ThemeMode {
        *self.mode.read()
    }

    /// Set the active mode.
    pub fn set(mut self, mode: ThemeMode) {
        self.mode.set(mode);
    }

    /// Toggle to the next mode.
    pub fn toggle(self) {
        self.set(self.current().next());
    }
}

/// Provide theme state to descendants.
pub fn provide_theme(mode: ThemeMode) {
    use_context_provider(|| ThemeContext::new(mode));
}

/// Read theme state from the nearest provider.
pub fn use_theme() -> ThemeContext {
    use_context::<ThemeContext>()
}

/// Safe, resolved corporate identity presentation state. The server resolves
/// inheritance and locks; the client maps typed tokens only to this fixed CSS
/// variable allow-list.
#[derive(Clone, Copy)]
pub struct CorporateIdentityContext {
    identity: Signal<ResolvedCorporateIdentity>,
}

impl CorporateIdentityContext {
    pub fn new(identity: ResolvedCorporateIdentity) -> Self {
        Self {
            identity: Signal::new(identity),
        }
    }

    pub fn current(self) -> ResolvedCorporateIdentity {
        self.identity.read().clone()
    }

    pub fn set(mut self, identity: ResolvedCorporateIdentity) {
        if identity.validate().is_ok() {
            self.identity.set(identity);
        }
    }
}

pub fn provide_corporate_identity(identity: ResolvedCorporateIdentity) {
    use_context_provider(|| CorporateIdentityContext::new(identity));
}

pub fn use_corporate_identity() -> CorporateIdentityContext {
    use_context::<CorporateIdentityContext>()
}

/// Serialize a validated identity into fixed, non-executable CSS custom
/// properties. Every value originates from typed enums or #RRGGBB colors.
pub fn corporate_identity_style(identity: &ResolvedCorporateIdentity) -> String {
    let light = &identity.tokens.light;
    let dark = &identity.tokens.dark;
    let radius = match identity.tokens.radius {
        BrandRadius::Square => "0px",
        BrandRadius::Soft => "10px",
        BrandRadius::Rounded => "16px",
    };
    let control_height = match identity.tokens.density {
        BrandDensity::Compact => "32px",
        BrandDensity::Comfortable => "40px",
        BrandDensity::Spacious => "48px",
    };
    let shadow = match identity.tokens.elevation {
        BrandElevation::Flat => "none",
        BrandElevation::Subtle => "0 8px 24px rgb(2 8 12 / 12%)",
        BrandElevation::Elevated => "0 18px 48px rgb(2 8 12 / 22%)",
    };
    let rail_mix = match identity.tokens.navigation {
        BrandNavigation::Solid => "0%",
        BrandNavigation::Tinted => "8%",
    };
    let motion = match identity.tokens.motion {
        BrandMotion::None => "0ms",
        BrandMotion::Subtle => "140ms",
        BrandMotion::Standard => "220ms",
    };
    let heading_scale = percentage_factor(identity.tokens.heading_scale_percent);
    let body_scale = percentage_factor(identity.tokens.body_scale_percent);
    let line_height = percentage_factor(identity.tokens.line_height_percent);
    format!(
        "--erpx-light-primary:{};--erpx-light-on-primary:{};--erpx-light-accent:{};--erpx-light-on-accent:{};--erpx-light-bg:{};--erpx-light-surface:{};--erpx-light-surface-raised:{};--erpx-light-border:{};--erpx-light-text:{};--erpx-light-text-muted:{};--erpx-light-link:{};--erpx-light-focus:{};--erpx-dark-primary:{};--erpx-dark-on-primary:{};--erpx-dark-accent:{};--erpx-dark-on-accent:{};--erpx-dark-bg:{};--erpx-dark-surface:{};--erpx-dark-surface-raised:{};--erpx-dark-border:{};--erpx-dark-text:{};--erpx-dark-text-muted:{};--erpx-dark-link:{};--erpx-dark-focus:{};--erpx-font-sans:{};--erpx-heading-scale:{};--erpx-body-scale:{};--erpx-line-height:{};--erpx-radius:{};--erpx-control-height:{};--erpx-shadow:{};--erpx-nav-tint:{};--erpx-motion:{};",
        light.primary.as_str(),
        light.on_primary.as_str(),
        light.accent.as_str(),
        light.on_accent.as_str(),
        light.canvas.as_str(),
        light.surface.as_str(),
        light.surface_raised.as_str(),
        light.border.as_str(),
        light.text.as_str(),
        light.text_muted.as_str(),
        light.link.as_str(),
        light.focus.as_str(),
        dark.primary.as_str(),
        dark.on_primary.as_str(),
        dark.accent.as_str(),
        dark.on_accent.as_str(),
        dark.canvas.as_str(),
        dark.surface.as_str(),
        dark.surface_raised.as_str(),
        dark.border.as_str(),
        dark.text.as_str(),
        dark.text_muted.as_str(),
        dark.link.as_str(),
        dark.focus.as_str(),
        identity.tokens.font_pack.css_stack(),
        heading_scale,
        body_scale,
        line_height,
        radius,
        control_height,
        shadow,
        rail_mix,
        motion,
    )
}

/// Convert a validated whole percentage into a CSS number without introducing
/// locale-sensitive floating-point formatting. Typography calculations require
/// a unitless factor: using a percentage here makes line-height collapse to a
/// fraction of one percent when it is divided again by CSS.
fn percentage_factor(percent: u16) -> String {
    format!("{}.{:02}", percent / 100, percent % 100)
}

#[cfg(test)]
mod tests {
    use super::ThemeMode;

    #[test]
    fn theme_cycle_is_deterministic() {
        assert_eq!(ThemeMode::System.next(), ThemeMode::Light);
        assert_eq!(ThemeMode::Light.next(), ThemeMode::Dark);
        assert_eq!(ThemeMode::Dark.next(), ThemeMode::System);
        assert_eq!(ThemeMode::Dark.css_token(), "dark");
        assert_eq!(ThemeMode::default(), ThemeMode::Light);
    }

    #[test]
    fn governed_identity_style_contains_only_fixed_variable_names() {
        let identity = erpx_domain::corporate_identity::ResolvedCorporateIdentity::neutral_erpx();
        let style = super::corporate_identity_style(&identity);
        assert!(style.contains("--erpx-light-primary:#0b6b8a"));
        assert!(style.contains("--erpx-dark-bg:#080d12"));
        assert!(style.contains("--erpx-heading-scale:1.00"));
        assert!(style.contains("--erpx-body-scale:1.00"));
        assert!(style.contains("--erpx-line-height:1.50"));
        assert!(!style.contains("--erpx-line-height:150%"));
        assert!(!style.contains("url("));
        assert!(!style.contains("<script"));
    }
}
