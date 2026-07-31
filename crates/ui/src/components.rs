use std::rc::Rc;

use dioxus::{dioxus_core::current_scope_id, prelude::*};

use crate::{
    Locale, LocaleContext, NavItem, RouteState, SafeSessionMetadata, SafeUiText, ScopeOption,
    TranslationKey, UiStateKind, redact_deep_link, use_locale, use_theme,
};

/// A top-level responsive shell shared by web, desktop, and mobile clients.
#[component]
pub fn AppShell(
    /// Product/tenant-facing brand name.
    brand: String,
    /// Short context subtitle.
    subtitle: String,
    /// Navigation entries rendered in the responsive rail.
    nav_items: Vec<NavItem>,
    /// Current normalized path.
    active_path: String,
    /// Optional read-only/support/impersonation context banner.
    context_banner: Option<BannerKind>,
    /// Optional security or freshness note displayed in the header.
    header_note: Option<String>,
    /// Safe, non-secret metadata supplied by the authenticated application boundary.
    session_metadata: Option<SafeSessionMetadata>,
    /// Scope options supplied by the authenticated application boundary.
    scope_options: Vec<ScopeOption>,
    /// Selected non-secret scope key.
    selected_scope: String,
    /// Callback for requesting a scope change. The server remains authoritative.
    on_scope_change: EventHandler<String>,
    /// Page content.
    children: Element,
) -> Element {
    let locale = use_locale();
    let theme = use_theme();
    let current_locale = locale.current();
    let theme_token = theme.current().css_token();
    let brand = safe_display_text(&brand);
    let subtitle = safe_display_text(&subtitle);
    let active_path = redact_deep_link(&active_path);
    let header_note = header_note.map(|note| safe_display_text(&note));
    let nav_items = nav_items
        .into_iter()
        .map(|item| NavItem {
            label: safe_display_text(&item.label),
            path: redact_deep_link(&item.path),
            icon: safe_display_text(&item.icon),
        })
        .collect::<Vec<_>>();

    rsx! {
        div {
            class: "ui-shell ui-theme-{theme_token}",
            "data-locale": current_locale.language_tag(),
            "data-theme": theme_token,
            nav {
                class: "ui-nav",
                aria_label: locale.text(TranslationKey::Navigation),
                div { class: "ui-nav-brand",
                    span { class: "ui-brand-mark", aria_hidden: "true", "E" }
                    div {
                        strong { "{brand}" }
                        span { "{subtitle}" }
                    }
                }
                div { class: "ui-nav-items",
                    for item in nav_items {
                        a {
                            class: if is_active(&active_path, &item.path) { "ui-nav-item active" } else { "ui-nav-item" },
                            href: "{item.path}",
                            aria_current: if is_active(&active_path, &item.path) { "page" } else { "false" },
                            span { class: "ui-nav-icon", aria_hidden: "true", "{item.icon}" }
                            span { "{item.label}" }
                        }
                    }
                }
                div { class: "ui-nav-footer",
                    LocaleToggle {}
                    span { class: "ui-locale-name", "{current_locale.native_name()}" }
                }
            }
            main { class: "ui-main",
                header { class: "ui-topbar",
                    div {
                        span { class: "ui-eyebrow", "ERPX CLIENT" }
                        h1 { "{brand}" }
                    }
                    div { class: "ui-topbar-actions",
                        if let Some(note) = header_note {
                            span { class: "ui-header-note", "{note}" }
                        }
                        ScopeSelector {
                            label: locale.text(TranslationKey::Scope).to_string(),
                            options: scope_options,
                            selected: selected_scope,
                            on_change: on_scope_change,
                        }
                    }
                }
                if let Some(metadata) = session_metadata {
                    SessionBanners { metadata }
                }
                if let Some(banner) = context_banner {
                    ContextBanner { kind: banner }
                }
                {children}
            }
        }
    }
}

/// A safe scope selector surface. Selecting a value only emits a request to the consuming app;
/// it does not grant access or place tenant credentials in the client.
#[component]
pub fn ScopeSelector(
    label: String,
    options: Vec<ScopeOption>,
    selected: String,
    on_change: EventHandler<String>,
) -> Element {
    let label = safe_display_text(&label);
    // The selected value is still only a server request, but a browser/native event can be
    // forged independently of the rendered option list. Keep the event boundary allow-listed so
    // an arbitrary DOM value never reaches the application's scope-change handoff.
    let mut allowed_ids = Vec::new();
    let options = options
        .into_iter()
        .filter_map(|option| {
            let id = safe_opaque_value(&option.id);
            if id.is_empty() || allowed_ids.iter().any(|known| known == &id) {
                return None;
            }
            allowed_ids.push(id.clone());
            Some(ScopeOption {
                id,
                label: safe_display_text(&option.label),
                kind: option.kind,
            })
        })
        .collect::<Vec<_>>();
    let selected = allowed_scope_selection(&selected, &allowed_ids).unwrap_or_default();
    rsx! {
        div { class: "ui-scope-selector",
            label { r#for: "erpx-scope-selector", class: "ui-scope-label", "{label}" }
            select {
                id: "erpx-scope-selector",
                value: "{selected}",
                aria_label: "{label}",
                onchange: move |event| {
                    if let Some(value) = allowed_scope_selection(&event.value(), &allowed_ids) {
                        on_change.call(value);
                    }
                },
                for option in options {
                    option { value: "{option.id}", "{option.label}" }
                }
            }
        }
    }
}

/// Render persistent support, impersonation, and read-only banners from safe session metadata.
#[component]
pub fn SessionBanners(metadata: SafeSessionMetadata) -> Element {
    rsx! {
        div { class: "ui-session-banners", aria_live: "polite",
            if let Some(support) = metadata.support.as_ref() {
                ContextBanner {
                    kind: BannerKind::Support {
                        message: format!("{} · {}", support.ticket_label, support.mode_label),
                        expires_at: support.expires_at.clone(),
                    }
                }
            }
            if let Some(impersonation) = metadata.impersonation.as_ref() {
                ContextBanner {
                    kind: BannerKind::Impersonation {
                        message: format!(
                            "Representing {} · {}",
                            impersonation.represented_label,
                            impersonation.reason
                        ),
                    }
                }
            }
            if let Some(read_only) = metadata.read_only.as_ref() {
                ContextBanner {
                    kind: BannerKind::ReadOnly { message: read_only.reason.clone() }
                }
            }
        }
    }
}

/// Contexts which must stay visible while a user navigates the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BannerKind {
    /// Mutations are blocked by billing or policy.
    ReadOnly { message: String },
    /// An approved support capability is active.
    Support { message: String, expires_at: String },
    /// An impersonated represented-user context is active.
    Impersonation { message: String },
    /// The read model is beyond its freshness budget.
    Stale { message: String },
}

#[component]
fn ContextBanner(kind: BannerKind) -> Element {
    let (class, label, message) = match kind {
        BannerKind::ReadOnly { message } => ("readonly", "READ-ONLY", message),
        BannerKind::Support {
            message,
            expires_at,
        } => (
            "support",
            "SUPPORT ACCESS",
            format!("{message} · Expires {expires_at}"),
        ),
        BannerKind::Impersonation { message } => ("impersonation", "IMPERSONATION", message),
        BannerKind::Stale { message } => ("stale", "STALE READ MODEL", message),
    };
    let message = safe_banner_text(&message);
    rsx! {
        aside { class: "ui-context-banner {class}", role: "status",
            strong { "{label}" }
            span { "{message}" }
        }
    }
}

fn safe_banner_text(value: &str) -> String {
    safe_display_text(value)
}

fn safe_display_text(value: &str) -> String {
    SafeUiText::new(value).as_str().to_owned()
}

/// Opaque IDs are placed in DOM attributes as well as being sent back to the application
/// boundary. Never allow a credential-bearing value into either path.
fn safe_opaque_value(value: &str) -> String {
    let safe = SafeUiText::new(value);
    if safe.as_str() == "[redacted]" {
        String::new()
    } else {
        safe.as_str().to_owned()
    }
}

fn allowed_scope_selection(value: &str, allowed_ids: &[String]) -> Option<String> {
    allowed_ids
        .iter()
        .find(|allowed| allowed.as_str() == value)
        .cloned()
}

fn safe_input_value(value: &str) -> String {
    safe_opaque_value(value)
}

/// Return an action label only when it remains a usable, non-sensitive accessible name.
///
/// A redacted label must not become an interactive control: `[redacted]` gives the user no
/// actionable meaning and can hide that an untrusted diagnostic was supplied as a label. Empty
/// and whitespace-only labels are likewise omitted so every rendered action has a meaningful
/// accessible name.
fn safe_action_label(value: String) -> Option<SafeUiText> {
    let safe = SafeUiText::new(value);
    (!safe.as_str().is_empty() && safe.as_str() != "[redacted]").then_some(safe)
}

fn safe_field_name(value: &str) -> String {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        "field".to_owned()
    } else {
        value.to_owned()
    }
}

/// A translated locale toggle for the shared shell footer or settings page.
#[component]
pub fn LocaleToggle() -> Element {
    let locale = use_locale();
    let current = locale.current();
    rsx! {
        button {
            class: "ui-icon-button",
            r#type: "button",
            aria_label: locale.text(TranslationKey::SwitchLanguage),
            onclick: move |_| locale.set(current.toggled()),
            "{current.toggled().native_name()}"
        }
    }
}

/// A consistent page title block.
#[component]
pub fn PageHeader(eyebrow: String, title: String, description: String) -> Element {
    let eyebrow = safe_display_text(&eyebrow);
    let title = safe_display_text(&title);
    let description = safe_display_text(&description);
    rsx! {
        header { class: "ui-page-header",
            span { class: "ui-eyebrow", "{eyebrow}" }
            h2 { "{title}" }
            p { "{description}" }
        }
    }
}

/// A semantic filter row that keeps controls grouped for keyboard and screen-reader users.
#[component]
pub fn FilterBar(aria_label: String, children: Element) -> Element {
    let aria_label = safe_display_text(&aria_label);
    rsx! {
        section { class: "ui-filter-bar", aria_label: "{aria_label}", role: "search",
            {children}
        }
    }
}

/// A labeled text input with a stable field id and visible validation/help text.
#[component]
pub fn TextField(
    label: String,
    name: String,
    value: String,
    placeholder: Option<String>,
    error: Option<String>,
    help: Option<String>,
    required: bool,
    oninput: EventHandler<FormEvent>,
) -> Element {
    let name = safe_field_name(&name);
    let label = safe_display_text(&label);
    let value = safe_input_value(&value);
    let placeholder = placeholder.map(|value| safe_display_text(&value));
    let error = error.map(|value| safe_display_text(&value));
    let help = help.map(|value| safe_display_text(&value));
    let scope_id = current_scope_id().0;
    let input_id = format!("erpx-field-{scope_id}-{name}");
    let error_id = format!("{input_id}-error");
    let help_id = format!("{input_id}-help");
    let has_error = error.is_some();
    let described_by = match (&error, &help) {
        (Some(_), Some(_)) => format!("{error_id} {help_id}"),
        (Some(_), None) => error_id.clone(),
        (None, Some(_)) => help_id.clone(),
        (None, None) => String::new(),
    };
    let described_by = (!described_by.is_empty()).then_some(described_by);
    rsx! {
        div { class: "ui-field",
            label { r#for: "{input_id}", class: "ui-field-label",
                "{label}"
                if required { span { class: "ui-required", aria_label: "required", " *" } }
            }
            input {
                id: "{input_id}",
                name: "{name}",
                r#type: "text",
                value: "{value}",
                placeholder: placeholder,
                required: required,
                aria_invalid: if has_error { "true" } else { "false" },
                aria_describedby: described_by,
                oninput: move |event| oninput.call(event),
            }
            if let Some(help) = help {
                small { id: "{help_id}", class: "ui-field-help", "{help}" }
            }
            if let Some(error) = &error {
                small { id: "{error_id}", class: "ui-field-error", role: "alert", "{error}" }
            }
        }
    }
}

/// A field wrapper for select, date, upload, and other custom controls.
#[component]
pub fn Field(
    label: String,
    name: String,
    error: Option<String>,
    help: Option<String>,
    required: bool,
    children: Element,
) -> Element {
    let name = safe_field_name(&name);
    let label = safe_display_text(&label);
    let error = error.map(|value| safe_display_text(&value));
    let help = help.map(|value| safe_display_text(&value));
    let scope_id = current_scope_id().0;
    let label_id = format!("erpx-field-{scope_id}-{name}-label");
    let error_id = format!("erpx-field-{scope_id}-{name}-error");
    let help_id = format!("erpx-field-{scope_id}-{name}-help");
    let described_by = match (&error, &help) {
        (Some(_), Some(_)) => format!("{error_id} {help_id}"),
        (Some(_), None) => error_id.clone(),
        (None, Some(_)) => help_id.clone(),
        (None, None) => String::new(),
    };
    let described_by = (!described_by.is_empty()).then_some(described_by);
    rsx! {
        div {
            class: "ui-field",
            role: "group",
            aria_labelledby: "{label_id}",
            aria_describedby: described_by,
            label { id: "{label_id}", class: "ui-field-label",
                "{label}"
                if required { span { class: "ui-required", aria_label: "required", " *" } }
            }
            {children}
            if let Some(help) = help {
                small { id: "{help_id}", class: "ui-field-help", "{help}" }
            }
            if let Some(error) = error {
                small { id: "{error_id}", class: "ui-field-error", role: "alert", "{error}" }
            }
        }
    }
}

/// A table header definition used by `DataTable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableColumn {
    /// Stable column id.
    pub id: String,
    /// Visible label.
    pub label: String,
    /// Whether the column is numeric/right-aligned.
    pub numeric: bool,
}

/// A responsive, scrollable data table primitive. Rows remain application-owned RSX so domain
/// types never enter this crate.
#[component]
pub fn DataTable(
    caption: String,
    columns: Vec<TableColumn>,
    empty: bool,
    children: Element,
) -> Element {
    let caption = safe_display_text(&caption);
    let columns = columns
        .into_iter()
        .map(|column| TableColumn {
            id: safe_field_name(&column.id),
            label: safe_display_text(&column.label),
            numeric: column.numeric,
        })
        .collect::<Vec<_>>();
    // A valid table still needs one cell for an empty result when the authoritative response has
    // no column metadata. Rendering `colspan="0"` leaves assistive technology without a stable
    // empty-result cell on several targets.
    let column_count = columns.len().max(1);
    rsx! {
        div { class: "ui-table-wrap",
            table { class: "ui-table",
                caption { class: "sr-only", "{caption}" }
                thead {
                    tr {
                        for column in columns {
                            th { scope: "col", class: if column.numeric { "numeric" } else { "" }, "{column.label}" }
                        }
                    }
                }
                tbody {
                    if empty {
                        tr { td { colspan: "{column_count}", class: "ui-table-empty", "No records" } }
                    } else {
                        {children}
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogKeyboardAction {
    Close,
    FocusNext,
    FocusPrevious,
    Ignore,
}

fn dialog_keyboard_action(key: &str, shift: bool) -> DialogKeyboardAction {
    match key {
        "Escape" => DialogKeyboardAction::Close,
        "Tab" if shift => DialogKeyboardAction::FocusPrevious,
        "Tab" => DialogKeyboardAction::FocusNext,
        _ => DialogKeyboardAction::Ignore,
    }
}

fn focus_mounted_target(target: ReadSignal<Option<Rc<MountedData>>>) {
    if let Some(target) = target.read().clone() {
        spawn(async move {
            let _ = target.set_focus(true).await;
        });
    }
}

fn dialog_close_label() -> &'static str {
    match try_consume_context::<LocaleContext>().map(|locale| locale.current()) {
        Some(Locale::Th) => "ปิด",
        _ => "Close",
    }
}

/// A modal dialog primitive. It renders only while open and labels the dialog for assistive tech.
#[component]
pub fn Dialog(
    open: bool,
    title: String,
    on_close: EventHandler<()>,
    #[props(default)] restore_focus: ReadSignal<Option<Rc<MountedData>>>,
    children: Element,
) -> Element {
    // Scope IDs are stable for the lifetime of a mounted component and avoid duplicate
    // aria-labelledby/aria-describedby targets when a page renders more than one dialog.
    let scope_id = current_scope_id().0;
    let dialog_id = format!("erpx-dialog-{scope_id}");
    let title_id = format!("erpx-dialog-title-{scope_id}");
    let body_id = format!("erpx-dialog-body-{scope_id}");
    let mut close_button = use_signal(|| None::<Rc<MountedData>>);
    let close_button_for_start_guard = close_button;
    let close_button_for_end_guard = close_button;
    let mut previous_open = use_signal(|| open);
    let restore_focus_on_transition = restore_focus;
    use_effect(use_reactive!(|open| {
        let was_open = *previous_open.peek();
        if was_open && !open {
            focus_mounted_target(restore_focus_on_transition);
        }
        previous_open.set(open);
    }));
    let restore_focus_on_drop = restore_focus;
    let previous_open_on_drop = previous_open;
    use_drop(move || {
        if *previous_open_on_drop.peek() {
            focus_mounted_target(restore_focus_on_drop);
        }
    });

    if !open {
        return rsx! {};
    }
    let title = safe_display_text(&title);
    let close_label = dialog_close_label();
    let dialog_role = "dialog";
    let aria_modal = "true";
    let escape_key = "Escape";
    let focus_trap = "true";
    let focus_restore = if restore_focus.read().is_some() {
        "true"
    } else {
        "caller-target-required"
    };
    let focus_guard_label = "Dialog focus boundary";
    let start_focus_guard = "start";
    let end_focus_guard = "end";
    rsx! {
        div {
            class: "ui-dialog-backdrop",
            role: "presentation",
            "data-dialog-backdrop": "{focus_trap}",
            onclick: move |_| on_close.call(()),
            div {
                class: "ui-dialog",
                id: "{dialog_id}",
                role: "{dialog_role}",
                aria_modal: "{aria_modal}",
                aria_labelledby: "{title_id}",
                aria_describedby: "{body_id}",
                aria_keyshortcuts: "{escape_key}",
                "data-dialog-focus-trap": "{focus_trap}",
                "data-dialog-focus-restore": "{focus_restore}",
                tabindex: "-1",
                onclick: move |event| event.stop_propagation(),
                onkeydown: move |event| {
                    let key = event.data.key().to_string();
                    let action = dialog_keyboard_action(&key, event.data.modifiers().shift());
                    match action {
                        DialogKeyboardAction::Close => {
                            event.prevent_default();
                            event.stop_propagation();
                            on_close.call(());
                        }
                        // Native sequential focus moves through the dialog's focus guards. Their
                        // focus handlers wrap both forward and reverse traversal without a
                        // renderer-specific DOM API.
                        DialogKeyboardAction::FocusNext | DialogKeyboardAction::FocusPrevious => {}
                        DialogKeyboardAction::Ignore => {}
                    }
                },
                span {
                    class: "sr-only",
                    tabindex: "0",
                    aria_label: "{focus_guard_label}",
                    "data-dialog-focus-guard": "{start_focus_guard}",
                    onfocus: move |_| async move {
                        if let Some(button) = close_button_for_start_guard.read().clone() {
                            let _ = button.set_focus(true).await;
                        }
                    },
                }
                header { class: "ui-dialog-header",
                    h2 { id: "{title_id}", "{title}" }
                    button {
                        class: "ui-icon-button",
                        r#type: "button",
                        aria_label: "{close_label}",
                        autofocus: true,
                        onmounted: move |event| async move {
                            let button = event.data();
                            close_button.set(Some(button.clone()));
                            let _ = button.set_focus(true).await;
                        },
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }
                section { id: "{body_id}", class: "ui-dialog-body", {children} }
                span {
                    class: "sr-only",
                    tabindex: "0",
                    aria_label: "{focus_guard_label}",
                    "data-dialog-focus-guard": "{end_focus_guard}",
                    onfocus: move |_| async move {
                        if let Some(button) = close_button_for_end_guard.read().clone() {
                            let _ = button.set_focus(true).await;
                        }
                    },
                }
            }
        }
    }
}

/// A compact panel for every non-ready state in `UiStateKind`.
#[component]
pub fn StatePanel(
    state: UiStateKind,
    title: String,
    message: String,
    action_label: Option<String>,
    on_action: Option<EventHandler<MouseEvent>>,
) -> Element {
    let accessibility = state.accessibility();
    let title = SafeUiText::new(title);
    let message = SafeUiText::new(message);
    let action_label = action_label.and_then(safe_action_label);
    // Scope IDs keep the accessible naming relation unique across repeated list/route states.
    let scope_id = current_scope_id().0;
    let title_id = format!("erpx-state-title-{scope_id}");
    let message_id = format!("erpx-state-message-{scope_id}");
    let class = match state {
        UiStateKind::Loading => "loading",
        UiStateKind::Empty => "empty",
        UiStateKind::Denied => "denied",
        UiStateKind::Stale => "stale",
        UiStateKind::ReadOnly => "readonly",
        UiStateKind::Conflict => "conflict",
        UiStateKind::Offline => "offline",
        UiStateKind::Error => "error",
    };
    rsx! {
        section {
            class: "ui-state-panel {class}",
            "data-ui-state": state.dom_key(),
            "data-ui-authorization": state.authorization_key(),
            "data-ui-cached-payload": if accessibility.cached_payload_allowed { "allowed" } else { "hidden" },
            role: accessibility.role,
            aria_live: accessibility.aria_live,
            aria_atomic: accessibility.aria_atomic.to_string(),
            aria_busy: accessibility.aria_busy.to_string(),
            aria_labelledby: "{title_id}",
            aria_describedby: "{message_id}",
            div { class: "ui-state-icon", aria_hidden: "true",
                match state {
                    UiStateKind::Loading => "◌",
                    UiStateKind::Empty => "○",
                    UiStateKind::Denied => "⊘",
                    UiStateKind::Stale => "◷",
                    UiStateKind::ReadOnly => "▣",
                    UiStateKind::Conflict => "↯",
                    UiStateKind::Offline => "⌁",
                    UiStateKind::Error => "!",
                }
            }
            h3 { id: "{title_id}", "{title.as_str()}" }
            p { id: "{message_id}", "{message.as_str()}" }
            if let (Some(label), Some(on_action)) = (action_label, on_action) {
                button {
                    class: "ui-button secondary",
                    r#type: "button",
                    aria_label: "{label.as_str()}",
                    onclick: move |event| on_action.call(event),
                    "{label.as_str()}"
                }
            }
        }
    }
}

/// Render children only when the state is ready, otherwise use the explicit state panel.
#[component]
pub fn StateBoundary(
    state: Option<UiStateKind>,
    title: String,
    message: String,
    children: Element,
) -> Element {
    if let Some(state) = state {
        return rsx! { StatePanel { state, title, message, action_label: None, on_action: None } };
    }
    children
}

/// Gate route content using an externally supplied authentication/session state. This component
/// never authenticates, refreshes, or stores credentials.
#[component]
pub fn RouteStateBoundary(state: RouteState, children: Element) -> Element {
    match state {
        RouteState::Authenticated { session } => rsx! {
            SessionBanners { metadata: session }
            {children}
        },
        RouteState::ReadOnly { session, reason } => rsx! {
            SessionBanners { metadata: session }
            ContextBanner { kind: BannerKind::ReadOnly { message: reason } }
            {children}
        },
        RouteState::Unauthenticated { return_to } => route_state_panel(
            UiStateKind::Denied,
            "Sign-in required",
            format!("Sign in to continue to {}.", redact_deep_link(&return_to)),
        ),
        RouteState::SessionExpired { return_to } => route_state_panel(
            UiStateKind::Denied,
            "Session expired",
            format!(
                "Sign in again to return to {}.",
                redact_deep_link(&return_to)
            ),
        ),
        RouteState::SessionRevoked { return_to } => route_state_panel(
            UiStateKind::Denied,
            "Session revoked",
            format!(
                "This session is no longer active. Return path: {}.",
                redact_deep_link(&return_to)
            ),
        ),
        RouteState::Offline {
            cached_view_available,
            reason,
        } => route_state_panel(
            UiStateKind::Offline,
            "Offline",
            if cached_view_available {
                format!("{reason} Cached content may be available; writes remain disabled.")
            } else {
                format!("{reason} No cached content is available.")
            },
        ),
        RouteState::Denied { reason } => {
            route_state_panel(UiStateKind::Denied, "Access denied", reason)
        }
        RouteState::Stale {
            age_seconds,
            reason,
        } => route_state_panel(
            UiStateKind::Stale,
            "Read model may be stale",
            format!("{reason} Last known projection age: {age_seconds} seconds."),
        ),
        RouteState::Conflict { reason } => {
            route_state_panel(UiStateKind::Conflict, "Changes need review", reason)
        }
    }
}

fn route_state_panel(state: UiStateKind, title: &str, message: String) -> Element {
    rsx! {
        StatePanel {
            state,
            title: title.to_string(),
            message,
            action_label: None,
            on_action: None,
        }
    }
}

fn is_active(active_path: &str, item_path: &str) -> bool {
    let active_path = crate::normalize_path(active_path);
    let item_path = crate::normalize_path(item_path);
    active_path == item_path
        || (item_path != "/"
            && active_path.starts_with(item_path)
            && active_path.as_bytes().get(item_path.len()) == Some(&b'/'))
}

#[cfg(test)]
mod tests {
    use super::{
        DialogKeyboardAction, SafeUiText, allowed_scope_selection, dialog_keyboard_action,
        is_active, safe_action_label, safe_banner_text, safe_field_name, safe_input_value,
        safe_opaque_value,
    };

    #[test]
    fn context_banner_text_is_redacted_and_bounded_before_rendering() {
        assert_eq!(safe_banner_text("access_token=secret"), "[redacted]");
        assert!(safe_banner_text(&"a".repeat(600)).ends_with('…'));
    }

    #[test]
    fn navigation_activation_is_segment_aware() {
        assert!(is_active("/platform/customers", "/platform"));
        assert!(!is_active("/platforming", "/platform"));
        assert!(!is_active("/operations", "/"));
    }

    #[test]
    fn shared_component_display_and_attribute_boundaries_fail_closed() {
        assert_eq!(
            safe_banner_text("Authorization: Bearer top-secret"),
            "[redacted]"
        );
        assert_eq!(safe_opaque_value("refresh_token=top-secret"), "");
        assert_eq!(safe_input_value("postgres://user:secret@example/db"), "");
        assert_eq!(safe_field_name("user name"), "field");
        assert_eq!(safe_field_name("directory_user"), "directory_user");
    }

    #[test]
    fn state_actions_never_render_without_a_safe_accessible_name() {
        assert!(safe_action_label("access_token=top-secret".to_owned()).is_none());
        assert!(safe_action_label("   ".to_owned()).is_none());
        assert!(safe_action_label(String::new()).is_none());

        let label = safe_action_label("Review conflict".to_owned()).expect("safe action label");
        assert_eq!(label.as_str(), "Review conflict");
        assert!(safe_action_label("x".repeat(SafeUiText::MAX_BYTES + 64)).is_some());
    }

    #[test]
    fn scope_selection_only_hands_off_a_rendered_authorized_option() {
        let allowed = vec!["platform".to_owned(), "holding-1".to_owned()];
        assert_eq!(
            allowed_scope_selection("holding-1", &allowed).as_deref(),
            Some("holding-1")
        );
        assert_eq!(allowed_scope_selection("holding-2", &allowed), None);
        assert_eq!(
            allowed_scope_selection("access_token=secret", &allowed),
            None
        );
    }

    #[test]
    fn dialog_keyboard_policy_closes_only_on_escape_and_preserves_tab_direction() {
        assert_eq!(
            dialog_keyboard_action("Escape", false),
            DialogKeyboardAction::Close
        );
        assert_eq!(
            dialog_keyboard_action("Tab", false),
            DialogKeyboardAction::FocusNext
        );
        assert_eq!(
            dialog_keyboard_action("Tab", true),
            DialogKeyboardAction::FocusPrevious
        );
        assert_eq!(
            dialog_keyboard_action("Enter", false),
            DialogKeyboardAction::Ignore
        );
    }
}
