use dioxus::prelude::*;

/// Theme modes supported by the shared shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// Follow the operating system where supported.
    System,
    /// Light theme.
    Light,
    /// Dark theme.
    #[default]
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

#[cfg(test)]
mod tests {
    use super::ThemeMode;

    #[test]
    fn theme_cycle_is_deterministic() {
        assert_eq!(ThemeMode::System.next(), ThemeMode::Light);
        assert_eq!(ThemeMode::Light.next(), ThemeMode::Dark);
        assert_eq!(ThemeMode::Dark.next(), ThemeMode::System);
        assert_eq!(ThemeMode::Dark.css_token(), "dark");
    }
}
