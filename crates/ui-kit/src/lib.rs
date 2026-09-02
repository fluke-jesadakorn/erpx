//! Stable component-kit boundary for ERPX applications.

use dioxus::prelude::*;

mod components;
mod current_workspace;
mod enterprise;
pub mod kit;
mod premium;

pub use components::{
    AppShell, BannerKind, CollectionLayout, CorporateIdentityEditor, CorporateIdentityEditorAction,
    CorporateIdentityEditorActionKind, CorporateIdentityEditorSubmission, DashboardLayout,
    DataTable, Dialog, DialogSize, EnterpriseIcon, Field, FilterBar, LocaleToggle,
    LockedTooltipButton, NavigationDisclosureState, OperationsLayout, PageHeader, RecordLayout,
    RoleContextSummary, RouteStateBoundary, ScopeSelector, SessionBanners, SettingsLayout,
    StateBoundary, StatePanel, TableColumn, TextField, WorkflowLayout,
    use_account_session_focus_target, use_navigation_disclosure_state,
};
pub use current_workspace::{
    CurrentCommandAvailability, CurrentCommandCard, CurrentFeatureKind, CurrentRecordFamilySpec,
    CurrentRecordRoute, LIVE_CURRENT_RECORD_TYPES, ReadYourWritesEvidence, RecordPublicationState,
    current_command_cards, current_workspace_state, render_current_workspace_surface,
};
pub use enterprise::{AdaptiveCollectionSurface, EnterpriseStatePanel};
pub use erpx_ui_core::{ReadModelAvailability, ReadModelFreshness};
pub use kit::{
    Density, Disclosure, KitProvider, Link, SelectControl, Table, button, input, textarea,
};
pub use premium::{
    BarChart, Board, BoardCardDef, BoardColumnDef, Button, ButtonTone, ChartPoint, ChartSeries,
    DashboardBreakdown, DashboardBreakdownSegment, DashboardMetric, DashboardRange,
    DashboardRangeSelector, DashboardReadModel, DashboardWorkItem, DonutChart, Drawer, EmptyState,
    HeatmapChart, LineAreaChart, MetricCard, PageHero, PremiumDataTable, ProgressRing,
    ReadModelMeta, SectionCard, Skeleton, Sparkline, StackedBarChart, StatusBadge, StatusTone,
    TabItem, Tabs, Timeline, TimelineItem, Toast, ToastTone, Toolbar, chart_polyline_points,
};

/// Shared CSS variables and component styles. Applications can include this once in their root
/// component and layer client-specific styles on top.
pub const SHARED_STYLES: &str = concat!(
    include_str!("../assets/tokens.css"),
    include_str!("../assets/tailwind.css"),
    include_str!("../assets/linear-kit-overrides.css"),
    include_str!("../assets/foundation.css"),
    include_str!("../assets/premium-composites.css"),
    include_str!("../assets/chart-composites.css")
);

/// Compiled, hashed stylesheet layers for Dioxus web targets.
#[component]
pub fn SharedStylesheets() -> Element {
    static TOKENS: Asset = asset!("/assets/tokens.css");
    static TAILWIND: Asset = asset!("/assets/tailwind.css");
    static LINEAR_OVERRIDES: Asset = asset!("/assets/linear-kit-overrides.css");
    static FOUNDATION: Asset = asset!("/assets/foundation.css");
    static PREMIUM_COMPOSITES: Asset = asset!("/assets/premium-composites.css");
    static CHART_COMPOSITES: Asset = asset!("/assets/chart-composites.css");
    rsx! {
        document::Stylesheet { href: TOKENS }
        document::Stylesheet { href: TAILWIND }
        document::Stylesheet { href: LINEAR_OVERRIDES }
        document::Stylesheet { href: FOUNDATION }
        document::Stylesheet { href: PREMIUM_COMPOSITES }
        document::Stylesheet { href: CHART_COMPOSITES }
    }
}
