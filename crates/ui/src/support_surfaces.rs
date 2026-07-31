//! Shared Dioxus presentation surfaces for support tickets and platform incidents.
//!
//! These components are deliberately transport-neutral. An application owns the conversion from
//! an authorized gRPC response into the display models in [`crate::support`], and this module only
//! renders those already-redacted models. There are no command callbacks, credentials, capability
//! secrets, tenant connection details, or authorization decisions here.

use dioxus::prelude::*;

use crate::{
    IncidentDetailState, IncidentDisplayModel, IncidentListState, StatePanel, SupportDisplayModel,
    SupportRoute, SupportState, SupportTicketListState, SupportViewState, TranslationKey,
    UiStateKind, use_locale,
};

/// State payload accepted by the route-aware support/incident surface.
///
/// The consuming application is responsible for selecting the matching variant after an
/// authorized transport response. A mismatched route/state pair renders a fail-closed error
/// panel and never guesses which data should be displayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportIncidentSurfaceState {
    /// A support route display state.
    Support(SupportState),
    /// The platform incident list display state.
    IncidentList(IncidentListState),
    /// The platform incident detail display state.
    IncidentDetail(IncidentDetailState),
    /// The platform support ticket list display state.
    TicketList(SupportTicketListState),
}

/// Render the support and incident route tabs using the canonical shared route registry.
///
/// The callback is navigation-only. It does not authorize, create, approve, revoke, or mutate
/// support access; the consuming application must still obtain every read through its authorized
/// transport boundary.
#[component]
pub fn SupportRouteTabs(
    active_route: SupportRoute,
    on_navigate: EventHandler<SupportRoute>,
) -> Element {
    let locale = use_locale();
    rsx! {
        nav { class: "ui-support-route-tabs", aria_label: "Support and incident routes",
            for definition in route_definitions_for_audience(active_route) {
                button {
                    class: if definition.route == active_route { "ui-route-tab active" } else { "ui-route-tab" },
                    r#type: "button",
                    aria_current: if definition.route == active_route {
                        Some("page")
                    } else {
                        None::<&str>
                    },
                    title: "{definition.template}",
                    onclick: move |_| on_navigate.call(definition.route),
                    "{definition.route.label(locale.current())}"
                }
            }
        }
    }
}

/// Route-aware support/incident surface dispatcher.
#[component]
pub fn SupportIncidentSurface(route: SupportRoute, state: SupportIncidentSurfaceState) -> Element {
    match (route, state) {
        (route, SupportIncidentSurfaceState::Support(state))
            if !matches!(
                route,
                SupportRoute::PlatformIncidents | SupportRoute::PlatformIncidentDetail
            ) && support_state_matches_route(route, &state) =>
        {
            rsx! {
                SupportSurface { route, state }
            }
        }
        (SupportRoute::PlatformIncidents, SupportIncidentSurfaceState::IncidentList(state)) => {
            rsx! { IncidentListSurface { state } }
        }
        (
            SupportRoute::PlatformIncidentDetail,
            SupportIncidentSurfaceState::IncidentDetail(state),
        ) => {
            rsx! { IncidentDetailSurface { state } }
        }
        (SupportRoute::PlatformSupport, SupportIncidentSurfaceState::TicketList(state)) => {
            rsx! { SupportTicketListSurface { state } }
        }
        (route, _) => rsx! {
            SurfaceStateMismatch { route }
        },
    }
}

/// Render the platform support ticket list from its explicit safe state.
#[component]
pub fn SupportTicketListSurface(state: SupportTicketListState) -> Element {
    if !data_matches_route(
        state.data(),
        SupportRoute::PlatformSupport,
        |model: &crate::SupportTicketListDisplayModel| model.route,
    ) {
        return rsx! { SurfaceStateMismatch { route: SupportRoute::PlatformSupport } };
    }
    let locale = use_locale();
    let state_kind = state.kind();
    let state_title = state
        .state_label(locale.current())
        .unwrap_or_else(|| SupportRoute::PlatformSupport.label(locale.current()));
    let state_message = support_ticket_list_state_message(&state);
    let model = state.data().cloned();

    rsx! {
        section { class: "ui-support-surface", aria_label: "Support ticket list surface",
            SurfaceHeading {
                route: SupportRoute::PlatformSupport,
                title: SupportRoute::PlatformSupport.label(locale.current()).to_string(),
                description: "Platform-scoped support ticket metadata from the authorized read projection. Ticket bodies and commands remain server-authorized.".to_string(),
            }
            if let Some(kind) = state_kind {
                StatePanel {
                    state: kind,
                    title: state_title.to_string(),
                    message: state_message,
                    action_label: None,
                    on_action: None,
                }
            }
            if let Some(model) = model {
                SupportTicketListContent { model }
            }
        }
    }
}

#[component]
fn SupportTicketListContent(model: crate::SupportTicketListDisplayModel) -> Element {
    let locale = use_locale();
    rsx! {
        div { class: "ui-support-ticket-list",
            if model.items().is_empty() {
                p { class: "ui-safe-note", "No platform support tickets are present in this projection." }
            } else {
                for item in model.items() {
                    article { class: "ui-support-card",
                        header { class: "ui-support-card-heading",
                            div {
                                span { class: "ui-eyebrow", "SAFE SUPPORT TICKET" }
                                strong { "{item.id()}" }
                            }
                            span { class: "ui-support-status", "{item.status().label(locale.current())}" }
                        }
                        dl { class: "ui-support-fields",
                            dt { "Customer reference" }
                            dd { "{item.customer_id()}" }
                            dt { "Severity" }
                            dd { "{item.severity()}" }
                        }
                        p { class: "ui-safe-note", "Ticket description, customer replies, internal notes, capabilities, and commands are not rendered by this metadata-only surface." }
                    }
                }
            }
        }
    }
}

/// Render any support route from its explicit safe state.
#[component]
pub fn SupportSurface(route: SupportRoute, state: SupportState) -> Element {
    if !support_state_matches_route(route, &state) {
        return rsx! { SurfaceStateMismatch { route } };
    }
    let locale = use_locale();
    let state_kind = state.kind();
    let state_title = state
        .state_label(locale.current())
        .unwrap_or_else(|| route.label(locale.current()));
    let state_message = support_state_message(&state);
    let model = state.data().cloned();

    rsx! {
        section { class: "ui-support-surface", aria_label: "Support surface",
            SurfaceHeading {
                route,
                title: route.label(locale.current()).to_string(),
                description: support_route_description(route, locale.current()).to_string(),
            }
            if let Some(kind) = state_kind {
                StatePanel {
                    state: kind,
                    title: state_title.to_string(),
                    message: state_message,
                    action_label: None,
                    on_action: None,
                }
            }
            if let Some(model) = model {
                SupportRecord { model }
            }
        }
    }
}

/// Render the platform incident list from its explicit safe state.
#[component]
pub fn IncidentListSurface(state: IncidentListState) -> Element {
    if !data_matches_route(
        state.data(),
        SupportRoute::PlatformIncidents,
        |model: &crate::IncidentListDisplayModel| model.route,
    ) {
        return rsx! { SurfaceStateMismatch { route: SupportRoute::PlatformIncidents } };
    }
    let locale = use_locale();
    let state_kind = state.kind();
    let state_title = state
        .state_label(locale.current())
        .unwrap_or_else(|| SupportRoute::PlatformIncidents.label(locale.current()));
    let state_message = incident_state_message(&state);
    let model = state.data().cloned();

    rsx! {
        section { class: "ui-support-surface", aria_label: "Incident list surface",
            SurfaceHeading {
                route: SupportRoute::PlatformIncidents,
                title: SupportRoute::PlatformIncidents.label(locale.current()).to_string(),
                description: "Platform-scoped incident projection. This view is read-only; incident commands remain server-authorized.".to_string(),
            }
            if let Some(kind) = state_kind {
                StatePanel {
                    state: kind,
                    title: state_title.to_string(),
                    message: state_message,
                    action_label: None,
                    on_action: None,
                }
            }
            if let Some(model) = model {
                IncidentListContent { model }
            }
        }
    }
}

/// Render the platform incident detail from its explicit safe state.
#[component]
pub fn IncidentDetailSurface(state: IncidentDetailState) -> Element {
    if !data_matches_route(
        state.data(),
        SupportRoute::PlatformIncidentDetail,
        |model: &crate::IncidentDisplayModel| model.route,
    ) {
        return rsx! { SurfaceStateMismatch { route: SupportRoute::PlatformIncidentDetail } };
    }
    let locale = use_locale();
    let state_kind = state.kind();
    let state_title = state
        .state_label(locale.current())
        .unwrap_or_else(|| SupportRoute::PlatformIncidentDetail.label(locale.current()));
    let state_message = incident_state_message(&state);
    let model = state.data().cloned();

    rsx! {
        section { class: "ui-support-surface", aria_label: "Incident detail surface",
            SurfaceHeading {
                route: SupportRoute::PlatformIncidentDetail,
                title: SupportRoute::PlatformIncidentDetail.label(locale.current()).to_string(),
                description: "Platform-scoped incident details from the authorized read projection. Commands are intentionally not exposed here.".to_string(),
            }
            if let Some(kind) = state_kind {
                StatePanel {
                    state: kind,
                    title: state_title.to_string(),
                    message: state_message,
                    action_label: None,
                    on_action: None,
                }
            }
            if let Some(model) = model {
                IncidentDetailContent { model }
            }
        }
    }
}

#[component]
fn SurfaceStateMismatch(route: SupportRoute) -> Element {
    rsx! {
        StatePanel {
            state: UiStateKind::Error,
            title: "Surface state mismatch".to_string(),
            message: format!(
                "The authorized read for {} was not bound to its typed route. No data was rendered.",
                route.template(),
            ),
            action_label: None,
            on_action: None,
        }
    }
}

fn support_state_matches_route(route: SupportRoute, state: &SupportState) -> bool {
    let Some(model) = state.data() else {
        return true;
    };
    if model.route != route {
        return false;
    }
    if route.audience() == crate::SupportRouteAudience::Tenant {
        return state.metadata().is_some_and(|state_metadata| {
            state_metadata.has_scope() && state_metadata.has_same_scope_as(&model.metadata)
        });
    }
    true
}

fn route_definitions_for_audience(
    active_route: SupportRoute,
) -> impl Iterator<Item = crate::SupportRouteDefinition> {
    let active_audience = active_route.audience();
    SupportRoute::all()
        .iter()
        .copied()
        .filter(move |definition| definition.audience == active_audience)
}

fn data_matches_route<T, F>(data: Option<&T>, route: SupportRoute, route_of: F) -> bool
where
    F: FnOnce(&T) -> SupportRoute,
{
    data.is_none_or(|data| route_of(data) == route)
}

#[component]
fn SurfaceHeading(route: SupportRoute, title: String, description: String) -> Element {
    rsx! {
        header { class: "ui-support-heading",
            div {
                span { class: "ui-eyebrow", "{route.template()}" }
                h2 { "{title}" }
                p { "{description}" }
            }
            span { class: "ui-readonly-pill", "READ-ONLY" }
        }
    }
}

#[component]
fn SupportRecord(model: SupportDisplayModel) -> Element {
    let locale = use_locale();
    let visible_fields = visible_support_fields(&model, locale.current());
    rsx! {
        article { class: "ui-support-card",
            header { class: "ui-support-card-heading",
                div {
                    span { class: "ui-eyebrow", "SAFE SUPPORT PROJECTION" }
                    h3 { "{model.route_label(locale.current())}" }
                }
                span { class: "ui-support-status", "{model.context.approval.label(locale.current())}" }
            }
            div { class: "ui-support-badges",
                span { class: "ui-support-badge", "{model.context.mode.label(locale.current())}" }
                if model.context.is_read_only() {
                    span { class: "ui-support-badge readonly", "{locale.text(TranslationKey::ReadOnly)}" }
                }
                if model.metadata.is_stale() {
                    span { class: "ui-support-badge stale", "{locale.text(TranslationKey::Stale)}" }
                }
            }
            if visible_fields.is_empty() {
                p { class: "ui-safe-note", "No safe support fields are available in this projection." }
            } else {
                dl { class: "ui-support-fields",
                    for field in visible_fields {
                        dt { "{field.0}" }
                        dd { "{field.1}" }
                    }
                }
            }
            p { class: "ui-safe-note", "Only server-authorized, redacted display fields are shown. Support approval and mutation remain outside this presentation component." }
        }
    }
}

#[component]
fn IncidentListContent(model: crate::IncidentListDisplayModel) -> Element {
    let locale = use_locale();
    rsx! {
        div { class: "ui-incident-list",
            if model.items().is_empty() {
                p { class: "ui-safe-note", "No platform incidents are present in this projection." }
            } else {
                for item in model.items() {
                    article { class: "ui-incident-row",
                        div { class: "ui-incident-row-main",
                            h3 { class: "ui-incident-row-title",
                                if let Some(path) = incident_path(item.id()) {
                                    a { href: "{path}", "{item.id()}" }
                                } else {
                                    strong { "redacted incident reference" }
                                }
                                span { class: "ui-support-status", "{item.status_label(locale.current())}" }
                            }
                            div { class: "ui-incident-chip-list",
                                for service in item.affected_services().iter().filter_map(|value| safe_display_value(value)) {
                                    span { class: "ui-support-badge", "{service}" }
                                }
                            }
                            p { class: "ui-safe-note", "{incident_summary(item)}" }
                        }
                        span { class: "ui-incident-version", "v{item.version()}" }
                    }
                }
            }
        }
    }
}

#[component]
fn IncidentDetailContent(model: IncidentDisplayModel) -> Element {
    let locale = use_locale();
    let incident_label =
        safe_display_value(model.id()).unwrap_or_else(|| "redacted incident reference".to_string());
    let services = safe_values(model.affected_services());
    let customers = safe_values(model.affected_customer_accounts());
    let leads = safe_values(model.leads());
    rsx! {
        article { class: "ui-support-card ui-incident-detail",
            header { class: "ui-support-card-heading",
                div {
                    span { class: "ui-eyebrow", "SAFE INCIDENT PROJECTION" }
                    h3 { "{incident_label}" }
                }
                span { class: "ui-support-status", "{model.status_label(locale.current())}" }
            }
            div { class: "ui-incident-detail-grid",
                SafeValueGroup { label: "Commander", values: model.commander_ref().into_iter().filter_map(safe_display_value).collect() }
                SafeValueGroup { label: "Leads", values: leads }
                SafeValueGroup { label: "Affected services", values: services }
                SafeValueGroup { label: "Affected customer accounts", values: customers }
            }
            p { class: "ui-safe-note", "Aggregate version v{model.version()} · platform scope · read-only projection. Timeline, broadcasts, mitigation, root-cause, corrective-action, and evidence are not rendered by this surface." }
        }
    }
}

#[component]
fn SafeValueGroup(label: &'static str, values: Vec<String>) -> Element {
    rsx! {
        section { class: "ui-safe-value-group",
            h4 { "{label}" }
            if values.is_empty() {
                span { class: "ui-safe-note", "—" }
            } else {
                ul {
                    for value in values {
                        li { "{value}" }
                    }
                }
            }
        }
    }
}

fn support_route_description(route: SupportRoute, locale: crate::Locale) -> &'static str {
    match (route, locale) {
        (SupportRoute::PlatformSupport, crate::Locale::Th) => {
            "คิวสนับสนุนของแพลตฟอร์มจากข้อมูลอ่านที่ได้รับอนุญาต"
        }
        (SupportRoute::PlatformSupport, crate::Locale::En) => {
            "Platform support queue from an authorized read projection."
        }
        (SupportRoute::PlatformIncidents, crate::Locale::Th) => {
            "รายการเหตุขัดข้องของแพลตฟอร์มจากข้อมูลอ่านที่ตรวจสอบสิทธิ์แล้ว"
        }
        (SupportRoute::PlatformIncidents, crate::Locale::En) => {
            "Platform incident queue from an authorized read projection."
        }
        (_, crate::Locale::Th) => "พื้นผิวสนับสนุนแบบอ่านอย่างเดียวจากข้อมูลที่ได้รับอนุญาต",
        (_, crate::Locale::En) => {
            "Read-only support projection from an authorized transport boundary."
        }
    }
}

fn support_state_message(state: &SupportState) -> String {
    match state {
        SupportViewState::Loading { .. } => "Fetching an authorized support projection. No commands are available while loading.".into(),
        SupportViewState::Empty { .. } => "The authorized support projection contains no records.".into(),
        SupportViewState::Denied { .. } => "The server denied this support read. Contact an eligible administrator if access is required.".into(),
        SupportViewState::Stale { age_seconds, .. } => format!("The cached support projection is {age_seconds} seconds old. Verify freshness before relying on it."),
        SupportViewState::ReadOnly { .. } => "This support projection is explicitly read-only; no mutation controls are exposed.".into(),
        SupportViewState::Conflict { .. } => "A concurrent change requires a fresh authorized read. No retry or mutation is issued here.".into(),
        SupportViewState::Offline { .. } => "The support service is unavailable. Cached content, when present, remains read-only.".into(),
        SupportViewState::Error { .. } => "The support projection could not be loaded. Backend details are not rendered in this surface.".into(),
        SupportViewState::Ready { .. } => String::new(),
    }
}

fn support_ticket_list_state_message<T>(state: &SupportViewState<T>) -> String {
    match state {
        SupportViewState::Loading { .. } => "Fetching authorized support ticket metadata. No commands are available while loading.".into(),
        SupportViewState::Empty { .. } => "The authorized support ticket projection contains no records.".into(),
        SupportViewState::Denied { .. } => "The server denied this support ticket read.".into(),
        SupportViewState::Stale { age_seconds, .. } => format!("The cached support ticket projection is {age_seconds} seconds old. Verify freshness before relying on it."),
        SupportViewState::ReadOnly { .. } => "This support ticket projection is explicitly read-only; ticket bodies and mutation controls are not exposed.".into(),
        SupportViewState::Conflict { .. } => "A concurrent change requires a fresh authorized support ticket read.".into(),
        SupportViewState::Offline { .. } => "The support ticket service is unavailable. Cached metadata, when present, remains read-only.".into(),
        SupportViewState::Error { .. } => "The support ticket projection could not be loaded. Backend details are not rendered.".into(),
        SupportViewState::Ready { .. } => String::new(),
    }
}

fn incident_state_message<T>(state: &crate::IncidentViewState<T>) -> String {
    match state {
        crate::IncidentViewState::Loading { .. } => "Fetching the platform incident projection. No commands are available while loading.".into(),
        crate::IncidentViewState::Empty { .. } => "The authorized incident projection contains no records.".into(),
        crate::IncidentViewState::Denied { .. } => "The server denied this incident read. Platform scope authorization remains authoritative.".into(),
        crate::IncidentViewState::Stale { age_seconds, .. } => format!("The cached incident projection is {age_seconds} seconds old. Verify freshness before relying on it."),
        crate::IncidentViewState::ReadOnly { .. } => "This incident projection is explicitly read-only; no incident commands are exposed.".into(),
        crate::IncidentViewState::Conflict { .. } => "A concurrent change requires a fresh authorized read. No retry or mutation is issued here.".into(),
        crate::IncidentViewState::Offline { .. } => "The incident service is unavailable. Cached content, when present, remains read-only.".into(),
        crate::IncidentViewState::Error { .. } => "The incident projection could not be loaded. Backend details are not rendered in this surface.".into(),
        crate::IncidentViewState::Ready { .. } => String::new(),
    }
}

fn visible_support_fields(
    model: &SupportDisplayModel,
    locale: crate::Locale,
) -> Vec<(String, String)> {
    model
        .fields()
        .iter()
        .filter_map(|field| {
            safe_display_value(field.value())
                .map(|value| (support_field_label(field.key(), locale), value))
        })
        .collect()
}

fn support_field_label(key: &str, locale: crate::Locale) -> String {
    let english = match key {
        "ticket_id" => "Ticket",
        "incident_id" => "Incident",
        "subject" => "Subject",
        "status" => "Status",
        "severity" => "Severity",
        "category" => "Category",
        "affected_user_count" => "Affected users",
        "environment" => "Environment",
        "scope_label" => "Scope",
        "requester_label" => "Requester",
        "assignee_label" => "Assignee",
        "commander_label" => "Commander",
        "created_at" => "Created",
        "updated_at" => "Updated",
        "sla_due_at" => "SLA due",
        "resolution_summary" => "Resolution",
        "affected_service" => "Affected service",
        "diagnostic_version" => "Diagnostic version",
        "projection_lag_seconds" => "Projection lag (s)",
        "worker_health" => "Worker health",
        "backup_health" => "Backup health",
        "integration_error_count" => "Integration errors",
        "correlation_id" => "Correlation",
        _ => "Support field",
    };
    if locale == crate::Locale::En {
        english.to_string()
    } else {
        match key {
            "ticket_id" => "ทิกเก็ต",
            "incident_id" => "เหตุขัดข้อง",
            "subject" => "หัวข้อ",
            "status" => "สถานะ",
            "severity" => "ความรุนแรง",
            "category" => "หมวดหมู่",
            "affected_user_count" => "ผู้ใช้ที่ได้รับผลกระทบ",
            "environment" => "สภาพแวดล้อม",
            "scope_label" => "ขอบเขต",
            "requester_label" => "ผู้ร้องขอ",
            "assignee_label" => "ผู้รับผิดชอบ",
            "commander_label" => "ผู้บัญชาการเหตุการณ์",
            "created_at" => "สร้างเมื่อ",
            "updated_at" => "ปรับปรุงเมื่อ",
            "sla_due_at" => "กำหนด SLA",
            "resolution_summary" => "สรุปการแก้ไข",
            "affected_service" => "บริการที่ได้รับผลกระทบ",
            "diagnostic_version" => "เวอร์ชันการวินิจฉัย",
            "projection_lag_seconds" => "ความล่าช้าข้อมูลอ่าน (วินาที)",
            "worker_health" => "สุขภาพ Worker",
            "backup_health" => "สุขภาพสำรองข้อมูล",
            "integration_error_count" => "ข้อผิดพลาดการเชื่อมต่อ",
            "correlation_id" => "รหัสติดตาม",
            _ => "ฟิลด์สนับสนุน",
        }
        .to_string()
    }
}

fn incident_summary(item: &crate::IncidentListItem) -> String {
    let service_count = item
        .affected_services()
        .iter()
        .filter(|value| safe_display_value(value).is_some())
        .count();
    let customer_count = item
        .affected_customer_accounts()
        .iter()
        .filter(|value| safe_display_value(value).is_some())
        .count();
    format!("{service_count} affected service(s) · {customer_count} affected customer account(s)")
}

fn safe_values(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| safe_display_value(value))
        .collect()
}

fn safe_display_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 512
        || trimmed.chars().any(char::is_control)
        || contains_sensitive_marker(trimmed)
    {
        None
    } else {
        Some(
            trimmed
                .chars()
                .filter(|character| !character.is_control())
                .collect(),
        )
    }
}

fn contains_sensitive_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "access_token",
        "refresh_token",
        "id_token",
        "token=",
        "bearer ",
        "secret",
        "credential",
        "password",
        "api_key",
        "private_key",
        "signed_url",
        "presigned_url",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || lower == "token"
}

fn incident_path(id: &str) -> Option<String> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || id.contains(['/', '\\', '?', '#'])
        || contains_sensitive_marker(id)
    {
        None
    } else {
        Some(format!(
            "{}/{}",
            SupportRoute::PlatformIncidents
                .template()
                .trim_end_matches("/:id"),
            id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        IncidentDtoInput, IncidentStatus, Locale, ProjectionFreshness, SupportApprovalState,
        SupportContextMetadata, SupportFieldInput, SupportMode, SupportProjectionMetadata,
        SupportRouteAudience,
    };

    fn support_model() -> SupportDisplayModel {
        SupportDisplayModel::from_untrusted_fields(
            SupportRoute::PlatformSupportDetail,
            SupportProjectionMetadata::fresh("2026-07-24T00:00:00Z"),
            SupportContextMetadata::new(
                SupportMode::DiagnosticRead,
                SupportApprovalState::Approved,
                crate::SupportImpersonationState::NotActive,
                true,
                Some("2026-07-24T08:00:00Z"),
            ),
            [
                SupportFieldInput::new("subject", "Projection lag"),
                SupportFieldInput::new("diagnostic_version", "v1"),
                SupportFieldInput::new("subject", "access_token=do-not-render"),
                SupportFieldInput::new("subject", "token"),
                SupportFieldInput::new("api_key", "hidden"),
            ],
        )
    }

    #[test]
    fn every_support_route_has_a_route_tab_and_safe_surface_description() {
        assert_eq!(SupportRoute::all().len(), 13);
        for definition in SupportRoute::all() {
            assert!(!definition.template.is_empty());
            assert!(!support_route_description(definition.route, Locale::En).is_empty());
            assert!(!support_route_description(definition.route, Locale::Th).is_empty());
        }
    }

    #[test]
    fn route_tabs_and_support_models_remain_bound_to_one_audience_and_route() {
        let platform_routes: Vec<_> = route_definitions_for_audience(SupportRoute::PlatformSupport)
            .map(|definition| definition.route)
            .collect();
        assert_eq!(platform_routes.len(), 8);
        assert!(
            platform_routes
                .iter()
                .all(|route| route.audience() == SupportRouteAudience::Platform)
        );

        let tenant_routes: Vec<_> = route_definitions_for_audience(SupportRoute::TenantSupport)
            .map(|definition| definition.route)
            .collect();
        assert_eq!(tenant_routes.len(), 5);
        assert!(
            tenant_routes
                .iter()
                .all(|route| route.audience() == SupportRouteAudience::Tenant)
        );

        let ready = SupportState::Ready {
            data: support_model(),
            metadata: SupportProjectionMetadata::fresh("2026-07-24T00:00:00Z"),
        };
        assert!(support_state_matches_route(
            SupportRoute::PlatformSupportDetail,
            &ready
        ));
        assert!(!support_state_matches_route(
            SupportRoute::TenantSupportDetail,
            &ready
        ));
    }

    #[test]
    fn tenant_support_payloads_fail_closed_without_matching_explicit_scope() {
        let context = SupportContextMetadata::new(
            SupportMode::DiagnosticRead,
            SupportApprovalState::Approved,
            crate::SupportImpersonationState::NotActive,
            true,
            None::<String>,
        );
        let unscoped = SupportDisplayModel::new(
            SupportRoute::TenantSupport,
            SupportProjectionMetadata::unknown(),
            context.clone(),
        );
        assert!(!support_state_matches_route(
            SupportRoute::TenantSupport,
            &SupportState::Ready {
                data: unscoped,
                metadata: SupportProjectionMetadata::unknown(),
            }
        ));

        let state_metadata = SupportProjectionMetadata::fresh_for_scope(
            "holding-1",
            "tenant-1",
            "2026-07-24T00:00:00Z",
        );
        let model_metadata = SupportProjectionMetadata::fresh_for_scope(
            "holding-1",
            "tenant-2",
            "2026-07-24T00:00:00Z",
        );
        let mismatched =
            SupportDisplayModel::new(SupportRoute::TenantSupport, model_metadata, context);
        assert!(!support_state_matches_route(
            SupportRoute::TenantSupport,
            &SupportState::Ready {
                data: mismatched,
                metadata: state_metadata,
            }
        ));
    }

    #[test]
    fn visible_support_fields_drop_secret_and_token_material_again() {
        let visible = visible_support_fields(&support_model(), Locale::En);
        assert_eq!(
            visible,
            vec![
                ("Subject".into(), "Projection lag".into()),
                ("Diagnostic version".into(), "v1".into())
            ]
        );
        let rendered = format!("{visible:?}");
        assert!(!rendered.contains("access_token"));
        assert!(!rendered.contains("do-not-render"));
        assert!(!rendered.contains("api_key"));
    }

    #[test]
    fn incident_paths_are_safe_and_never_turn_credentials_into_links() {
        assert_eq!(
            incident_path("inc-001"),
            Some("/platform/incidents/inc-001".into())
        );
        assert_eq!(incident_path("inc/001"), None);
        assert_eq!(incident_path("access_token=secret"), None);
        assert_eq!(incident_path(""), None);
    }

    #[test]
    fn incident_display_values_redact_token_markers() {
        assert_eq!(safe_display_value("API_KEY=secret"), None);
        assert_eq!(safe_display_value("Bearer abc"), None);
        assert_eq!(safe_display_value("token"), None);
        assert_eq!(
            safe_display_value("checkout-api"),
            Some("checkout-api".into())
        );
    }

    #[test]
    fn state_kind_is_preserved_for_cached_and_denied_support_states() {
        let metadata =
            SupportProjectionMetadata::present("2026-07-24T00:00:00Z", ProjectionFreshness::Stale);
        let stale = SupportState::Stale {
            data: support_model(),
            metadata,
            age_seconds: 600,
            reason: "projection lag".into(),
        };
        assert_eq!(stale.kind(), Some(UiStateKind::Stale));
        assert!(stale.data().is_some());
        let denied = SupportState::Denied {
            metadata: None,
            reason: "server policy".into(),
        };
        assert_eq!(denied.kind(), Some(UiStateKind::Denied));
        assert!(denied.data().is_none());
    }

    #[test]
    fn typed_incident_model_remains_platform_scoped_for_the_detail_surface() {
        let input = IncidentDtoInput::new(
            "incident-001",
            IncidentStatus::Investigating.as_str(),
            "commander-001",
            ["lead-001"],
            ["api"],
            ["customer-001"],
            3,
        );
        let detail = IncidentDisplayModel::from_safe_dto(
            crate::IncidentProjectionMetadata::present(
                "2026-07-24T00:00:00Z",
                ProjectionFreshness::Fresh,
            ),
            input,
        )
        .expect("safe incident detail");
        assert_eq!(detail.route, SupportRoute::PlatformIncidentDetail);
        assert!(detail.metadata.scope.is_platform());
    }
}
