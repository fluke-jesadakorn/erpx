use dioxus::dioxus_core::{AttributeValue, Mutation};
use dioxus::prelude::*;
use erpx_ui::{
    ChartPoint, ChartSeries, DashboardBreakdownSegment, DashboardMetric, DashboardRange,
    DashboardReadModel, DonutChart, HeatmapChart, LineAreaChart, MetricCard, PageHero,
    ReadModelAvailability, ReadModelFreshness, ReadModelMeta, RouteId, SHARED_STYLES,
    StackedBarChart, chart_polyline_points,
};

fn points(values: &[f64]) -> Vec<ChartPoint> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| ChartPoint {
            label: format!("P{index}"),
            value: *value,
        })
        .collect()
}

#[component]
fn premium_fixture() -> Element {
    rsx! {
        PageHero {
            eyebrow: "COMMAND CENTER",
            title: "ภาพรวมการดำเนินงาน",
            description: "ข้อมูลที่ได้รับอนุญาตสำหรับขอบเขตที่ใช้งาน",
            meta: Some("As of 08:00".to_owned()),
        }
        div { class: "ui-metric-grid",
            MetricCard {
                metric: DashboardMetric {
                    key: "authorized_workspaces".to_owned(),
                    label: "Authorized workspaces".to_owned(),
                    value: "12".to_owned(),
                    unit: "workspaces".to_owned(),
                    previous_period_delta: Some(4.2),
                    trend: points(&[2.0, 4.0, 3.0, 8.0]),
                    drill_target: Some(RouteId::PlatformHome),
                },
            }
        }
        DonutChart {
            label: "Authorized workspace distribution",
            segments: vec![
                DashboardBreakdownSegment { key: "finance".to_owned(), label: "Finance".to_owned(), value: 7.0 },
                DashboardBreakdownSegment { key: "operations".to_owned(), label: "Operations".to_owned(), value: 5.0 },
            ],
        }
        HeatmapChart { label: "Activity heatmap", points: points(&[0.0, 1.0, 10.0]) }
        LineAreaChart {
            label: "Authorized activity trend",
            series: vec![ChartSeries { key: "actual".to_owned(), label: "Actual".to_owned(), points: points(&[-2.0, 0.0, 8.0]) }],
        }
        StackedBarChart {
            label: "Authorized activity mix",
            series: vec![ChartSeries { key: "actual".to_owned(), label: "Actual".to_owned(), points: points(&[2.0, 4.0, 6.0]) }],
        }
    }
}

#[test]
fn premium_foundation_contains_exact_palette_accessibility_and_motion_contracts() {
    for token in [
        "#080d12", "#0f161e", "#16212b", "#2a3a47", "#f4f7fa", "#9eafbc", "#66d9ef", "#5eead4",
        "#f4f7f9", "#ffffff", "#edf2f5", "#ccd7df", "#14202a", "#5b6c78", "#0b6b8a", "#0f766e",
    ] {
        assert!(
            SHARED_STYLES.contains(token),
            "missing design token {token}"
        );
    }
    assert!(SHARED_STYLES.contains("--erpx-control-motion: 140ms"));
    assert!(SHARED_STYLES.contains("--erpx-surface-motion: 220ms"));
    assert!(SHARED_STYLES.contains("--erpx-chart-motion: 320ms"));
    assert!(SHARED_STYLES.contains("prefers-reduced-motion"));
    assert!(SHARED_STYLES.contains("--erpx-control-height: 44px"));
    assert!(SHARED_STYLES.contains(":focus-visible"));
}

#[test]
fn premium_primitives_render_safe_bilingual_and_keyboard_observable_markup() {
    let mut dom = VirtualDom::new(premium_fixture);
    let edits = dom.rebuild_to_vec().edits;
    let text = edits
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::CreateTextNode { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(!edits.is_empty());
    assert!(text.contains(&"ภาพรวมการดำเนินงาน"));
    assert!(edits.iter().any(|mutation| matches!(
        mutation,
        Mutation::SetAttribute {
            name: "aria-label",
            value: AttributeValue::Text(_),
            ..
        }
    )));
}

#[test]
fn chart_geometry_handles_zero_negative_large_and_missing_intervals() {
    assert_eq!(chart_polyline_points(&[], 120.0, 34.0), "");
    let geometry = chart_polyline_points(&points(&[-1_000_000.0, 0.0, 1_000_000.0]), 120.0, 34.0);
    assert_eq!(geometry, "0.00,34.00 60.00,17.00 120.00,0.00");
    let flat = chart_polyline_points(&points(&[0.0, 0.0]), 120.0, 34.0);
    assert_eq!(flat, "0.00,34.00 120.00,34.00");
}

#[test]
fn dashboard_contract_rejects_unbounded_or_non_finite_payloads() {
    let mut model = DashboardReadModel {
        range: DashboardRange::ThirtyDays,
        compared_to_previous: true,
        meta: ReadModelMeta {
            scope: "platform".to_owned(),
            as_of: "2026-08-08T00:00:00Z".to_owned(),
            freshness: ReadModelFreshness::Current,
            availability: ReadModelAvailability::Available,
        },
        metrics: vec![],
        breakdowns: vec![],
        work_items: vec![],
    };
    assert!(model.is_bounded());
    model.metrics.push(DashboardMetric {
        key: "bad".to_owned(),
        label: "Bad".to_owned(),
        value: "unavailable".to_owned(),
        unit: String::new(),
        previous_period_delta: None,
        trend: vec![ChartPoint {
            label: "invalid".to_owned(),
            value: f64::NAN,
        }],
        drill_target: Some(RouteId::PlatformHome),
    });
    assert!(!model.is_bounded());
    assert_eq!(
        DashboardRange::parse("90d"),
        Some(DashboardRange::NinetyDays)
    );
    assert_eq!(DashboardRange::parse("forever"), None);
}
