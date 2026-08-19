use std::{collections::BTreeSet, rc::Rc};

use dioxus::{
    dioxus_core::{current_scope_id, spawn_forever},
    prelude::*,
};
use dioxus_icons::lucide::{
    Activity, Building2, ChartNoAxesColumn, ChevronDown, CreditCard, Database, Folder, Headphones,
    Inbox, LayoutGrid, Lock, Menu, PanelLeft, Search, Settings, Shield, SunMoon, Users, Workflow,
    X,
};
use erpx_domain::corporate_identity::{
    BrandCategory, BrandDensity, BrandElevation, BrandFontPack, BrandMotion, BrandNavigation,
    BrandRadius, BrandTokenSet, HexColor, ResolvedCorporateIdentity,
};

use crate::kit::{
    Disclosure as KitDisclosure, Link as KitLink, SelectControl as KitSelect, Table as KitTable,
    button::{Button as KitButton, ButtonSize, ButtonVariant},
    dialog::{Dialog as KitDialog, DialogTitle as KitDialogTitle},
    input::Input as KitInput,
    sidebar::{Sidebar as KitSidebar, SidebarCollapsible, SidebarProvider},
    textarea::Textarea as KitTextarea,
    tooltip::{
        Tooltip as KitTooltip, TooltipContent as KitTooltipContent,
        TooltipTrigger as KitTooltipTrigger,
    },
};
use erpx_ui_core::{
    Locale, LocaleContext, LocalizedText, NavGroup, NavItem, RouteState, SafeSessionMetadata,
    SafeUiText, ScopeOption, ThemeMode, TranslationKey, UiStateKind, corporate_identity_style,
    localized_navigation_label, redact_deep_link, use_corporate_identity, use_locale, use_theme,
};

const fn shared_text(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    LocalizedText::new(en, th).text(locale)
}

fn current_locale() -> Locale {
    try_consume_context::<LocaleContext>()
        .map(|locale| locale.current())
        .unwrap_or(Locale::En)
}

#[derive(Clone, Copy)]
struct AccountSessionFocusTarget(ReadSignal<Option<Rc<MountedData>>>);

#[derive(Clone, Copy)]
pub struct NavigationDisclosureState {
    expanded: Signal<BTreeSet<String>>,
    on_change: Callback<Vec<String>>,
}

impl NavigationDisclosureState {
    pub fn is_expanded(self, group_id: &str) -> bool {
        self.expanded.read().contains(group_id)
    }

    pub fn toggle(self, group_id: &str) {
        let mut expanded = self.expanded;
        if expanded.read().contains(group_id) {
            expanded.write().remove(group_id);
        } else {
            expanded.write().insert(group_id.to_owned());
        }
        self.on_change
            .call(expanded.read().iter().cloned().collect::<Vec<_>>());
    }
}

pub fn use_navigation_disclosure_state() -> Option<NavigationDisclosureState> {
    try_consume_context::<NavigationDisclosureState>()
}

/// A discoverable navigation destination that explains its requirements without navigating.
/// The controlled tooltip opens on hover/focus through the kit primitive and explicitly on tap.
#[component]
pub fn LockedTooltipButton(
    wrapper_class: String,
    class: String,
    accessible_label: String,
    requirement: String,
    #[props(default)] option_role: bool,
    children: Element,
) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        KitTooltip {
            class: wrapper_class,
            open: Some(*open.read()),
            on_open_change: move |next_open: bool| open.set(next_open),
            KitTooltipTrigger {
                class,
                role: if option_role { "option" } else { "button" },
                aria_disabled: "true",
                aria_label: accessible_label,
                "data-access": "locked",
                onclick: move |event: MouseEvent| {
                    event.prevent_default();
                    open.set(true);
                },
                {children}
            }
            KitTooltipContent {
                {requirement}
            }
        }
    }
}

/// Return the mounted account/session trigger owned by the nearest [`AppShell`]. Dialogs
/// rendered by the application composition layer use this target for deterministic focus
/// restoration after Escape, dismissal, or an explicit close action.
pub fn use_account_session_focus_target() -> ReadSignal<Option<Rc<MountedData>>> {
    try_consume_context::<AccountSessionFocusTarget>()
        .map(|context| context.0)
        .unwrap_or_default()
}

const fn theme_mode_label(locale: Locale, mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => shared_text(locale, "System", "ตามระบบ"),
        ThemeMode::Light => shared_text(locale, "Light", "สว่าง"),
        ThemeMode::Dark => shared_text(locale, "Dark", "มืด"),
    }
}

const SIDEBAR_GROUP_LIMIT: usize = 5;

fn is_administration_group(group_id: &str) -> bool {
    matches!(
        group_id,
        "holding_administration" | "platform_administration"
    )
}

fn authorized_group_items(group: &NavGroup) -> Vec<&NavItem> {
    group
        .items
        .iter()
        .filter(|item| item.is_authorized())
        .collect()
}

fn visible_sidebar_items<'a>(
    authorized_items: &[&'a NavItem],
    active_path: &str,
    expanded: bool,
) -> Vec<&'a NavItem> {
    if expanded || authorized_items.len() <= SIDEBAR_GROUP_LIMIT {
        return authorized_items.to_vec();
    }

    let mut visible = authorized_items
        .iter()
        .take(SIDEBAR_GROUP_LIMIT)
        .copied()
        .collect::<Vec<_>>();
    if let Some(active) = authorized_items
        .iter()
        .skip(SIDEBAR_GROUP_LIMIT)
        .find(|item| is_active(active_path, &item.path))
    {
        visible[SIDEBAR_GROUP_LIMIT - 1] = *active;
    }
    visible
}

/// Display-safe summary of the active authorization context shown in the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleContextSummary {
    pub role_label: String,
    pub scope_label: String,
    pub switchable: bool,
}

impl RoleContextSummary {
    pub fn new(
        role_label: impl Into<String>,
        scope_label: impl Into<String>,
        switchable: bool,
    ) -> Self {
        Self {
            role_label: role_label.into(),
            scope_label: scope_label.into(),
            switchable,
        }
    }
}

/// A top-level responsive shell shared by web, desktop, and mobile clients.
#[component]
pub fn AppShell(
    /// Product/holding-facing brand name.
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
    /// Display-safe active role and scope shown below the product identity.
    role_context: RoleContextSummary,
    /// Callback for opening the application-owned active-role selector.
    on_change_role: EventHandler<()>,
    /// Controlled open state for the active-role selector.
    #[props(default)]
    role_context_open: bool,
    /// Scope options supplied by the authenticated application boundary.
    scope_options: Vec<ScopeOption>,
    /// Selected non-secret scope key.
    selected_scope: String,
    /// Callback for requesting a scope change. The server remains authoritative.
    on_scope_change: EventHandler<String>,
    /// Callback for opening the application-owned account and session tools.
    on_account_session: EventHandler<()>,
    /// Controlled open state for the account/session overlay. This keeps the profile trigger's
    /// accessibility state synchronized with the application-owned dialog.
    #[props(default)]
    account_session_open: bool,
    /// Callback for client-side navigation. Keeping navigation inside the mounted application
    /// preserves the in-memory session on web targets that intentionally have no browser token
    /// storage fallback.
    on_navigate: EventHandler<String>,
    /// Disclosure state restored by the renderer-specific preference adapter.
    #[props(default)]
    expanded_navigation_groups: Vec<String>,
    /// Persists disclosure changes without coupling shared RSX to a browser or native backend.
    #[props(default)]
    on_navigation_disclosure_change: Option<EventHandler<Vec<String>>>,
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
    let mut brand_asset_failed = use_signal(|| false);
    let brand_style = corporate_identity_style(&identity);
    let subtitle = safe_display_text(&subtitle);
    let active_path = redact_deep_link(&active_path);
    let header_note = header_note.map(|note| safe_display_text(&note));
    let profile_principal = safe_display_text(&profile_principal);
    let role_context = RoleContextSummary {
        role_label: safe_display_text(&role_context.role_label),
        scope_label: safe_display_text(&role_context.scope_label),
        switchable: role_context.switchable,
    };
    let profile_avatar = profile_principal
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned());
    let nav_groups = nav_groups
        .into_iter()
        .map(|group| NavGroup {
            group_id: group.group_id,
            label: safe_display_text(&localized_navigation_label(
                &group.label_key,
                &group.label,
                current_locale,
            )),
            label_key: group.label_key,
            items: group
                .items
                .into_iter()
                .map(|item| NavItem {
                    route_id: item.route_id,
                    label: safe_display_text(&localized_navigation_label(
                        &item.label_key,
                        &item.label,
                        current_locale,
                    )),
                    label_key: item.label_key,
                    path: redact_deep_link(&item.path),
                    icon: safe_display_text(&item.icon),
                    action_capabilities: item.action_capabilities,
                    access: item.access,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let known_navigation_groups = nav_groups
        .iter()
        .map(|group| group.group_id.clone())
        .collect::<BTreeSet<_>>();
    let initial_expanded = expanded_navigation_groups
        .into_iter()
        .filter(|group_id| known_navigation_groups.contains(group_id))
        .collect::<BTreeSet<_>>();
    let expanded_navigation_groups = use_signal(|| initial_expanded);
    let disclosure_change = Callback::new(move |groups: Vec<String>| {
        if let Some(handler) = &on_navigation_disclosure_change {
            handler.call(groups);
        }
    });
    use_context_provider(|| NavigationDisclosureState {
        expanded: expanded_navigation_groups,
        on_change: disclosure_change,
    });
    let primary_items = [
        NavItem::new(
            shared_text(current_locale, "My Work", "งานของฉัน"),
            "/work",
            "inbox",
        ),
        NavItem::new(
            shared_text(current_locale, "Processes", "กระบวนการ"),
            "/processes",
            "workflow",
        ),
        NavItem::new(
            shared_text(current_locale, "Insights", "ข้อมูลเชิงลึก"),
            "/insights",
            "insights",
        ),
    ];
    let module_hub_item = NavItem::new(
        shared_text(current_locale, "All modules", "โมดูลทั้งหมด"),
        "/modules",
        "module",
    );
    let has_module_catalog = nav_groups
        .iter()
        .find(|group| group.group_id == "business_modules")
        .is_some_and(|group| !group.items.is_empty());
    let active_context = primary_items
        .iter()
        .find(|item| is_active(&active_path, &item.path))
        .map(|item| {
            (
                shared_text(current_locale, "Work OS", "พื้นที่ทำงาน").to_owned(),
                item.label.clone(),
            )
        })
        .or_else(|| {
            is_active(&active_path, &module_hub_item.path).then(|| {
                (
                    shared_text(current_locale, "Workspaces", "พื้นที่ทำงาน").to_owned(),
                    module_hub_item.label.clone(),
                )
            })
        })
        .or_else(|| {
            nav_groups.iter().find_map(|group| {
                group
                    .items
                    .iter()
                    .find(|item| is_active(&active_path, &item.path))
                    .map(|item| (group.label.clone(), item.label.clone()))
            })
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
    let mut trust_open = use_signal(|| false);
    let mut account_session_focus_target = use_signal(|| None::<Rc<MountedData>>);
    let mut role_context_focus_target = use_signal(|| None::<Rc<MountedData>>);
    let mut account_profile_focus_target = use_signal(|| None::<Rc<MountedData>>);
    use_context_provider(|| AccountSessionFocusTarget(account_session_focus_target.into()));
    let mut command_query = use_signal(String::new);
    let command_query_value = command_query.read().trim().to_ascii_lowercase();
    let mut command_items = primary_items
        .iter()
        .chain(has_module_catalog.then_some(&module_hub_item))
        .chain(nav_groups.iter().flat_map(|group| group.items.iter()))
        .filter(|item| item.is_authorized())
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
    command_items.sort_by(|left, right| left.label.cmp(&right.label));
    let has_workspace_navigation = nav_groups.iter().any(|group| {
        !is_administration_group(&group.group_id)
            && group.items.iter().any(|item| item.is_authorized())
    });
    let has_administration_navigation = nav_groups.iter().any(|group| {
        is_administration_group(&group.group_id)
            && group.items.iter().any(|item| item.is_authorized())
    });
    let role_context_accessible_label = match current_locale {
        Locale::En => format!(
            "Active work role: {}. Scope: {}{}",
            role_context.role_label,
            role_context.scope_label,
            if role_context.switchable {
                ". Change work role"
            } else {
                ""
            }
        ),
        Locale::Th => format!(
            "บทบาทการทำงานปัจจุบัน: {} ขอบเขต: {}{}",
            role_context.role_label,
            role_context.scope_label,
            if role_context.switchable {
                " เปลี่ยนบทบาทการทำงาน"
            } else {
                ""
            }
        ),
    };
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
        SidebarProvider {
            open: Some(!*nav_collapsed.read()),
            on_open_change: move |next_open: bool| nav_collapsed.set(!next_open),
            "data-erpx-shell": "true",
            div {
                class: "{shell_class}",
                style: "{brand_style}",
                "data-locale": current_locale.language_tag(),
                "data-theme": theme_token,
                "data-brand-version": identity.published_version.to_string(),
            KitButton { size: ButtonSize::Default,
                variant: ButtonVariant::Ghost,
                class: if *mobile_nav_open.read() { "ui-nav-scrim visible" } else { "ui-nav-scrim" },
                r#type: "button",
                aria_label: shared_text(current_locale, "Close navigation", "ปิดเมนูนำทาง"),
                onclick: move |_| mobile_nav_open.set(false),
            }
            KitSidebar {
                class: "ui-sidebar-boundary",
                collapsible: SidebarCollapsible::None,
                nav {
                    class: "{nav_class}",
                    aria_label: locale.text(TranslationKey::Navigation),
                    onkeydown: move |event: KeyboardEvent| {
                        if event.key() == Key::Escape {
                            mobile_nav_open.set(false);
                        }
                    },
                div { class: "ui-nav-header",
                    div { class: "ui-nav-brand",
                        span { class: "ui-brand-mark", aria_hidden: "true",
                            if *brand_asset_failed.read() {
                                "{brand_mark}"
                            } else {
                                img {
                                    src: "/assets/branding/logo-mark",
                                    alt: "",
                                    onerror: move |_| brand_asset_failed.set(true),
                                }
                            }
                        }
                        div { class: "ui-brand-copy",
                            strong { "{brand}" }
                            span { "{subtitle}" }
                        }
                    }
                    KitButton { size: ButtonSize::Icon,
                        variant: ButtonVariant::Ghost,
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
                    KitButton { size: ButtonSize::Icon,
                        variant: ButtonVariant::Ghost,
                        class: "ui-icon-button ui-mobile-nav-close",
                        r#type: "button",
                        aria_label: shared_text(current_locale, "Close navigation", "ปิดเมนูนำทาง"),
                        onclick: move |_| mobile_nav_open.set(false),
                        EnterpriseIcon { name: "close".to_owned() }
                    }
                }
                if role_context.switchable {
                    KitButton {
                        size: ButtonSize::Default,
                        variant: ButtonVariant::Ghost,
                        class: "ui-role-context",
                        r#type: "button",
                        aria_label: role_context_accessible_label.clone(),
                        aria_expanded: role_context_open.to_string(),
                        aria_controls: role_context_open.then_some("erpx-role-selection-dialog"),
                        onmounted: move |event: MountedEvent| {
                            role_context_focus_target.set(Some(event.data()));
                        },
                        onclick: move |_| {
                            if let Some(target) = role_context_focus_target.read().clone() {
                                account_session_focus_target.set(Some(target));
                            }
                            on_change_role.call(());
                        },
                        span { class: "ui-role-context-icon", aria_hidden: "true",
                            EnterpriseIcon { name: "roles".to_owned() }
                        }
                        span { class: "ui-role-context-copy ui-nav-label",
                            strong { "{role_context.scope_label}" }
                            small { "{role_context.role_label}" }
                        }
                        span { class: "ui-role-context-chevron ui-nav-label", aria_hidden: "true",
                            EnterpriseIcon { name: "chevron".to_owned() }
                        }
                    }
                } else {
                    div {
                        class: "ui-role-context ui-role-context-static",
                        role: "group",
                        aria_label: role_context_accessible_label.clone(),
                        span { class: "ui-role-context-icon", aria_hidden: "true",
                            EnterpriseIcon { name: "roles".to_owned() }
                        }
                        span { class: "ui-role-context-copy ui-nav-label",
                            strong { "{role_context.scope_label}" }
                            small { "{role_context.role_label}" }
                        }
                    }
                }
                div { class: "ui-nav-items",
                    div { class: "ui-nav-primary", aria_label: shared_text(current_locale, "Primary navigation", "เมนูหลัก"),
                        for item in primary_items.iter() {
                            {
                                let path_for_click = item.path.clone();
                                rsx! {
                                    KitLink {
                                        class: if is_active(&active_path, &item.path) { "ui-nav-item active" } else { "ui-nav-item" },
                                        href: "{item.path}",
                                        onclick: move |event: MouseEvent| {
                                            event.prevent_default();
                                            on_navigate.call(path_for_click.clone());
                                            mobile_nav_open.set(false);
                                        },
                                        aria_current: if is_active(&active_path, &item.path) { "page" } else { "false" },
                                        span { class: "ui-nav-icon", aria_hidden: "true", EnterpriseIcon { name: item.icon.clone() } }
                                        span { class: "ui-nav-label", "{item.label}" }
                                    }
                                }
                            }
                        }
                    }
                    if has_workspace_navigation || has_module_catalog {
                        div { class: "ui-nav-section-label ui-nav-label", {shared_text(current_locale, "Workspaces", "พื้นที่ทำงาน")} }
                        for group in nav_groups.iter().filter(|group| !is_administration_group(&group.group_id)) {
                            {
                                let authorized_items = authorized_group_items(group);
                                let group_id = group.group_id.clone();
                                let disclosure = NavigationDisclosureState { expanded: expanded_navigation_groups, on_change: disclosure_change };
                                let expanded = disclosure.is_expanded(&group_id);
                                let visible_items = visible_sidebar_items(
                                    &authorized_items,
                                    &active_path,
                                    expanded && group_id != "business_modules",
                                );
                                let hidden_count = authorized_items.len().saturating_sub(visible_items.len());
                                let region_id = format!("erpx-nav-authorized-{group_id}");
                                if !authorized_items.is_empty() {
                                    rsx! {
                                        KitDisclosure { class: "ui-nav-group", open: true,
                                            summary { class: "ui-nav-group-heading",
                                                span { class: "ui-nav-group-title", "{group.label}" }
                                            }
                                            div { id: region_id.clone(), class: "ui-nav-group-items",
                                                for item in visible_items {
                                                    {
                                                        let path_for_click = item.path.clone();
                                                        rsx! {
                                                            KitLink {
                                                                class: if is_active(&active_path, &item.path) { "ui-nav-item active" } else { "ui-nav-item" },
                                                                href: "{item.path}",
                                                                onclick: move |event: MouseEvent| {
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
                                                if group_id != "business_modules" && (hidden_count > 0 || expanded && authorized_items.len() > SIDEBAR_GROUP_LIMIT) {
                                                    {
                                                        let group_id_for_toggle = group_id.clone();
                                                        rsx! {
                                                            KitButton {
                                                                size: ButtonSize::Default,
                                                                variant: ButtonVariant::Ghost,
                                                                class: "ui-nav-show-all ui-nav-label",
                                                                r#type: "button",
                                                                aria_expanded: expanded.to_string(),
                                                                aria_controls: region_id.clone(),
                                                                onclick: move |_| disclosure.toggle(&group_id_for_toggle),
                                                                if expanded {
                                                                    {shared_text(current_locale, "Show less", "แสดงน้อยลง")}
                                                                } else {
                                                                    {format!("{} ({hidden_count})", shared_text(current_locale, "More", "เพิ่มเติม"))}
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            }
                        }
                        if has_module_catalog {
                            {
                                let path_for_click = module_hub_item.path.clone();
                                rsx! {
                                    KitLink {
                                        class: if is_active(&active_path, &module_hub_item.path) { "ui-nav-item ui-nav-all-modules active" } else { "ui-nav-item ui-nav-all-modules" },
                                        href: "{module_hub_item.path}",
                                        onclick: move |event: MouseEvent| {
                                            event.prevent_default();
                                            on_navigate.call(path_for_click.clone());
                                            mobile_nav_open.set(false);
                                        },
                                        aria_current: if is_active(&active_path, &module_hub_item.path) { "page" } else { "false" },
                                        span { class: "ui-nav-icon", aria_hidden: "true",
                                            EnterpriseIcon { name: "module".to_owned() }
                                        }
                                        span { class: "ui-nav-label", "{module_hub_item.label}" }
                                    }
                                }
                            }
                        }
                    }
                    if has_administration_navigation {
                        div { class: "ui-nav-administration-divider" }
                        div { class: "ui-nav-section-label ui-nav-label", {shared_text(current_locale, "Administration", "การดูแลระบบ")} }
                        for group in nav_groups.iter().filter(|group| is_administration_group(&group.group_id)) {
                            {
                                let authorized_items = authorized_group_items(group);
                                let group_id = group.group_id.clone();
                                let disclosure = NavigationDisclosureState { expanded: expanded_navigation_groups, on_change: disclosure_change };
                                let expanded = disclosure.is_expanded(&group_id);
                                let visible_items = visible_sidebar_items(&authorized_items, &active_path, expanded);
                                let hidden_count = authorized_items.len().saturating_sub(visible_items.len());
                                let region_id = format!("erpx-nav-authorized-{group_id}");
                                if !authorized_items.is_empty() {
                                    rsx! {
                                        KitDisclosure { class: "ui-nav-group ui-nav-administration-group", open: true,
                                            summary { class: "ui-nav-group-heading",
                                                span { class: "ui-nav-group-title", "{group.label}" }
                                            }
                                            div { id: region_id.clone(), class: "ui-nav-group-items",
                                                for item in visible_items {
                                                    {
                                                        let path_for_click = item.path.clone();
                                                        rsx! {
                                                            KitLink {
                                                                class: if is_active(&active_path, &item.path) { "ui-nav-item active" } else { "ui-nav-item" },
                                                                href: "{item.path}",
                                                                onclick: move |event: MouseEvent| {
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
                                                if hidden_count > 0 || expanded && authorized_items.len() > SIDEBAR_GROUP_LIMIT {
                                                    {
                                                        let group_id_for_toggle = group_id.clone();
                                                        rsx! {
                                                            KitButton {
                                                                size: ButtonSize::Default,
                                                                variant: ButtonVariant::Ghost,
                                                                class: "ui-nav-show-all ui-nav-label",
                                                                r#type: "button",
                                                                aria_expanded: expanded.to_string(),
                                                                aria_controls: region_id.clone(),
                                                                onclick: move |_| disclosure.toggle(&group_id_for_toggle),
                                                                if expanded {
                                                                    {shared_text(current_locale, "Show less", "แสดงน้อยลง")}
                                                                } else {
                                                                    {format!("{} ({hidden_count})", shared_text(current_locale, "More", "เพิ่มเติม"))}
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            }
                        }
                    }
                }
                    div { class: "ui-nav-footer",
                        LocaleToggle {}
                        span { class: "ui-locale-name ui-nav-label", "{current_locale.native_name()}" }
                        KitButton { size: ButtonSize::Default,
                            variant: ButtonVariant::Secondary,
                            class: "ui-icon-button ui-theme-button",
                            r#type: "button",
                            aria_label: shared_text(current_locale, "Change color mode", "เปลี่ยนโหมดสี"),
                            onclick: move |_| theme.toggle(),
                            EnterpriseIcon { name: "theme".to_owned() }
                            span { class: "ui-nav-label", {theme_mode_label(current_locale, theme.current())} }
                        }
                    }
                }
            }
            main { class: "ui-main",
                header { class: "ui-topbar",
                    div { class: "ui-topbar-leading",
                        KitButton { size: ButtonSize::Icon,
                            variant: ButtonVariant::Ghost,
                            class: "ui-icon-button ui-mobile-menu",
                            r#type: "button",
                            aria_label: if *mobile_nav_open.read() {
                                shared_text(current_locale, "Close navigation", "ปิดเมนูนำทาง")
                            } else {
                                shared_text(current_locale, "Open navigation", "เปิดเมนูนำทาง")
                            },
                            aria_expanded: mobile_nav_open.read().to_string(),
                            onclick: move |_| {
                                let next = !*mobile_nav_open.read();
                                mobile_nav_open.set(next);
                            },
                            EnterpriseIcon { name: "menu".to_owned() }
                        }
                        nav { class: "ui-breadcrumbs", aria_label: shared_text(current_locale, "Breadcrumb", "เส้นทางนำทาง"),
                            span { "{active_context.0}" }
                            EnterpriseIcon { name: "chevron".to_owned() }
                            strong { "{active_context.1}" }
                        }
                    }
                    div { class: "ui-topbar-actions",
                        KitButton { size: ButtonSize::Default,
                            variant: ButtonVariant::Secondary,
                            class: "ui-global-search-trigger",
                            r#type: "button",
                            aria_keyshortcuts: "Control+K Meta+K",
                            onclick: move |_| command_open.set(true),
                            EnterpriseIcon { name: "search".to_owned() }
                            span { {shared_text(current_locale, "Search or jump to…", "ค้นหาหรือไปที่…")} }
                            kbd { "⌘K" }
                        }
                        if scope_options.len() > 1 {
                            ScopeSelector {
                                label: locale.text(TranslationKey::Scope).to_string(),
                                options: scope_options,
                                selected: selected_scope,
                                on_change: on_scope_change,
                            }
                        }
                        KitButton { size: ButtonSize::Icon,
                            variant: ButtonVariant::Ghost,
                            class: "ui-icon-button ui-trust-button",
                            r#type: "button",
                            aria_label: shared_text(current_locale, "Trust and system status", "ความน่าเชื่อถือและสถานะระบบ"),
                            onclick: move |_| trust_open.set(true),
                            EnterpriseIcon { name: "shield".to_owned() }
                        }
                        KitButton { size: ButtonSize::Default,
                            variant: ButtonVariant::Ghost,
                            class: "ui-profile-button",
                            r#type: "button",
                            aria_label: shared_text(current_locale, "Account and session", "บัญชีและเซสชัน"),
                            aria_expanded: account_session_open.to_string(),
                            aria_controls: account_session_open.then_some("erpx-account-session-dialog"),
                            onmounted: move |event: MountedEvent| {
                                let target = event.data();
                                account_profile_focus_target.set(Some(target.clone()));
                                account_session_focus_target.set(Some(target));
                            },
                            onclick: move |_| {
                                if let Some(target) = account_profile_focus_target.read().clone() {
                                    account_session_focus_target.set(Some(target));
                                }
                                on_account_session.call(());
                            },
                            span { class: "ui-profile-avatar", aria_hidden: "true", "{profile_avatar}" }
                            span { class: "ui-profile-copy",
                                strong { "{profile_principal}" }
                                small { {shared_text(current_locale, "Account & session", "บัญชีและเซสชัน")} }
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
                title: shared_text(current_locale, "Search and actions", "ค้นหาและคำสั่ง").to_owned(),
                on_close: move |_| command_open.set(false),
                div { class: "ui-command-palette",
                    div { class: "ui-command-input",
                        EnterpriseIcon { name: "search".to_owned() }
                        label { class: "sr-only", r#for: "erpx-command-search", {shared_text(current_locale, "Search product routes", "ค้นหาเส้นทางในผลิตภัณฑ์")} }
                        KitInput {
                            id: "erpx-command-search",
                            r#type: "search",
                            value: "{command_query}",
                            placeholder: shared_text(current_locale, "Search My Work, processes, workspaces…", "ค้นหางานของฉัน กระบวนการ พื้นที่ทำงาน…"),
                            oninput: move |event: FormEvent| command_query.set(event.value()),
                        }
                    }
                    div { class: "ui-command-results", role: "listbox",
                        if command_items.is_empty() {
                            div { class: "ui-command-empty", {shared_text(current_locale, "No product route matches this search.", "ไม่มีเส้นทางในผลิตภัณฑ์ตรงกับการค้นหานี้")} }
                        }
                        for item in command_items {
                            {
                                let path = item.path.clone();
                                rsx! {
                                    KitButton { size: ButtonSize::Default,
                                        variant: ButtonVariant::Ghost,
                                        class: "ui-command-result",
                                        r#type: "button",
                                        role: "option",
                                        onclick: move |_| {
                                            on_navigate.call(path.clone());
                                            command_open.set(false);
                                            command_query.set(String::new());
                                        },
                                        EnterpriseIcon { name: nav_icon_name(&item.path).to_owned() }
                                        span { strong { "{item.label}" } small { {shared_text(current_locale, "Open destination", "เปิดปลายทาง")} } }
                                        kbd { "↵" }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "ui-safe-note", {shared_text(current_locale, "Search includes only destinations available to the active work role. Customer records are not indexed.", "การค้นหาแสดงเฉพาะปลายทางที่บทบาทการทำงานปัจจุบันเข้าได้ และไม่มีการจัดทำดัชนีระเบียนลูกค้า")} }
                }
            }
            Dialog {
                open: *trust_open.read(),
                title: shared_text(current_locale, "Trust & System Status", "ความน่าเชื่อถือและสถานะระบบ").to_owned(),
                on_close: move |_| trust_open.set(false),
                div { class: "ui-trust-panel",
                    div { class: "ui-trust-status",
                        span { class: "ui-trust-dot", aria_hidden: "true" }
                        div {
                            strong { {shared_text(current_locale, "Enterprise controls active", "การควบคุมระดับองค์กรทำงานอยู่")} }
                            p { {shared_text(current_locale, "Authorization and audit remain server-owned for every query and command.", "สิทธิ์และหลักฐานการตรวจสอบยังถูกควบคุมโดยเซิร์ฟเวอร์สำหรับทุกคำขอและคำสั่ง")} }
                        }
                    }
                    if let Some(note) = header_note {
                        KitDisclosure { class: "ui-trust-diagnostics",
                            summary { {shared_text(current_locale, "Technical evidence", "หลักฐานทางเทคนิค")} }
                            p { "{note}" }
                        }
                    }
                }
            }
            }
        }
    }
}

fn nav_icon_name(path: &str) -> &'static str {
    if path == "/work" {
        "inbox"
    } else if path == "/processes" {
        "workflow"
    } else if path == "/insights" {
        "insights"
    } else if path.contains("customers") {
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
    } else if path.contains("holding") {
        "building"
    } else {
        "module"
    }
}

/// Semantic ERPX icon registry backed by the pinned Lucide catalog. Icon names
/// are presentation metadata and never interpreted as markup.
#[component]
pub fn EnterpriseIcon(name: String) -> Element {
    let icon_attributes = vec![
        Attribute::new("class", "ui-icon", None, false),
        Attribute::new("aria-hidden", "true", None, false),
        Attribute::new("focusable", "false", None, false),
        Attribute::new("data-erpx-component", "icon", None, false),
    ];
    match name.as_str() {
        "search" => rsx! { Search { size: 24, attributes: icon_attributes } },
        "customers" => rsx! { Users { size: 24, attributes: icon_attributes } },
        "directory" => rsx! { Folder { size: 24, attributes: icon_attributes } },
        "support" => rsx! { Headphones { size: 24, attributes: icon_attributes } },
        "billing" => rsx! { CreditCard { size: 24, attributes: icon_attributes } },
        "settings" => rsx! { Settings { size: 24, attributes: icon_attributes } },
        "shield" => rsx! { Shield { size: 24, attributes: icon_attributes } },
        "database" => rsx! { Database { size: 24, attributes: icon_attributes } },
        "activity" => rsx! { Activity { size: 24, attributes: icon_attributes } },
        "inbox" => rsx! { Inbox { size: 24, attributes: icon_attributes } },
        "workflow" => rsx! { Workflow { size: 24, attributes: icon_attributes } },
        "insights" => rsx! { ChartNoAxesColumn { size: 24, attributes: icon_attributes } },
        "building" => rsx! { Building2 { size: 24, attributes: icon_attributes } },
        "menu" => rsx! { Menu { size: 24, attributes: icon_attributes } },
        "close" => rsx! { X { size: 24, attributes: icon_attributes } },
        "panel" => rsx! { PanelLeft { size: 24, attributes: icon_attributes } },
        "theme" => rsx! { SunMoon { size: 24, attributes: icon_attributes } },
        "lock" => rsx! { Lock { size: 24, attributes: icon_attributes } },
        "chevron" => rsx! { ChevronDown { size: 24, attributes: icon_attributes } },
        _ => rsx! { LayoutGrid { size: 24, attributes: icon_attributes } },
    }
}

/// A safe scope selector surface. Selecting a value only emits a request to the consuming app;
/// it does not grant access or place holding credentials in the client.
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
            KitSelect {
                id: "erpx-scope-selector",
                value: "{selected}",
                aria_label: "{label}",
                onchange: move |event: FormEvent| {
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
        KitButton { size: ButtonSize::Default,
            variant: ButtonVariant::Ghost,
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
                        label { {shared_text(locale, "Display name", "ชื่อที่แสดง")} KitInput { value: "{display_name}", maxlength: "96", disabled: !editable, oninput: move |event: FormEvent| display_name.set(event.value()) } }
                        label { {shared_text(locale, "Short name", "ชื่อย่อ")} KitInput { value: "{short_name}", maxlength: "16", disabled: !editable, oninput: move |event: FormEvent| short_name.set(event.value()) } }
                    }
                    section { class: "ui-brand-control-section",
                        div { class: "ui-brand-section-heading", h4 { {shared_text(locale, "Color", "สี")} } span { {shared_text(locale, "Light and dark required", "ต้องกำหนดทั้งโหมดสว่างและมืด")} } }
                        div { class: "ui-brand-color-grid",
                            label { {shared_text(locale, "Light primary", "สีหลักโหมดสว่าง")} KitInput { r#type: "color", value: "{light_primary}", disabled: !editable, oninput: move |event: FormEvent| light_primary.set(event.value()) } }
                            label { {shared_text(locale, "Light accent", "สีเน้นโหมดสว่าง")} KitInput { r#type: "color", value: "{light_accent}", disabled: !editable, oninput: move |event: FormEvent| light_accent.set(event.value()) } }
                            label { {shared_text(locale, "Dark primary", "สีหลักโหมดมืด")} KitInput { r#type: "color", value: "{dark_primary}", disabled: !editable, oninput: move |event: FormEvent| dark_primary.set(event.value()) } }
                            label { {shared_text(locale, "Dark accent", "สีเน้นโหมดมืด")} KitInput { r#type: "color", value: "{dark_accent}", disabled: !editable, oninput: move |event: FormEvent| dark_accent.set(event.value()) } }
                        }
                    }
                    section { class: "ui-brand-control-section",
                        div { class: "ui-brand-section-heading", h4 { {shared_text(locale, "Presentation", "การนำเสนอ")} } span { {shared_text(locale, "Curated values only", "ใช้เฉพาะค่าที่กำหนดไว้")} } }
                        label { {shared_text(locale, "Font pack", "ชุดแบบอักษร")} KitSelect { value: font_pack.read().as_str(), disabled: !editable, onchange: move |event: FormEvent| if let Ok(value) = BrandFontPack::parse(&event.value()) { font_pack.set(value); },
                            option { value: "erpx_sans", "ERPX Sans" }
                            option { value: "corporate_sans", "Corporate Sans" }
                            option { value: "thai_formal", "Thai Formal" }
                            option { value: "system_ui", "System UI" }
                        } }
                        label { {shared_text(locale, "Density", "ความหนาแน่น")} KitSelect { value: density.read().as_str(), disabled: !editable, onchange: move |event: FormEvent| density.set(match event.value().as_str() { "compact" => BrandDensity::Compact, "spacious" => BrandDensity::Spacious, _ => BrandDensity::Comfortable }),
                            option { value: "compact", {shared_text(locale, "Compact", "กระชับ")} } option { value: "comfortable", {shared_text(locale, "Comfortable", "สบายตา")} } option { value: "spacious", {shared_text(locale, "Spacious", "โปร่ง")} }
                        } }
                        label { {shared_text(locale, "Shape", "รูปทรง")} KitSelect { value: radius.read().as_str(), disabled: !editable, onchange: move |event: FormEvent| radius.set(match event.value().as_str() { "square" => BrandRadius::Square, "rounded" => BrandRadius::Rounded, _ => BrandRadius::Soft }),
                            option { value: "square", {shared_text(locale, "Square", "เหลี่ยม")} } option { value: "soft", {shared_text(locale, "Soft", "นุ่มนวล")} } option { value: "rounded", {shared_text(locale, "Rounded", "โค้งมน")} }
                        } }
                        label { {shared_text(locale, "Elevation", "ระดับเงา")} KitSelect { value: elevation.read().as_str(), disabled: !editable, onchange: move |event: FormEvent| elevation.set(match event.value().as_str() { "flat" => BrandElevation::Flat, "elevated" => BrandElevation::Elevated, _ => BrandElevation::Subtle }),
                            option { value: "flat", {shared_text(locale, "Flat", "เรียบ")} } option { value: "subtle", {shared_text(locale, "Subtle", "บางเบา")} } option { value: "elevated", {shared_text(locale, "Elevated", "ยกขึ้น")} }
                        } }
                        label { {shared_text(locale, "Navigation", "การนำทาง")} KitSelect { value: navigation.read().as_str(), disabled: !editable, onchange: move |event: FormEvent| navigation.set(if event.value() == "solid" { BrandNavigation::Solid } else { BrandNavigation::Tinted }),
                            option { value: "solid", {shared_text(locale, "Solid rail", "แถบสีทึบ")} } option { value: "tinted", {shared_text(locale, "Tinted rail", "แถบสีอ่อน")} }
                        } }
                        label { {shared_text(locale, "Motion", "การเคลื่อนไหว")} KitSelect { value: motion.read().as_str(), disabled: !editable, onchange: move |event: FormEvent| motion.set(match event.value().as_str() { "none" => BrandMotion::None, "standard" => BrandMotion::Standard, _ => BrandMotion::Subtle }),
                            option { value: "none", {shared_text(locale, "None", "ไม่มี")} } option { value: "subtle", {shared_text(locale, "Subtle", "นุ่มนวล")} } option { value: "standard", {shared_text(locale, "Standard", "มาตรฐาน")} }
                        } }
                    }
                }
                section { class: "ui-brand-preview-region", aria_label: shared_text(locale, "Live corporate identity preview", "ตัวอย่างอัตลักษณ์องค์กรแบบสด"),
                    div { class: "ui-brand-preview-toolbar",
                        div { role: "group", aria_label: shared_text(locale, "Preview device", "อุปกรณ์ตัวอย่าง"),
                            for (device, label) in [("desktop", shared_text(locale, "Desktop", "เดสก์ท็อป")), ("tablet", shared_text(locale, "Tablet", "แท็บเล็ต")), ("mobile", shared_text(locale, "Mobile", "มือถือ"))] { KitButton { variant: ButtonVariant::Secondary, size: ButtonSize::Default, class: if preview_device.read().as_str() == device { "active" } else { "" }, r#type: "button", onclick: move |_| preview_device.set(device.to_owned()), "{label}" } }
                        }
                        div { role: "group", aria_label: shared_text(locale, "Preview language and mode", "ภาษาและโหมดตัวอย่าง"),
                            KitButton { variant: ButtonVariant::Secondary, size: ButtonSize::Default, r#type: "button", onclick: move |_| preview_locale.set(if preview_locale.read().as_str() == "English" { "ไทย".to_owned() } else { "English".to_owned() }), "{preview_locale}" }
                            KitButton { variant: ButtonVariant::Secondary, size: ButtonSize::Default, r#type: "button", onclick: move |_| preview_theme.set(if preview_theme.read().as_str() == "light" { "dark".to_owned() } else { "light".to_owned() }), "{preview_theme}" }
                        }
                    }
                    div { class: "{preview_class}", style: "{preview_style}",
                        div { class: "ui-brand-preview-shell",
                            aside { span { class: "ui-brand-preview-mark", "{preview.short_name}" } strong { "{preview.display_name}" }
                                nav { span { class: "active", {if preview_locale.read().as_str() == "ไทย" { "แดชบอร์ด" } else { "Dashboard" }} } span { {if preview_locale.read().as_str() == "ไทย" { "การเงิน" } else { "Finance" }} } span { {if preview_locale.read().as_str() == "ไทย" { "บุคลากรและบทบาท" } else { "People & roles" }} } span { {if preview_locale.read().as_str() == "ไทย" { "การตั้งค่า" } else { "Settings" }} } }
                            }
                            main {
                                header { small { "{preview_locale} · {scope_label}" } KitButton { variant: ButtonVariant::Secondary, size: ButtonSize::Default, {if preview_locale.read().as_str() == "ไทย" { "การดำเนินการหลัก" } else { "Primary action" }} } }
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
                                        KitInput { r#type: "checkbox", checked: is_locked || is_parent_locked, disabled: !editable || is_parent_locked,
                                            onchange: move |event: FormEvent| { let mut next = locked_categories.read().clone(); if event.checked() { next.insert(category); } else { next.remove(&category); } locked_categories.set(next); }
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
                            label { {shared_text(locale, "Reason", "เหตุผล")} KitTextarea { value: "{reason}", maxlength: "240", oninput: move |event: FormEvent| reason.set(event.value()), placeholder: shared_text(locale, "Required publication reason", "ระบุเหตุผลที่ต้องเผยแพร่") } }
                            label { {shared_text(locale, "Fresh MFA", "MFA ล่าสุด")} KitInput { r#type: "password", value: "{mfa_code}", autocomplete: "one-time-code", oninput: move |event: FormEvent| mfa_code.set(event.value()) } }
                            label { {shared_text(locale, "Immutable rollback source version", "เวอร์ชันต้นทางแบบแก้ไขไม่ได้สำหรับ rollback")} KitInput { r#type: "number", min: "1", value: "{rollback_source_version}", oninput: move |event: FormEvent| rollback_source_version.set(event.value()) } }
                            div { class: "ui-brand-action-row",
                                KitButton { variant: ButtonVariant::Secondary, size: ButtonSize::Default, class: "ui-button", r#type: "button", disabled: save_disabled, onclick: { let submission = submission.clone(); move |_| if let Some(handler) = on_action { handler.call(CorporateIdentityEditorAction { kind: CorporateIdentityEditorActionKind::SaveDraft, submission: submission.clone() }); } }, {shared_text(locale, "Save draft", "บันทึกฉบับร่าง")} }
                                KitButton { variant: ButtonVariant::Primary, size: ButtonSize::Default, class: "ui-button", r#type: "button", disabled: publish_disabled, onclick: { let submission = submission.clone(); move |_| if let Some(handler) = on_action { handler.call(CorporateIdentityEditorAction { kind: CorporateIdentityEditorActionKind::Publish, submission: submission.clone() }); } }, {shared_text(locale, "Publish with MFA", "เผยแพร่ด้วย MFA")} }
                                KitButton { variant: ButtonVariant::Destructive, size: ButtonSize::Default, class: "ui-button", r#type: "button", disabled: rollback_disabled, onclick: { let submission = submission.clone(); move |_| if let Some(handler) = on_action { handler.call(CorporateIdentityEditorAction { kind: CorporateIdentityEditorActionKind::Rollback, submission: submission.clone() }); } }, {shared_text(locale, "Rollback with MFA", "Rollback ด้วย MFA")} }
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
            KitInput {
                id: "{input_id}",
                name: "{name}",
                r#type: "text",
                value: "{value}",
                placeholder: placeholder,
                required: required,
                aria_invalid: if has_error { "true" } else { "false" },
                aria_describedby: described_by,
                oninput: move |event: FormEvent| oninput.call(event),
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
            KitTable { class: "ui-table",
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

fn focus_mounted_target(target: ReadSignal<Option<Rc<MountedData>>>) {
    if let Some(target) = target.read().clone() {
        // Dialog owners may conditionally unmount immediately after `on_open_change(false)`.
        // Keep the focus task alive beyond that component scope so restoration is reliable.
        spawn_forever(async move {
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

/// Governed dialog widths. Content-specific selectors must not own modal geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogSize {
    Sm,
    #[default]
    Md,
    Lg,
    Full,
}

impl DialogSize {
    const fn css_token(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
            Self::Full => "full",
        }
    }
}

/// A modal dialog primitive. It renders only while open and labels the dialog for assistive tech.
#[component]
pub fn Dialog(
    open: bool,
    title: String,
    on_close: EventHandler<()>,
    #[props(default)] size: DialogSize,
    #[props(default)] restore_focus: ReadSignal<Option<Rc<MountedData>>>,
    children: Element,
) -> Element {
    // Scope IDs are stable for the lifetime of a mounted component and avoid duplicate
    // aria-labelledby/aria-describedby targets when a page renders more than one dialog.
    let scope_id = current_scope_id().0;
    let dialog_id = format!("erpx-dialog-{scope_id}");
    let title_id = format!("erpx-dialog-title-{scope_id}");
    let body_id = format!("erpx-dialog-body-{scope_id}");
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

    let title = safe_display_text(&title);
    let close_label = dialog_close_label();
    let focus_restore = if restore_focus.read().is_some() {
        "true"
    } else {
        "caller-target-required"
    };
    rsx! {
        KitDialog {
            id: dialog_id,
            open,
            on_open_change: move |next_open: bool| if !next_open { on_close.call(()); },
            class: "ui-dialog",
            aria_labelledby: title_id.clone(),
            aria_describedby: body_id.clone(),
            aria_keyshortcuts: "Escape",
            "data-size": size.css_token(),
            "data-erpx-component": "dialog",
            "data-dialog-backdrop": "true",
            "data-dialog-focus-trap": "true",
            "data-dialog-focus-restore": focus_restore,
            header { class: "ui-dialog-header",
                KitDialogTitle { id: title_id, "{title}" }
                KitButton {
                    class: "ui-icon-button",
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::IconSm,
                    r#type: "button",
                    aria_label: "{close_label}",
                    "data-erpx-component": "button",
                    "data-tone": "ghost",
                    "data-size": "icon-sm",
                    onclick: move |_| on_close.call(()),
                    "×"
                }
            }
            section { id: "{body_id}", class: "ui-dialog-body", {children} }
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
                KitButton { variant: ButtonVariant::Secondary, size: ButtonSize::Default,
                    class: "ui-button",
                    r#type: "button",
                    aria_label: "{label.as_str()}",
                    onclick: move |event: MouseEvent| on_action.call(event),
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
    let active_path = erpx_ui_core::normalize_path(active_path);
    let item_path = erpx_ui_core::normalize_path(item_path);
    active_path == item_path
        || (item_path != "/"
            && active_path.starts_with(item_path)
            && active_path.as_bytes().get(item_path.len()) == Some(&b'/'))
}

#[cfg(test)]
mod tests {
    use super::{
        SafeUiText, allowed_scope_selection, authorized_group_items, is_active,
        is_administration_group, safe_action_label, safe_banner_text, safe_field_name,
        safe_input_value, safe_opaque_value, visible_sidebar_items,
    };
    use erpx_ui_core::{NavAccess, NavGroup, NavItem};

    #[test]
    fn context_banner_text_is_redacted_and_bounded_before_rendering() {
        assert_eq!(safe_banner_text("access_token=secret"), "[redacted]");
        assert!(safe_banner_text(&"a".repeat(600)).ends_with('…'));
    }

    #[test]
    fn navigation_activation_is_segment_aware() {
        assert!(is_active("/platform/holdings", "/platform"));
        assert!(!is_active("/platforming", "/platform"));
        assert!(!is_active("/operations", "/"));
    }

    #[test]
    fn daily_navigation_hides_locked_items_and_preserves_the_active_overflow_item() {
        let mut items = (0..7)
            .map(|index| {
                NavItem::new(
                    format!("Module {index}"),
                    format!("/modules/module-{index}"),
                    "module",
                )
            })
            .collect::<Vec<_>>();
        items.push(
            NavItem::new("Locked", "/modules/locked", "module")
                .with_access(NavAccess::locked(["finance_viewer"])),
        );
        let group = NavGroup::new("Business Modules", items);
        let authorized = authorized_group_items(&group);
        let visible = visible_sidebar_items(&authorized, "/modules/module-6", false);

        assert_eq!(authorized.len(), 7);
        assert_eq!(visible.len(), 5);
        assert_eq!(
            visible.last().map(|item| item.path.as_str()),
            Some("/modules/module-6")
        );
        assert!(visible.iter().all(|item| item.is_authorized()));
        assert!(is_administration_group("holding_administration"));
        assert!(!is_administration_group("business_modules"));
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
    fn kit_buttons_use_explicit_governed_variants_and_sizes() {
        let marker = ["Kit", "Button {"].concat();
        for (path, source) in [
            ("components.rs", include_str!("components.rs")),
            ("premium.rs", include_str!("premium.rs")),
            (
                "support_surfaces.rs",
                include_str!("../../ui-domains/support/src/support_surfaces.rs"),
            ),
        ] {
            for (index, segment) in source.split(&marker).skip(1).enumerate() {
                let head = segment.get(..segment.len().min(1_600)).unwrap_or(segment);
                assert!(
                    head.contains("variant:") || head.contains("\n            variant,"),
                    "{path} KitButton #{index} has no explicit variant"
                );
                assert!(
                    head.contains("size:") || head.contains("\n            size,"),
                    "{path} KitButton #{index} has no explicit size"
                );
            }
            let forbidden_classes = [
                ["class: \"button ", "primary"].concat(),
                ["class: \"button ", "secondary"].concat(),
                ["class: \"button ", "danger"].concat(),
                ["class: \"ui-button ", "primary"].concat(),
                ["class: \"ui-button ", "danger"].concat(),
                ["class: \"link", "-button"].concat(),
            ];
            for forbidden in forbidden_classes {
                assert!(!source.contains(&forbidden), "{path} retains {forbidden}");
            }
        }
    }
}
