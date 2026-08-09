use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    Locale, RouteId, SafeUiText, TableColumn,
    kit::{
        Table as KitTable,
        badge::{Badge as KitBadge, BadgeVariant},
        button::{Button as KitButton, ButtonSize, ButtonVariant},
        scroll_area::ScrollArea,
        sheet::{Sheet as KitSheet, SheetContentClose, SheetHeader, SheetTitle},
        skeleton::Skeleton as KitSkeleton,
        tabs::{TabList, TabTrigger, Tabs as KitTabs, TabsVariant},
        toolbar::Toolbar as KitToolbar,
    },
    use_locale,
};

pub const MAX_DASHBOARD_METRICS: usize = 12;
pub const MAX_DASHBOARD_BREAKDOWNS: usize = 8;
pub const MAX_DASHBOARD_WORK_ITEMS: usize = 24;
pub const MAX_CHART_POINTS: usize = 48;
pub const MAX_CHART_SERIES: usize = 6;

const fn premium_text(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DashboardRange {
    SevenDays,
    #[default]
    ThirtyDays,
    NinetyDays,
    TwelveMonths,
}

impl DashboardRange {
    pub const ALL: [Self; 4] = [
        Self::SevenDays,
        Self::ThirtyDays,
        Self::NinetyDays,
        Self::TwelveMonths,
    ];

    pub const fn query_value(self) -> &'static str {
        match self {
            Self::SevenDays => "7d",
            Self::ThirtyDays => "30d",
            Self::NinetyDays => "90d",
            Self::TwelveMonths => "12m",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "7d" => Some(Self::SevenDays),
            "30d" => Some(Self::ThirtyDays),
            "90d" => Some(Self::NinetyDays),
            "12m" => Some(Self::TwelveMonths),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadModelAvailability {
    Available,
    Partial,
    Empty,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadModelFreshness {
    Live,
    Current,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartPoint {
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartSeries {
    pub key: String,
    pub label: String,
    pub points: Vec<ChartPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardMetric {
    pub key: String,
    pub label: String,
    pub value: String,
    pub unit: String,
    pub previous_period_delta: Option<f64>,
    pub trend: Vec<ChartPoint>,
    pub drill_target: Option<RouteId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardBreakdownSegment {
    pub key: String,
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardBreakdown {
    pub key: String,
    pub label: String,
    pub segments: Vec<DashboardBreakdownSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatusTone {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Danger,
    Security,
}

impl StatusTone {
    const fn css_token(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
            Self::Security => "security",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardWorkItem {
    pub key: String,
    pub label: String,
    pub count: u32,
    pub severity: StatusTone,
    pub drill_target: RouteId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadModelMeta {
    pub scope: String,
    pub as_of: String,
    pub freshness: ReadModelFreshness,
    pub availability: ReadModelAvailability,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardReadModel {
    pub range: DashboardRange,
    pub compared_to_previous: bool,
    pub meta: ReadModelMeta,
    pub metrics: Vec<DashboardMetric>,
    pub breakdowns: Vec<DashboardBreakdown>,
    pub work_items: Vec<DashboardWorkItem>,
}

impl DashboardReadModel {
    pub fn is_bounded(&self) -> bool {
        self.metrics.len() <= MAX_DASHBOARD_METRICS
            && self.breakdowns.len() <= MAX_DASHBOARD_BREAKDOWNS
            && self.work_items.len() <= MAX_DASHBOARD_WORK_ITEMS
            && self.metrics.iter().all(|metric| {
                metric.trend.len() <= MAX_CHART_POINTS
                    && metric.trend.iter().all(|point| point.value.is_finite())
            })
            && self.breakdowns.iter().all(|breakdown| {
                breakdown.segments.len() <= MAX_CHART_POINTS
                    && breakdown
                        .segments
                        .iter()
                        .all(|segment| segment.value.is_finite())
            })
    }
}

fn safe(value: impl AsRef<str>) -> String {
    SafeUiText::new(value).as_str().to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonTone {
    #[default]
    Secondary,
    Primary,
    Ghost,
    Danger,
}

impl ButtonTone {
    const fn css_token(self) -> &'static str {
        match self {
            Self::Secondary => "secondary",
            Self::Primary => "primary",
            Self::Ghost => "ghost",
            Self::Danger => "danger",
        }
    }
}

#[component]
pub fn Button(
    label: String,
    #[props(default)] tone: ButtonTone,
    #[props(default)] disabled: bool,
    #[props(default)] busy: bool,
    #[props(default)] on_press: Option<EventHandler<()>>,
) -> Element {
    let label = safe(label);
    let class = format!("ui-premium-button {}", tone.css_token());
    let variant = match tone {
        ButtonTone::Primary => ButtonVariant::Primary,
        ButtonTone::Secondary => ButtonVariant::Secondary,
        ButtonTone::Ghost => ButtonVariant::Ghost,
        ButtonTone::Danger => ButtonVariant::Destructive,
    };
    rsx! {
        KitButton {
            class,
            variant,
            size: ButtonSize::Default,
            r#type: "button",
            disabled: disabled || busy,
            aria_busy: busy.to_string(),
            "data-erpx-component": "button",
            "data-tone": tone.css_token(),
            "data-size": "default",
            onclick: move |_| {
                if !disabled && !busy && let Some(handler) = on_press.as_ref() {
                    handler.call(());
                }
            },
            if busy { span { class: "ui-spinner", aria_hidden: "true" } }
            span { "{label}" }
        }
    }
}

#[component]
pub fn PageHero(
    eyebrow: String,
    title: String,
    description: String,
    #[props(default)] meta: Option<String>,
    #[props(default)] actions: Option<Element>,
) -> Element {
    let eyebrow = safe(eyebrow);
    let title = safe(title);
    let description = safe(description);
    let meta = meta.map(safe);
    rsx! {
        header { class: "ui-page-hero",
            div { class: "ui-page-hero-copy",
                div { class: "ui-page-hero-eyebrow", "{eyebrow}" }
                h2 { "{title}" }
                p { "{description}" }
                if let Some(meta) = meta { small { "{meta}" } }
            }
            if let Some(actions) = actions {
                div { class: "ui-page-hero-actions", {actions} }
            }
        }
    }
}

#[component]
pub fn SectionCard(
    title: String,
    #[props(default)] eyebrow: Option<String>,
    #[props(default)] description: Option<String>,
    #[props(default)] actions: Option<Element>,
    children: Element,
) -> Element {
    let title = safe(title);
    let eyebrow = eyebrow.map(safe);
    let description = description.map(safe);
    rsx! {
        section { class: "ui-section-card",
            header { class: "ui-section-card-header",
                div {
                    if let Some(eyebrow) = eyebrow { small { "{eyebrow}" } }
                    h3 { "{title}" }
                    if let Some(description) = description { p { "{description}" } }
                }
                if let Some(actions) = actions { div { class: "ui-section-card-actions", {actions} } }
            }
            div { class: "ui-section-card-body", {children} }
        }
    }
}

#[component]
pub fn StatusBadge(label: String, #[props(default)] tone: StatusTone) -> Element {
    let label = safe(label);
    let class = format!("ui-status-badge {}", tone.css_token());
    let variant = match tone {
        StatusTone::Danger | StatusTone::Security => BadgeVariant::Destructive,
        StatusTone::Info | StatusTone::Success => BadgeVariant::Primary,
        StatusTone::Neutral | StatusTone::Warning => BadgeVariant::Secondary,
    };
    rsx! {
        KitBadge {
            class,
            variant,
            "data-erpx-component": "badge",
            "data-tone": tone.css_token(),
            "{label}"
        }
    }
}

#[component]
pub fn MetricCard(
    metric: DashboardMetric,
    #[props(default)] on_drill: Option<EventHandler<RouteId>>,
) -> Element {
    let locale = use_locale().current();
    let label = safe(&metric.label);
    let value = safe(&metric.value);
    let unit = safe(&metric.unit);
    let delta = metric.previous_period_delta.map(|value| {
        let prefix = if value > 0.0 { "+" } else { "" };
        format!("{prefix}{value:.1}%")
    });
    let delta_tone = match metric.previous_period_delta {
        Some(value) if value > 0.0 => StatusTone::Success,
        Some(value) if value < 0.0 => StatusTone::Danger,
        _ => StatusTone::Neutral,
    };
    let target = metric.drill_target;
    rsx! {
        article { class: "ui-metric-card",
            header {
                span { "{label}" }
                if let Some(delta) = delta { StatusBadge { label: delta, tone: delta_tone } }
            }
            div { class: "ui-metric-value", strong { "{value}" } span { "{unit}" } }
            Sparkline { points: metric.trend, label: format!("{label} trend") }
            if let Some(target) = target {
                KitButton {
                    class: "ui-card-drill",
                    variant: ButtonVariant::Link,
                    size: ButtonSize::Sm,
                    r#type: "button",
                    "data-erpx-component": "button",
                    "data-tone": "ghost",
                    "data-size": "sm",
                    onclick: move |_| if let Some(handler) = on_drill.as_ref() { handler.call(target); },
                    {premium_text(locale, "Open details", "เปิดรายละเอียด")}
                    span { aria_hidden: "true", "→" }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabItem {
    pub id: String,
    pub label: String,
}

#[component]
pub fn Tabs(items: Vec<TabItem>, active_id: String, on_change: EventHandler<String>) -> Element {
    let active_id = safe(active_id);
    rsx! {
        KitTabs {
            class: "ui-premium-tabs",
            value: Some(active_id.clone()),
            variant: TabsVariant::Default,
            "data-erpx-component": "tabs",
            on_value_change: move |value| on_change.call(value),
            TabList {
                for (index, item) in items.into_iter().enumerate() {
                    {
                        let id = safe(&item.id);
                        let label = safe(&item.label);
                        rsx! {
                            TabTrigger {
                                value: id,
                                index,
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn Toolbar(label: String, children: Element) -> Element {
    rsx! {
        KitToolbar {
            class: "ui-premium-toolbar",
            aria_label: safe(label),
            "data-erpx-component": "toolbar",
            {children}
        }
    }
}

#[component]
pub fn Drawer(open: bool, title: String, on_close: EventHandler<()>, children: Element) -> Element {
    let locale = use_locale().current();
    let title = safe(title);
    rsx! {
        KitSheet {
            class: "ui-drawer-layer",
            open,
            on_open_change: move |next_open: bool| if !next_open { on_close.call(()); },
            aria_keyshortcuts: "Escape",
            "data-erpx-component": "sheet",
            "data-drawer-focus-trap": "true",
            SheetHeader {
                SheetTitle { id: "ui-drawer-title", "{title}" }
                SheetContentClose {
                    aria_label: premium_text(locale, "Close drawer", "ปิดแผงรายละเอียด"),
                }
            }
            div { class: "ui-drawer-body", {children} }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastTone {
    #[default]
    Info,
    Success,
    Warning,
    Danger,
}

#[component]
pub fn Toast(title: String, message: String, #[props(default)] tone: ToastTone) -> Element {
    let tone = match tone {
        ToastTone::Info => "info",
        ToastTone::Success => "success",
        ToastTone::Warning => "warning",
        ToastTone::Danger => "danger",
    };
    rsx! { div { class: "ui-toast {tone}", role: "status", aria_live: "polite", strong { {safe(title)} } p { {safe(message)} } } }
}

#[component]
pub fn Skeleton(#[props(default = 3)] lines: usize) -> Element {
    let locale = use_locale().current();
    let lines = lines.clamp(1, 8);
    rsx! {
        div {
            class: "ui-skeleton",
            aria_label: premium_text(locale, "Loading", "กำลังโหลด"),
            aria_busy: "true",
            "data-erpx-component": "skeleton",
            for index in 0..lines {
                KitSkeleton { style: "--skeleton-index: {index}" }
            }
        }
    }
}

#[component]
pub fn EmptyState(
    title: String,
    message: String,
    #[props(default)] action: Option<Element>,
) -> Element {
    rsx! { section { class: "ui-empty-state", role: "status", div { class: "ui-empty-state-glyph", aria_hidden: "true", "◇" } h3 { {safe(title)} } p { {safe(message)} } if let Some(action) = action { {action} } } }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineItem {
    pub key: String,
    pub title: String,
    pub description: String,
    pub timestamp: String,
    pub tone: StatusTone,
}

#[component]
pub fn Timeline(items: Vec<TimelineItem>, label: String) -> Element {
    rsx! { ol { class: "ui-timeline", aria_label: safe(label), for item in items { li { "data-tone": item.tone.css_token(), div { class: "ui-timeline-marker", aria_hidden: "true" } div { strong { {safe(item.title)} } p { {safe(item.description)} } time { {safe(item.timestamp)} } } } } } }
}

#[component]
pub fn DataTableV2(
    caption: String,
    columns: Vec<TableColumn>,
    empty: bool,
    children: Element,
) -> Element {
    let locale = use_locale().current();
    let columns = columns
        .into_iter()
        .map(|column| TableColumn {
            id: safe(column.id),
            label: safe(column.label),
            numeric: column.numeric,
        })
        .collect::<Vec<_>>();
    let count = columns.len().max(1);
    rsx! {
        ScrollArea {
            class: "ui-data-table-v2",
            tabindex: "0",
            "data-erpx-component": "data-table-scroll-area",
            KitTable {
                "data-erpx-component": "data-table",
                caption { class: "sr-only", {safe(caption)} }
                thead {
                    tr {
                        for column in columns {
                            th {
                                scope: "col",
                                class: if column.numeric { "numeric" } else { "" },
                                "{column.label}"
                            }
                        }
                    }
                }
                tbody {
                    if empty {
                        tr { td { colspan: "{count}", class: "ui-table-empty", {premium_text(locale, "No records", "ไม่มีข้อมูล")} } }
                    } else {
                        {children}
                    }
                }
            }
        }
    }
}

#[component]
pub fn DashboardRangeSelector(
    selected: DashboardRange,
    on_change: EventHandler<DashboardRange>,
) -> Element {
    let locale = use_locale().current();
    let label = match locale {
        Locale::En => "Dashboard time range",
        Locale::Th => "ช่วงเวลาของแดชบอร์ด",
    };
    rsx! { div { class: "ui-range-selector", aria_label: label, "data-erpx-component": "toggle-group", for range in DashboardRange::ALL { KitButton { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, r#type: "button", class: if range == selected { "active" } else { "" }, aria_pressed: (range == selected).to_string(), "data-erpx-component": "toggle", "data-state": if range == selected { "on" } else { "off" }, onclick: move |_| on_change.call(range), "{range.query_value()}" } } } }
}

pub fn chart_polyline_points(points: &[ChartPoint], width: f64, height: f64) -> String {
    let bounded = points.iter().take(MAX_CHART_POINTS).collect::<Vec<_>>();
    if bounded.is_empty() {
        return String::new();
    }
    let min = bounded
        .iter()
        .map(|point| point.value)
        .fold(f64::INFINITY, f64::min);
    let max = bounded
        .iter()
        .map(|point| point.value)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).abs().max(1.0);
    let divisor = bounded.len().saturating_sub(1).max(1) as f64;
    bounded
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let x = index as f64 / divisor * width;
            let y = height - ((point.value - min) / span * height);
            format!("{x:.2},{y:.2}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[component]
pub fn Sparkline(points: Vec<ChartPoint>, label: String) -> Element {
    let points = points
        .into_iter()
        .take(MAX_CHART_POINTS)
        .filter(|point| point.value.is_finite())
        .collect::<Vec<_>>();
    let geometry = chart_polyline_points(&points, 120.0, 34.0);
    rsx! { svg { class: "ui-sparkline", view_box: "0 0 120 34", role: "img", "aria-label": safe(label), preserve_aspect_ratio: "none", polyline { points: geometry } } }
}

#[component]
pub fn LineAreaChart(series: Vec<ChartSeries>, label: String) -> Element {
    let locale = use_locale().current();
    let series = series
        .into_iter()
        .take(MAX_CHART_SERIES)
        .map(|mut item| {
            item.points = item
                .points
                .into_iter()
                .take(MAX_CHART_POINTS)
                .filter(|point| point.value.is_finite())
                .collect();
            item
        })
        .collect::<Vec<_>>();
    let series_count = series.len();
    let mut visible = use_signal(move || vec![true; series_count]);
    rsx! {
        figure { class: "ui-chart ui-line-area-chart",
            svg {
                view_box: "0 0 640 220",
                role: "img",
                "aria-label": safe(&label),
                preserve_aspect_ratio: "none",
                line { class: "axis", x1: "0", y1: "200", x2: "640", y2: "200" }
                for (series_index, item) in series.iter().enumerate() {
                    if visible.read().get(series_index).copied().unwrap_or(false) {
                        {
                            let geometry = chart_polyline_points(&item.points, 640.0, 190.0);
                            let area = if geometry.is_empty() {
                                String::new()
                            } else {
                                format!("0,200 {geometry} 640,200")
                            };
                            let coordinates = geometry
                                .split(' ')
                                .map(str::to_owned)
                                .collect::<Vec<_>>();
                            rsx! {
                                polygon { class: "area", "data-series": series_index.to_string(), points: area }
                                polyline { class: "line", "data-series": series_index.to_string(), points: geometry }
                                for (point, coordinate) in item.points.iter().zip(coordinates) {
                                    {
                                        let (x, y) = coordinate
                                            .split_once(',')
                                            .unwrap_or(("0", "200"));
                                        rsx! {
                                            circle {
                                                "data-series": series_index.to_string(),
                                                cx: x,
                                                cy: y,
                                                r: "8",
                                                tabindex: "0",
                                                "title": format!("{} · {}: {}", safe(&item.label), safe(&point.label), point.value),
                                                "aria-label": format!("{} {} {}", safe(&item.label), safe(&point.label), point.value),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            figcaption { class: "ui-chart-legend", aria_label: premium_text(locale, "Chart series", "ชุดข้อมูลแผนภูมิ"),
                for (series_index, item) in series.iter().enumerate() {
                    KitButton { variant: ButtonVariant::Secondary, size: ButtonSize::Default,
                        r#type: "button",
                        "data-series": series_index.to_string(),
                        aria_pressed: visible.read().get(series_index).copied().unwrap_or(false).to_string(),
                        onclick: move |_| {
                            if let Some(value) = visible.write().get_mut(series_index) {
                                *value = !*value;
                            }
                        },
                        i {}
                        {safe(&item.label)}
                    }
                }
            }
        }
    }
}

#[component]
pub fn BarChart(series: Vec<ChartSeries>, label: String) -> Element {
    let series = series
        .into_iter()
        .take(MAX_CHART_SERIES)
        .collect::<Vec<_>>();
    let values = series
        .iter()
        .flat_map(|item| item.points.iter())
        .filter(|point| point.value.is_finite())
        .collect::<Vec<_>>();
    let max = values
        .iter()
        .map(|point| point.value.abs())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    rsx! {
        div {
            class: "ui-chart ui-bar-chart",
            role: "img",
            aria_label: safe(label),
            for (series_index, item) in series.iter().enumerate() {
                div {
                    class: "ui-bar-series",
                    "data-series": series_index.to_string(),
                    for point in item.points.iter().take(MAX_CHART_POINTS) {
                        div {
                            class: "ui-bar-column",
                            title: format!("{}: {}", safe(&point.label), point.value),
                            span {
                                style: format!(
                                    "--bar-size: {:.2}%",
                                    point.value.abs() / max * 100.0
                                ),
                                tabindex: "0",
                                aria_label: format!("{} {}", safe(&point.label), point.value),
                            }
                            small { {safe(&point.label)} }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn StackedBarChart(series: Vec<ChartSeries>, label: String) -> Element {
    let series = series
        .into_iter()
        .take(MAX_CHART_SERIES)
        .collect::<Vec<_>>();
    let category_count = series
        .iter()
        .map(|item| item.points.len())
        .max()
        .unwrap_or(0)
        .min(MAX_CHART_POINTS);
    let categories = (0..category_count)
        .map(|index| {
            let label = series
                .iter()
                .find_map(|item| item.points.get(index))
                .map(|point| safe(&point.label))
                .unwrap_or_default();
            let total = series
                .iter()
                .filter_map(|item| item.points.get(index))
                .filter(|point| point.value.is_finite())
                .map(|point| point.value.abs())
                .sum::<f64>()
                .max(1.0);
            (index, label, total)
        })
        .collect::<Vec<_>>();
    rsx! {
        figure { class: "ui-chart ui-stacked-bar-chart", role: "img", aria_label: safe(label),
            div { class: "ui-stacked-bars",
                for (category_index, category_label, total) in categories {
                    div { class: "ui-stacked-column",
                        div { class: "ui-stacked-track",
                            for (series_index, item) in series.iter().enumerate() {
                                if let Some(point) = item.points.get(category_index) {
                                    span {
                                        "data-series": series_index.to_string(),
                                        tabindex: "0",
                                        style: format!("--stack-size: {:.3}", point.value.abs() / total),
                                        title: format!("{} · {}: {}", safe(&item.label), safe(&point.label), point.value),
                                        aria_label: format!("{} {} {}", safe(&item.label), safe(&point.label), point.value),
                                    }
                                }
                            }
                        }
                        small { "{category_label}" }
                    }
                }
            }
            figcaption { class: "ui-chart-legend",
                for (series_index, item) in series.iter().enumerate() {
                    span { "data-series": series_index.to_string(), i {} {safe(&item.label)} }
                }
            }
        }
    }
}

#[component]
pub fn DonutChart(segments: Vec<DashboardBreakdownSegment>, label: String) -> Element {
    let segments = segments
        .into_iter()
        .take(MAX_CHART_SERIES)
        .filter(|segment| segment.value.is_finite() && segment.value > 0.0)
        .collect::<Vec<_>>();
    let total = segments
        .iter()
        .map(|segment| segment.value)
        .sum::<f64>()
        .max(1.0);
    let mut consumed = 0.0;
    let rings = segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let size = segment.value / total * 100.0;
            let offset = -consumed;
            consumed += size;
            (index, segment.clone(), size, offset)
        })
        .collect::<Vec<_>>();
    rsx! {
        figure { class: "ui-chart ui-donut-chart",
            svg {
                view_box: "0 0 42 42",
                role: "img",
                "aria-label": safe(&label),
                circle { class: "track", cx: "21", cy: "21", r: "15.9155" }
                for (index, segment, size, offset) in rings {
                    circle {
                        class: "segment",
                        "data-series": index.to_string(),
                        cx: "21",
                        cy: "21",
                        r: "15.9155",
                        stroke_dasharray: format!("{size:.3} {:.3}", 100.0 - size),
                        stroke_dashoffset: format!("{offset:.3}"),
                        tabindex: "0",
                        "aria-label": format!("{} {}", safe(&segment.label), segment.value),
                    }
                }
                text { x: "21", y: "22", text_anchor: "middle", "{total:.0}" }
            }
            figcaption {
                for (index, segment) in segments.iter().enumerate() {
                    span {
                        "data-series": index.to_string(),
                        i {}
                        {safe(&segment.label)}
                        strong { "{segment.value:.0}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn HeatmapChart(points: Vec<ChartPoint>, label: String) -> Element {
    let points = points
        .into_iter()
        .take(MAX_CHART_POINTS)
        .filter(|point| point.value.is_finite())
        .collect::<Vec<_>>();
    let max = points
        .iter()
        .map(|point| point.value.abs())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    rsx! {
        div {
            class: "ui-chart ui-heatmap",
            role: "img",
            aria_label: safe(label),
            for point in points {
                span {
                    tabindex: "0",
                    style: format!("--heat: {:.3}", point.value.abs() / max),
                    title: format!("{}: {}", safe(&point.label), point.value),
                    aria_label: format!("{} {}", safe(&point.label), point.value),
                }
            }
        }
    }
}

#[component]
pub fn ProgressRing(value: f64, label: String) -> Element {
    let value = if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    };
    rsx! { div { class: "ui-progress-ring", role: "progressbar", aria_label: safe(label), aria_valuemin: "0", aria_valuemax: "100", aria_valuenow: format!("{value:.0}"), style: format!("--progress: {value:.2}%"), span { "{value:.0}%" } } }
}
