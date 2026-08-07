use std::{collections::BTreeSet, rc::Rc};

use dioxus::{dioxus_core::current_scope_id, prelude::*};
use erpx_domain::corporate_identity::{
    BrandCategory, BrandDensity, BrandElevation, BrandFontPack, BrandMotion, BrandNavigation,
    BrandRadius, BrandTokenSet, HexColor, ResolvedCorporateIdentity,
};

use crate::{
    Locale, LocaleContext, LocalizedText, NavGroup, NavItem, RouteState, SafeSessionMetadata,
    SafeUiText, ScopeOption, ThemeMode, TranslationKey, UiStateKind, corporate_identity_style,
    redact_deep_link, use_corporate_identity, use_locale, use_theme,
};

const fn shared_text(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    LocalizedText::new(en, th).text(locale)
}

fn current_locale() -> Locale {
    try_consume_context::<LocaleContext>()
        .map(|locale| locale.current())
        .unwrap_or(Locale::En)
}

const fn theme_mode_label(locale: Locale, mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => shared_text(locale, "System", "ตามระบบ"),
        ThemeMode::Light => shared_text(locale, "Light", "สว่าง"),
        ThemeMode::Dark => shared_text(locale, "Dark", "มืด"),
    }
}

/// A top-level responsive shell shared by web, desktop, and mobile clients.
#[component]
pub fn AppShell(
    /// Product/tenant-facing brand name.
    brand: String,
    /// Short context subtitle.
    subtitle: String,
    /// Named work areas rendered in the responsive rail.
    nav_groups: Vec<NavGroup>,
    /// Current normalized path.
    active_path: String,
    /// Optional read-only/support/impersonation context banner.
    context_banner: Option<BannerKind>,
    /// Optional security or freshness note displayed in the header.
    header_note: Option<String>,
    /// Safe, non-secret metadata supplied by the authenticated application boundary.
    session_metadata: Option<SafeSessionMetadata>,
    /// Display-safe principal label for the account control.
    profile_principal: String,
    /// Display-safe exact active-role label for the account control.
    profile_role: String,
    /// Scope options supplied by the authenticated application boundary.
    scope_options: Vec<ScopeOption>,
    /// Selected non-secret scope key.
    selected_scope: String,
    /// Callback for requesting a scope change. The server remains authoritative.
    on_scope_change: EventHandler<String>,
    /// Callback for opening the application-owned account and session tools.
    on_account_session: EventHandler<()>,
    /// Callback for client-side navigation. Keeping navigation inside the mounted application
    /// preserves the in-memory session on web targets that intentionally have no browser token
    /// storage fallback.
    on_navigate: EventHandler<String>,
    /// Page content.
    children: Element,
) -> Element {
    let locale = use_locale();
    let theme = use_theme();
    let corporate_identity = use_corporate_identity();
    let identity = corporate_identity.current();
    let current_locale = locale.current();
    let theme_token = theme.current().css_token();
    let fallback_brand = safe_display_text(&brand);
    let brand = safe_display_text(if identity.display_name.is_empty() {
        &fallback_brand
    } else {
        &identity.display_name
    });
    let brand_mark = identity.short_name.chars().take(2).collect::<String>();
    let brand_style = corporate_identity_style(&identity);
    let subtitle = safe_display_text(&subtitle);
    let active_path = redact_deep_link(&active_path);
    let header_note = header_note.map(|note| safe_display_text(&note));
    let profile_principal = safe_display_text(&profile_principal);
    let profile_role = safe_display_text(&profile_role);
    let profile_avatar = profile_principal
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned());
    let nav_groups = nav_groups
        .into_iter()
        .map(|group| NavGroup {
            label: safe_display_text(&group.label),
            items: group
                .items
                .into_iter()
                .map(|item| NavItem {
                    label: safe_display_text(&item.label),
                    path: redact_deep_link(&item.path),
                    icon: safe_display_text(&item.icon),
                    action_capabilities: item.action_capabilities,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let active_context = nav_groups
        .iter()
        .find_map(|group| {
            group
                .items
                .iter()
                .find(|item| is_active(&active_path, &item.path))
                .map(|item| (group.label.clone(), item.label.clone()))
        })
        .unwrap_or_else(|| {
            (
                shared_text(current_locale, "Workspace", "พื้นที่ทำงาน").to_owned(),
                brand.clone(),
            )
        });
    let mut nav_collapsed = use_signal(|| !identity.tokens.navigation_expanded_by_default);
    let mut mobile_nav_open = use_signal(|| false);
    let mut command_open = use_signal(|| false);
    let mut command_query = use_signal(String::new);
    let command_query_value = command_query.read().trim().to_ascii_lowercase();
    let command_items = nav_groups
        .iter()
        .flat_map(|group| group.items.iter())
        .filter(|item| {
            command_query_value.is_empty()
                || item
                    .label
                    .to_ascii_lowercase()
                    .contains(&command_query_value)
                || item
                    .path
                    .to_ascii_lowercase()
                    .contains(&command_query_value)
        })
        .cloned()
        .collect::<Vec<_>>();
    let shell_class = format!(
        "ui-shell ui-theme-{theme_token}{}",
        if *nav_collapsed.read() {
            " nav-collapsed"
        } else {
            ""
        }
    );
    let nav_class = if *mobile_nav_open.read() {
        "ui-nav mobile-open"
    } else {
        "ui-nav"
    };

    rsx! {
        div {
            class: "{shell_class}",
            style: "{brand_style}",
            "data-locale": current_locale.language_tag(),
            "data-theme": theme_token,
            "data-brand-version": identity.published_version.to_string(),
            button {
                class: if *mobile_nav_open.read() { "ui-nav-scrim visible" } else { "ui-nav-scrim" },
                r#type: "button",
                aria_label: shared_text(current_locale, "Close navigation", "ปิดเมนูนำทาง"),
                onclick: move |_| mobile_nav_open.set(false),
            }
            nav {
                class: "{nav_class}",
                aria_label: locale.text(TranslationKey::Navigation),
                div { class: "ui-nav-header",
                    div { class: "ui-nav-brand",
                        span { class: "ui-brand-mark", aria_hidden: "true", "{brand_mark}" }
                        div { class: "ui-brand-copy",
                            strong { "{brand}" }
                            span { "{subtitle}" }
                        }
                    }
                    button {
                        class: "ui-icon-button ui-collapse-button",
                        r#type: "button",
                        aria_label: if *nav_collapsed.read() {
                            shared_text(current_locale, "Expand navigation", "ขยายเมนูนำทาง")
                        } else {
                            shared_text(current_locale, "Collapse navigation", "ย่อเมนูนำทาง")
                        },
                        aria_expanded: (!*nav_collapsed.read()).to_string(),
                        onclick: move |_| {
                            let next = !*nav_collapsed.read();
                            nav_collapsed.set(next);
                        },
                        EnterpriseIcon { name: "panel".to_owned() }
                    }
                }
                button {
                    class: "ui-command-trigger",
                    r#type: "button",
                    aria_keyshortcuts: "Control+K Meta+K",
                    onclick: move |_| command_open.set(true),
                    EnterpriseIcon { name: "search".to_owned() }
                    span { {shared_text(current_locale, "Search workspaces", "ค้นหาพื้นที่ทำงาน")} }
                    kbd { "⌘K" }
                }
                div { class: "ui-nav-items",
                    for group in nav_groups.iter() {
                        details { class: "ui-nav-group", open: true,
                            summary { class: "ui-nav-group-heading",
                                span { class: "ui-nav-group-title", "{group.label}" }
                                span { class: "ui-nav-group-count", "{group.items.len()}" }
                            }
                            div { class: "ui-nav-group-items",
                            for item in group.items.iter() {
                                {
                                    let path_for_click = item.path.clone();
                                    rsx! {
                                        a {
                                            class: if is_active(&active_path, &item.path) { "ui-nav-item active" } else { "ui-nav-item" },
                                            href: "{item.path}",
                                            onclick: move |event| {
                                                event.prevent_default();
                                                on_navigate.call(path_for_click.clone());
                                                mobile_nav_open.set(false);
                                            },
                                            aria_current: if is_active(&active_path, &item.path) { "page" } else { "false" },
                                            span { class: "ui-nav-icon", aria_hidden: "true",
                                                EnterpriseIcon { name: nav_icon_name(&item.path).to_owned() }
                                            }
                                            span { class: "ui-nav-label", "{item.label}" }
                                        }
                                    }
                                }
                            }
                            }
                        }
                    }
                }
                div { class: "ui-nav-footer",
                    LocaleToggle {}
                    span { class: "ui-locale-name ui-nav-label", "{current_locale.native_name()}" }
                    button {
                        class: "ui-icon-button ui-theme-button",
                        r#type: "button",
                        aria_label: shared_text(current_locale, "Change color mode", "เปลี่ยนโหมดสี"),
                        onclick: move |_| theme.toggle(),
                        EnterpriseIcon { name: "theme".to_owned() }
                        span { class: "ui-nav-label", {theme_mode_label(current_locale, theme.current())} }
                    }
                }
            }
            main { class: "ui-main",
                header { class: "ui-topbar",
                    div { class: "ui-topbar-leading",
                        button {
                            class: "ui-icon-button ui-mobile-menu",
                            r#type: "button",
                            aria_label: shared_text(current_locale, "Open navigation", "เปิดเมนูนำทาง"),
                            aria_expanded: mobile_nav_open.read().to_string(),
                            onclick: move |_| mobile_nav_open.set(true),
                            EnterpriseIcon { name: "menu".to_owned() }
                        }
                        nav { class: "ui-breadcrumbs", aria_label: shared_text(current_locale, "Breadcrumb", "เส้นทางนำทาง"),
                            span { "{active_context.0}" }
                            EnterpriseIcon { name: "chevron".to_owned() }
                            strong { "{active_context.1}" }
                        }
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
                        button {
                            class: "ui-profile-button",
                            r#type: "button",
                            aria_label: shared_text(current_locale, "Account and session", "บัญชีและเซสชัน"),
                            onclick: move |_| on_account_session.call(()),
                            span { class: "ui-profile-avatar", aria_hidden: "true", "{profile_avatar}" }
                            span { class: "ui-profile-copy",
                                strong { "{profile_principal}" }
                                small { "{profile_role}" }
                            }
                        }
                    }
                }
                div { class: "ui-content",
                    if let Some(metadata) = session_metadata {
                        SessionBanners { metadata }
                    }
                    if let Some(banner) = context_banner {
                        ContextBanner { kind: banner }
                    }
                    {children}
                }
            }
            Dialog {
                open: *command_open.read(),
                title: shared_text(current_locale, "Find a workspace", "ค้นหาพื้นที่ทำงาน").to_owned(),
                on_close: move |_| command_open.set(false),
                div { class: "ui-command-palette",
                    div { class: "ui-command-input",
                        EnterpriseIcon { name: "search".to_owned() }
                        label { class: "sr-only", r#for: "erpx-command-search", {shared_text(current_locale, "Search authorized routes", "ค้นหาเส้นทางที่ได้รับอนุญาต")} }
                        input {
                            id: "erpx-command-search",
                            r#type: "search",
                            value: "{command_query}",
                            placeholder: shared_text(current_locale, "Search authorized workspaces and settings", "ค้นหาพื้นที่ทำงานและการตั้งค่าที่ได้รับอนุญาต"),
                            autofocus: true,
                            oninput: move |event| command_query.set(event.value()),
                        }
                    }
                    div { class: "ui-command-results", role: "listbox",
                        if command_items.is_empty() {
                            div { class: "ui-command-empty", {shared_text(current_locale, "No authorized route matches this search.", "ไม่มีเส้นทางที่ได้รับอนุญาตตรงกับการค้นหานี้")} }
                        }
                        for item in command_items {
                            {
                                let path = item.path.clone();
                                rsx! {
                                    button {
                                        class: "ui-command-result",
                                        r#type: "button",
                                        role: "option",
                                        onclick: move |_| {
                                            on_navigate.call(path.clone());
                                            command_open.set(false);
                                            command_query.set(String::new());
                                        },
                                        EnterpriseIcon { name: nav_icon_name(&item.path).to_owned() }
                                        span { strong { "{item.label}" } small { "{item.path}" } }
                                        kbd { "↵" }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "ui-safe-note", {shared_text(current_locale, "Search is limited to server-authorized navigation. It does not index customer business records.", "การค้นหาจำกัดเฉพาะเมนูนำทางที่เซิร์ฟเวอร์อนุญาต และไม่จัดทำดัชนีระเบียนธุรกิจของลูกค้า")} }
                }
            }
        }
    }
}

fn nav_icon_name(path: &str) -> &'static str {
    if path.contains("customers") {
        "customers"
    } else if path.contains("director") {
        "directory"
    } else if path.contains("support") {
        "support"
    } else if path.contains("billing") || path.contains("finance") {
        "billing"
    } else if path.contains("settings") {
        "settings"
    } else if path.contains("audit") || path.contains("role-review") {
        "shield"
    } else if path.contains("backup") {
        "database"
    } else if path.contains("projection") {
        "activity"
    } else if path.contains("tenant") {
        "building"
    } else {
        "module"
    }
}

/// Fixed inline-SVG icon registry shared across every target. Icon names are
/// presentation metadata and never interpreted as markup.
#[component]
pub fn EnterpriseIcon(name: String) -> Element {
    let path = match name.as_str() {
        "search" => "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14Zm5 12 4 4",
        "customers" => {
            "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm13 10v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"
        }
        "directory" => "M3 5h6l2 2h10v12H3V5Zm4 7h10M7 16h7",
        "support" => {
            "M4 15v-3a8 8 0 0 1 16 0v3M4 15h3v5H5a1 1 0 0 1-1-1v-4Zm16 0h-3v5h2a1 1 0 0 0 1-1v-4Z"
        }
        "billing" => "M12 2v20M17 6H9.5a3.5 3.5 0 0 0 0 7H14a3.5 3.5 0 0 1 0 7H6",
        "settings" => {
            "M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Zm7.4-3.5a7 7 0 0 0-.1-1l2-1.6-2-3.4-2.5 1a8 8 0 0 0-1.7-1L14.7 3h-4L10 6a8 8 0 0 0-1.8 1L5.8 6 3.8 9.4 6 11a7 7 0 0 0 0 2l-2.2 1.6 2 3.4 2.4-1a8 8 0 0 0 1.8 1l.7 3h4l.6-3a8 8 0 0 0 1.8-1l2.4 1 2-3.4-2.1-1.6a7 7 0 0 0 0-1Z"
        }
        "shield" => "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Zm-3-10 2 2 4-4",
        "database" => {
            "M4 6c0 2 3.6 4 8 4s8-2 8-4-3.6-4-8-4-8 2-8 4Zm0 0v6c0 2 3.6 4 8 4s8-2 8-4V6M4 12v6c0 2 3.6 4 8 4s8-2 8-4v-6"
        }
        "activity" => "M3 12h4l2-7 4 14 2-7h6",
        "building" => "M4 22V3h12v19M16 9h4v13M8 7h4M8 11h4M8 15h4M8 19h4",
        "menu" => "M4 6h16M4 12h16M4 18h16",
        "panel" => "M4 4h16v16H4V4Zm6 0v16",
        "theme" => "M12 3a9 9 0 1 0 9 9 7 7 0 0 1-9-9Z",
        "chevron" => "m9 18 6-6-6-6",
        _ => "M4 4h7v7H4V4Zm9 0h7v7h-7V4ZM4 13h7v7H4v-7Zm9 0h7v7h-7v-7Z",
    };
    rsx! {
        svg {
            class: "ui-icon",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.8",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "{path}" }
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
    let locale = current_locale();
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
                        message: match locale {
                            Locale::En => format!("Representing {} · {}", impersonation.represented_label, impersonation.reason),
                            Locale::Th => format!("กำลังเป็นตัวแทน {} · {}", impersonation.represented_label, impersonation.reason),
                        },
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
    let locale = current_locale();
    let (class, label, message) = match kind {
        BannerKind::ReadOnly { message } => (
            "readonly",
            shared_text(locale, "READ-ONLY", "อ่านอย่างเดียว"),
            message,
        ),
        BannerKind::Support {
            message,
            expires_at,
        } => (
            "support",
            shared_text(locale, "SUPPORT ACCESS", "สิทธิ์เข้าถึงเพื่อสนับสนุน"),
            match locale {
                Locale::En => format!("{message} · Expires {expires_at}"),
                Locale::Th => format!("{message} · หมดอายุ {expires_at}"),
            },
        ),
        BannerKind::Impersonation { message } => (
            "impersonation",
            shared_text(locale, "IMPERSONATION", "การสวมสิทธิ์"),
            message,
        ),
        BannerKind::Stale { message } => (
            "stale",
            shared_text(locale, "STALE READ MODEL", "แบบจำลองการอ่านล้าสมัย"),
            message,
        ),
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
    let target = current.toggled();
    let accessible_label = match target {
        Locale::En => shared_text(current, "Switch to English", "เปลี่ยนเป็นภาษาอังกฤษ"),
        Locale::Th => shared_text(current, "Switch to Thai", "เปลี่ยนเป็นภาษาไทย"),
    };
    rsx! {
        button {
            class: "ui-icon-button ui-locale-toggle",
            r#type: "button",
            aria_label: accessible_label,
            title: accessible_label,
            onclick: move |_| locale.set(target),
            "{target.native_name()}"
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

fn page_family(
    family: &'static str,
    title: String,
    description: String,
    actions: Option<Element>,
    children: Element,
) -> Element {
    let title = safe_display_text(&title);
    let description = safe_display_text(&description);
    rsx! {
        section { class: "ui-page-family ui-page-family-{family}", "data-page-family": family,
            header { class: "ui-page-family-header",
                div {
                    h3 { "{title}" }
                    p { "{description}" }
                }
                if let Some(actions) = actions {
                    div { class: "ui-page-family-actions", {actions} }
                }
            }
            div { class: "ui-page-family-body", {children} }
        }
    }
}

/// KPI, exception, activity, readiness, and next-action dashboard surface.
#[component]
pub fn DashboardLayout(
    title: String,
    description: String,
    actions: Option<Element>,
    children: Element,
) -> Element {
    page_family("dashboard", title, description, actions, children)
}

/// Searchable, filterable, pageable collection and list/detail surface.
#[component]
pub fn CollectionLayout(
    title: String,
    description: String,
    actions: Option<Element>,
    children: Element,
) -> Element {
    page_family("collection", title, description, actions, children)
}

/// Summary, status, actions, tabs, timeline, and audit record surface.
#[component]
pub fn RecordLayout(
    title: String,
    description: String,
    actions: Option<Element>,
    children: Element,
) -> Element {
    page_family("record", title, description, actions, children)
}

/// Guided validation, review, confirmation, receipt, and rollback workflow.
#[component]
pub fn WorkflowLayout(
    title: String,
    description: String,
    actions: Option<Element>,
    children: Element,
) -> Element {
    page_family("workflow", title, description, actions, children)
}

/// Categorized inherited settings with validation and dirty-state protection.
#[component]
pub fn SettingsLayout(
    title: String,
    description: String,
    actions: Option<Element>,
    children: Element,
) -> Element {
    page_family("settings", title, description, actions, children)
}

/// Desired/observed health, drift, incident evidence, and high-risk operations.
#[component]
pub fn OperationsLayout(
    title: String,
    description: String,
    actions: Option<Element>,
    children: Element,
) -> Element {
    page_family("operations", title, description, actions, children)
}

/// Validated editor payload. It contains only typed design tokens and bounded
/// display fields; credentials and executable styling never enter this model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporateIdentityEditorSubmission {
    pub display_name: String,
    pub short_name: String,
    pub tokens: BrandTokenSet,
    pub locked_categories: BTreeSet<BrandCategory>,
    pub reason: String,
    pub mfa_code: String,
    pub rollback_source_version: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorporateIdentityEditorActionKind {
    SaveDraft,
    Publish,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporateIdentityEditorAction {
    pub kind: CorporateIdentityEditorActionKind,
    pub submission: CorporateIdentityEditorSubmission,
}

fn update_preview_color(target: &mut HexColor, candidate: &str) -> bool {
    match HexColor::new(candidate) {
        Ok(value) => {
            *target = value;
            true
        }
        Err(_) => false,
    }
}

/// Governed three-region corporate-identity editor shared by platform,
/// holding, and business-unit settings. The application remains responsible
/// for server authorization and mutations.
#[component]
pub fn CorporateIdentityEditor(
    scope_label: String,
    profile: ResolvedCorporateIdentity,
    parent_locked_categories: Vec<BrandCategory>,
    editable: bool,
    on_action: Option<EventHandler<CorporateIdentityEditorAction>>,
) -> Element {
    let locale = current_locale();
    let mut display_name = use_signal(|| profile.display_name.clone());
    let mut short_name = use_signal(|| profile.short_name.clone());
    let mut light_primary = use_signal(|| profile.tokens.light.primary.as_str().to_owned());
    let mut light_accent = use_signal(|| profile.tokens.light.accent.as_str().to_owned());
    let mut dark_primary = use_signal(|| profile.tokens.dark.primary.as_str().to_owned());
    let mut dark_accent = use_signal(|| profile.tokens.dark.accent.as_str().to_owned());
    let mut font_pack = use_signal(|| profile.tokens.font_pack);
    let mut density = use_signal(|| profile.tokens.density);
    let mut radius = use_signal(|| profile.tokens.radius);
    let mut elevation = use_signal(|| profile.tokens.elevation);
    let mut navigation = use_signal(|| profile.tokens.navigation);
    let mut motion = use_signal(|| profile.tokens.motion);
    let mut locked_categories = use_signal(|| profile.locked_categories.clone());
    let mut preview_theme = use_signal(|| "light".to_owned());
    let mut preview_locale = use_signal(|| "English".to_owned());
    let mut preview_device = use_signal(|| "desktop".to_owned());
    let mut reason = use_signal(String::new);
    let mut mfa_code = use_signal(String::new);
    let mut rollback_source_version = use_signal(String::new);

    let parent_locked = parent_locked_categories
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut preview = profile.clone();
    preview.display_name = safe_display_text(&display_name.read()).to_string();
    preview.short_name = safe_display_text(&short_name.read()).to_string();
    preview.tokens.font_pack = *font_pack.read();
    preview.tokens.density = *density.read();
    preview.tokens.radius = *radius.read();
    preview.tokens.elevation = *elevation.read();
    preview.tokens.navigation = *navigation.read();
    preview.tokens.motion = *motion.read();
    let colors_valid =
        update_preview_color(&mut preview.tokens.light.primary, &light_primary.read())
            && update_preview_color(&mut preview.tokens.light.accent, &light_accent.read())
            && update_preview_color(&mut preview.tokens.dark.primary, &dark_primary.read())
            && update_preview_color(&mut preview.tokens.dark.accent, &dark_accent.read());
    let token_validation = colors_valid.then(|| preview.tokens.validate()).transpose();
    let publication_error = match token_validation {
        Ok(Some(())) => None,
        Err(error) => Some(format!(
            "{}: {error:?}",
            shared_text(locale, "Publication blocked", "ไม่สามารถเผยแพร่ได้")
        )),
        Ok(None) => Some(
            shared_text(
                locale,
                "Publication blocked: colors must use #RRGGBB.",
                "ไม่สามารถเผยแพร่ได้: สีต้องอยู่ในรูปแบบ #RRGGBB",
            )
            .to_owned(),
        ),
    };
    let preview_style = corporate_identity_style(&preview);
    let preview_class = format!(
        "ui-brand-preview {} {}",
        preview_theme.read().as_str(),
        preview_device.read().as_str()
    );
    let scope_label = safe_display_text(&scope_label);
    let version = profile.published_version;
    let inherited_count = profile.inherited_categories.len();

    let submission = CorporateIdentityEditorSubmission {
        display_name: preview.display_name.clone(),
        short_name: preview.short_name.clone(),
        tokens: preview.tokens.clone(),
        locked_categories: locked_categories.read().clone(),
        reason: safe_display_text(&reason.read()).to_string(),
        mfa_code: mfa_code.read().trim().to_owned(),
        rollback_source_version: rollback_source_version
            .read()
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|version| *version > 0),
    };
    let publish_disabled = !editable
        || publication_error.is_some()
        || submission.reason.is_empty()
        || submission.mfa_code.is_empty();
    let save_disabled = !editable || publication_error.is_some() || submission.reason.is_empty();
    let rollback_disabled = !editable
        || submission.reason.is_empty()
        || submission.mfa_code.is_empty()
        || submission.rollback_source_version.is_none();

    rsx! {
        SettingsLayout {
            title: shared_text(locale, "Corporate identity", "อัตลักษณ์องค์กร").to_owned(),
            description: match locale {
                Locale::En => format!("{scope_label} · published v{version} · {inherited_count} inherited categories"),
                Locale::Th => format!("{scope_label} · เผยแพร่ v{version} · หมวดหมู่ที่สืบทอด {inherited_count} รายการ"),
            },
            actions: None,
            div { class: "ui-brand-editor",
                aside { class: "ui-brand-controls", aria_label: shared_text(locale, "Corporate identity tokens", "โทเค็นอัตลักษณ์องค์กร"),
                    section { class: "ui-brand-control-section",
                        div { class: "ui-brand-section-heading", h4 { {shared_text(locale, "Identity", "อัตลักษณ์")} } span { {shared_text(locale, "Typed display fields", "ฟิลด์แสดงผลที่กำหนดชนิด")} } }
                        label { {shared_text(locale, "Display name", "ชื่อที่แสดง")} input { value: "{display_name}", maxlength: "96", disabled: !editable, oninput: move |event| display_name.set(event.value()) } }
                        label { {shared_text(locale, "Short name", "ชื่อย่อ")} input { value: "{short_name}", maxlength: "16", disabled: !editable, oninput: move |event| short_name.set(event.value()) } }
                    }
                    section { class: "ui-brand-control-section",
                        div { class: "ui-brand-section-heading", h4 { {shared_text(locale, "Color", "สี")} } span { {shared_text(locale, "Light and dark required", "ต้องกำหนดทั้งโหมดสว่างและมืด")} } }
                        div { class: "ui-brand-color-grid",
                            label { {shared_text(locale, "Light primary", "สีหลักโหมดสว่าง")} input { r#type: "color", value: "{light_primary}", disabled: !editable, oninput: move |event| light_primary.set(event.value()) } }
                            label { {shared_text(locale, "Light accent", "สีเน้นโหมดสว่าง")} input { r#type: "color", value: "{light_accent}", disabled: !editable, oninput: move |event| light_accent.set(event.value()) } }
                            label { {shared_text(locale, "Dark primary", "สีหลักโหมดมืด")} input { r#type: "color", value: "{dark_primary}", disabled: !editable, oninput: move |event| dark_primary.set(event.value()) } }
                            label { {shared_text(locale, "Dark accent", "สีเน้นโหมดมืด")} input { r#type: "color", value: "{dark_accent}", disabled: !editable, oninput: move |event| dark_accent.set(event.value()) } }
                        }
                    }
                    section { class: "ui-brand-control-section",
                        div { class: "ui-brand-section-heading", h4 { {shared_text(locale, "Presentation", "การนำเสนอ")} } span { {shared_text(locale, "Curated values only", "ใช้เฉพาะค่าที่กำหนดไว้")} } }
                        label { {shared_text(locale, "Font pack", "ชุดแบบอักษร")} select { value: font_pack.read().as_str(), disabled: !editable, onchange: move |event| if let Ok(value) = BrandFontPack::parse(&event.value()) { font_pack.set(value); },
                            option { value: "erpx_sans", "ERPX Sans" }
                            option { value: "corporate_sans", "Corporate Sans" }
                            option { value: "thai_formal", "Thai Formal" }
                            option { value: "system_ui", "System UI" }
                        } }
                        label { {shared_text(locale, "Density", "ความหนาแน่น")} select { value: density.read().as_str(), disabled: !editable, onchange: move |event| density.set(match event.value().as_str() { "compact" => BrandDensity::Compact, "spacious" => BrandDensity::Spacious, _ => BrandDensity::Comfortable }),
                            option { value: "compact", {shared_text(locale, "Compact", "กระชับ")} } option { value: "comfortable", {shared_text(locale, "Comfortable", "สบายตา")} } option { value: "spacious", {shared_text(locale, "Spacious", "โปร่ง")} }
                        } }
                        label { {shared_text(locale, "Shape", "รูปทรง")} select { value: radius.read().as_str(), disabled: !editable, onchange: move |event| radius.set(match event.value().as_str() { "square" => BrandRadius::Square, "rounded" => BrandRadius::Rounded, _ => BrandRadius::Soft }),
                            option { value: "square", {shared_text(locale, "Square", "เหลี่ยม")} } option { value: "soft", {shared_text(locale, "Soft", "นุ่มนวล")} } option { value: "rounded", {shared_text(locale, "Rounded", "โค้งมน")} }
                        } }
                        label { {shared_text(locale, "Elevation", "ระดับเงา")} select { value: elevation.read().as_str(), disabled: !editable, onchange: move |event| elevation.set(match event.value().as_str() { "flat" => BrandElevation::Flat, "elevated" => BrandElevation::Elevated, _ => BrandElevation::Subtle }),
                            option { value: "flat", {shared_text(locale, "Flat", "เรียบ")} } option { value: "subtle", {shared_text(locale, "Subtle", "บางเบา")} } option { value: "elevated", {shared_text(locale, "Elevated", "ยกขึ้น")} }
                        } }
                        label { {shared_text(locale, "Navigation", "การนำทาง")} select { value: navigation.read().as_str(), disabled: !editable, onchange: move |event| navigation.set(if event.value() == "solid" { BrandNavigation::Solid } else { BrandNavigation::Tinted }),
                            option { value: "solid", {shared_text(locale, "Solid rail", "แถบสีทึบ")} } option { value: "tinted", {shared_text(locale, "Tinted rail", "แถบสีอ่อน")} }
                        } }
                        label { {shared_text(locale, "Motion", "การเคลื่อนไหว")} select { value: motion.read().as_str(), disabled: !editable, onchange: move |event| motion.set(match event.value().as_str() { "none" => BrandMotion::None, "standard" => BrandMotion::Standard, _ => BrandMotion::Subtle }),
                            option { value: "none", {shared_text(locale, "None", "ไม่มี")} } option { value: "subtle", {shared_text(locale, "Subtle", "นุ่มนวล")} } option { value: "standard", {shared_text(locale, "Standard", "มาตรฐาน")} }
                        } }
                    }
                }
                section { class: "ui-brand-preview-region", aria_label: shared_text(locale, "Live corporate identity preview", "ตัวอย่างอัตลักษณ์องค์กรแบบสด"),
                    div { class: "ui-brand-preview-toolbar",
                        div { role: "group", aria_label: shared_text(locale, "Preview device", "อุปกรณ์ตัวอย่าง"),
                            for (device, label) in [("desktop", shared_text(locale, "Desktop", "เดสก์ท็อป")), ("tablet", shared_text(locale, "Tablet", "แท็บเล็ต")), ("mobile", shared_text(locale, "Mobile", "มือถือ"))] { button { class: if preview_device.read().as_str() == device { "active" } else { "" }, r#type: "button", onclick: move |_| preview_device.set(device.to_owned()), "{label}" } }
                        }
                        div { role: "group", aria_label: shared_text(locale, "Preview language and mode", "ภาษาและโหมดตัวอย่าง"),
                            button { r#type: "button", onclick: move |_| preview_locale.set(if preview_locale.read().as_str() == "English" { "ไทย".to_owned() } else { "English".to_owned() }), "{preview_locale}" }
                            button { r#type: "button", onclick: move |_| preview_theme.set(if preview_theme.read().as_str() == "light" { "dark".to_owned() } else { "light".to_owned() }), "{preview_theme}" }
                        }
                    }
                    div { class: "{preview_class}", style: "{preview_style}",
                        div { class: "ui-brand-preview-shell",
                            aside { span { class: "ui-brand-preview-mark", "{preview.short_name}" } strong { "{preview.display_name}" }
                                nav { span { class: "active", {if preview_locale.read().as_str() == "ไทย" { "แดชบอร์ด" } else { "Dashboard" }} } span { {if preview_locale.read().as_str() == "ไทย" { "การเงิน" } else { "Finance" }} } span { {if preview_locale.read().as_str() == "ไทย" { "บุคลากรและบทบาท" } else { "People & roles" }} } span { {if preview_locale.read().as_str() == "ไทย" { "การตั้งค่า" } else { "Settings" }} } }
                            }
                            main {
                                header { small { "{preview_locale} · {scope_label}" } button { {if preview_locale.read().as_str() == "ไทย" { "การดำเนินการหลัก" } else { "Primary action" }} } }
                                div { class: "ui-brand-preview-content",
                                    div { class: "ui-brand-preview-kpis", for (label, value) in if preview_locale.read().as_str() == "ไทย" { [("รายการอนุมัติที่เปิดอยู่", "12"), ("บริการที่ปกติ", "9/10"), ("ความใหม่ของไดเรกทอรี", "30 วินาที")] } else { [("Open approvals", "12"), ("Healthy services", "9/10"), ("Directory freshness", "30s")] } { article { small { "{label}" } strong { "{value}" } } } }
                                    article { class: "ui-brand-preview-table", h5 { {if preview_locale.read().as_str() == "ไทย" { "งานล่าสุด" } else { "Recent work" }} }
                                        div { strong { {if preview_locale.read().as_str() == "ไทย" { "ตรวจสอบการปิดไตรมาส" } else { "Quarter close review" }} } span { {if preview_locale.read().as_str() == "ไทย" { "พร้อมอนุมัติ" } else { "Ready for approval" }} } }
                                        div { strong { {if preview_locale.read().as_str() == "ไทย" { "ซิงโครไนซ์ไดเรกทอรี" } else { "Directory synchronization" }} } span { {if preview_locale.read().as_str() == "ไทย" { "ปกติ" } else { "Healthy" }} } }
                                    }
                                    div { class: "ui-brand-preview-safety", strong { {if preview_locale.read().as_str() == "ไทย" { "สถานะความปลอดภัย" } else { "Security status" }} } {if preview_locale.read().as_str() == "ไทย" { " · สีความปลอดภัยที่ ERPX กำหนดไม่สามารถปรับตามแบรนด์ได้" } else { " · Mandatory ERPX safety colors are not brand configurable." }} }
                                }
                            }
                        }
                    }
                }
                aside { class: "ui-brand-governance", aria_label: shared_text(locale, "Identity inheritance and publication", "การสืบทอดและเผยแพร่อัตลักษณ์"),
                    section { h4 { {shared_text(locale, "Inheritance & locks", "การสืบทอดและการล็อก")} }
                        p { {shared_text(locale, "A child can specialize only categories left unlocked by its parent.", "ระดับลูกปรับเฉพาะหมวดหมู่ที่ระดับแม่ไม่ได้ล็อกไว้เท่านั้น")} }
                        div { class: "ui-brand-category-list",
                            for category in BrandCategory::ALL {
                                {
                                    let is_parent_locked = parent_locked.contains(&category);
                                    let is_locked = locked_categories.read().contains(&category);
                                    rsx! { label { class: if is_parent_locked { "locked-by-parent" } else { "" },
                                        input { r#type: "checkbox", checked: is_locked || is_parent_locked, disabled: !editable || is_parent_locked,
                                            onchange: move |event| { let mut next = locked_categories.read().clone(); if event.checked() { next.insert(category); } else { next.remove(&category); } locked_categories.set(next); }
                                        }
                                        span { "{category.as_str()}" }
                                        if is_parent_locked { small { {shared_text(locale, "Inherited · locked by parent", "สืบทอด · ล็อกโดยระดับแม่")} } }
                                    } }
                                }
                            }
                        }
                    }
                    section { h4 { {shared_text(locale, "Validation", "การตรวจสอบ")} }
                        if let Some(error) = publication_error.as_ref() { div { class: "ui-brand-validation bad", role: "alert", "{error}" } }
                        else { div { class: "ui-brand-validation good", role: "status", {shared_text(locale, "Both palettes pass typed token and WCAG publication checks.", "ชุดสีทั้งสองผ่านการตรวจสอบโทเค็นและ WCAG สำหรับการเผยแพร่")} } }
                        p { {shared_text(locale, "Safety banners, focus visibility, destructive actions, and Root/support identity always remain ERPX controlled.", "แบนเนอร์ความปลอดภัย การมองเห็นโฟกัส การดำเนินการที่มีผลกระทบ และอัตลักษณ์ Root/ฝ่ายสนับสนุนยังอยู่ภายใต้การควบคุมของ ERPX เสมอ")} }
                    }
                    if editable {
                        section { h4 { {shared_text(locale, "Publish", "เผยแพร่")} }
                            label { {shared_text(locale, "Reason", "เหตุผล")} textarea { value: "{reason}", maxlength: "240", oninput: move |event| reason.set(event.value()), placeholder: shared_text(locale, "Required publication reason", "ระบุเหตุผลที่ต้องเผยแพร่") } }
                            label { {shared_text(locale, "Fresh MFA", "MFA ล่าสุด")} input { r#type: "password", value: "{mfa_code}", autocomplete: "one-time-code", oninput: move |event| mfa_code.set(event.value()) } }
                            label { {shared_text(locale, "Immutable rollback source version", "เวอร์ชันต้นทางแบบแก้ไขไม่ได้สำหรับ rollback")} input { r#type: "number", min: "1", value: "{rollback_source_version}", oninput: move |event| rollback_source_version.set(event.value()) } }
                            div { class: "ui-brand-action-row",
                                button { class: "ui-button", r#type: "button", disabled: save_disabled, onclick: { let submission = submission.clone(); move |_| if let Some(handler) = on_action { handler.call(CorporateIdentityEditorAction { kind: CorporateIdentityEditorActionKind::SaveDraft, submission: submission.clone() }); } }, {shared_text(locale, "Save draft", "บันทึกฉบับร่าง")} }
                                button { class: "ui-button primary", r#type: "button", disabled: publish_disabled, onclick: { let submission = submission.clone(); move |_| if let Some(handler) = on_action { handler.call(CorporateIdentityEditorAction { kind: CorporateIdentityEditorActionKind::Publish, submission: submission.clone() }); } }, {shared_text(locale, "Publish with MFA", "เผยแพร่ด้วย MFA")} }
                                button { class: "ui-button danger", r#type: "button", disabled: rollback_disabled, onclick: { let submission = submission.clone(); move |_| if let Some(handler) = on_action { handler.call(CorporateIdentityEditorAction { kind: CorporateIdentityEditorActionKind::Rollback, submission: submission.clone() }); } }, {shared_text(locale, "Rollback with MFA", "Rollback ด้วย MFA")} }
                            }
                        }
                    } else {
                        div { class: "ui-brand-validation neutral", {shared_text(locale, "Read-only effective identity. Platform Root changes to a customer profile must use an audited proposal activated by a holding-authorized publisher.", "อัตลักษณ์ที่มีผลเป็นแบบอ่านอย่างเดียว การเปลี่ยนโปรไฟล์ลูกค้าโดย Platform Root ต้องใช้ข้อเสนอที่ตรวจสอบได้และเปิดใช้โดยผู้เผยแพร่ที่โฮลดิ้งอนุญาต")} }
                    }
                }
            }
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
    let locale = current_locale();
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
                if required { span { class: "ui-required", aria_label: shared_text(locale, "required", "จำเป็น"), " *" } }
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
    let locale = current_locale();
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
                if required { span { class: "ui-required", aria_label: shared_text(locale, "required", "จำเป็น"), " *" } }
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
    let locale = current_locale();
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
                        tr { td { colspan: "{column_count}", class: "ui-table-empty", {shared_text(locale, "No records", "ไม่มีระเบียน")} } }
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
    let locale = current_locale();
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
    let focus_guard_label = shared_text(locale, "Dialog focus boundary", "ขอบเขตโฟกัสของกล่องโต้ตอบ");
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
        UiStateKind::Submitting => "submitting",
        UiStateKind::Empty => "empty",
        UiStateKind::Success => "success",
        UiStateKind::Denied => "denied",
        UiStateKind::Stale => "stale",
        UiStateKind::ReadOnly => "readonly",
        UiStateKind::Conflict => "conflict",
        UiStateKind::Offline => "offline",
        UiStateKind::RetryableError => "retryable-error",
        UiStateKind::Unavailable => "unavailable",
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
                    UiStateKind::Submitting => "◌",
                    UiStateKind::Empty => "○",
                    UiStateKind::Success => "✓",
                    UiStateKind::Denied => "⊘",
                    UiStateKind::Stale => "◷",
                    UiStateKind::ReadOnly => "▣",
                    UiStateKind::Conflict => "↯",
                    UiStateKind::Offline => "⌁",
                    UiStateKind::RetryableError => "↻",
                    UiStateKind::Unavailable => "—",
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
    let locale = current_locale();
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
            shared_text(locale, "Sign-in required", "ต้องเข้าสู่ระบบ"),
            match locale {
                Locale::En => format!("Sign in to continue to {}.", redact_deep_link(&return_to)),
                Locale::Th => format!("เข้าสู่ระบบเพื่อไปยัง {} ต่อ", redact_deep_link(&return_to)),
            },
        ),
        RouteState::SessionExpired { return_to } => route_state_panel(
            UiStateKind::Denied,
            shared_text(locale, "Session expired", "เซสชันหมดอายุ"),
            match locale {
                Locale::En => format!(
                    "Sign in again to return to {}.",
                    redact_deep_link(&return_to)
                ),
                Locale::Th => format!("เข้าสู่ระบบอีกครั้งเพื่อกลับไปยัง {}", redact_deep_link(&return_to)),
            },
        ),
        RouteState::SessionRevoked { return_to } => route_state_panel(
            UiStateKind::Denied,
            shared_text(locale, "Session revoked", "เซสชันถูกเพิกถอน"),
            match locale {
                Locale::En => format!(
                    "This session is no longer active. Return path: {}.",
                    redact_deep_link(&return_to)
                ),
                Locale::Th => format!(
                    "เซสชันนี้ไม่ทำงานแล้ว เส้นทางกลับ: {}",
                    redact_deep_link(&return_to)
                ),
            },
        ),
        RouteState::Offline {
            cached_view_available,
            reason,
        } => route_state_panel(
            UiStateKind::Offline,
            shared_text(locale, "Offline", "ออฟไลน์"),
            match (locale, cached_view_available) {
                (Locale::En, true) => {
                    format!("{reason} Cached content may be available; writes remain disabled.")
                }
                (Locale::En, false) => format!("{reason} No cached content is available."),
                (Locale::Th, true) => format!("{reason} อาจมีเนื้อหาที่แคชไว้ แต่ยังปิดใช้งานการเขียน"),
                (Locale::Th, false) => format!("{reason} ไม่มีเนื้อหาที่แคชไว้"),
            },
        ),
        RouteState::Denied { reason } => route_state_panel(
            UiStateKind::Denied,
            shared_text(locale, "Access denied", "ไม่มีสิทธิ์เข้าถึง"),
            reason,
        ),
        RouteState::Stale {
            age_seconds,
            reason,
        } => route_state_panel(
            UiStateKind::Stale,
            shared_text(locale, "Read model may be stale", "แบบจำลองการอ่านอาจล้าสมัย"),
            match locale {
                Locale::En => format!("{reason} Last known projection age: {age_seconds} seconds."),
                Locale::Th => format!("{reason} อายุภาพฉายล่าสุดที่ทราบ: {age_seconds} วินาที"),
            },
        ),
        RouteState::Conflict { reason } => route_state_panel(
            UiStateKind::Conflict,
            shared_text(locale, "Changes need review", "ต้องตรวจสอบการเปลี่ยนแปลง"),
            reason,
        ),
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
