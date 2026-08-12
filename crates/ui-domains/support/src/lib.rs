//! Target-neutral Support UI models and shared presentation surfaces.
mod support;
mod support_surfaces;
pub use support::*;
pub use support_surfaces::*;

#[cfg(test)]
pub(crate) use erpx_ui_core::ProjectionFreshness;
pub(crate) use erpx_ui_core::{Locale, LocalizedText, TranslationKey, UiStateKind, use_locale};
pub(crate) use erpx_ui_kit::StatePanel;
