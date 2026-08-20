#[cfg(feature = "server")]
use std::collections::BTreeSet;
#[cfg(any(feature = "web", feature = "server"))]
use std::time::Duration;
use std::{cell::RefCell, rc::Rc, str::FromStr};

use dioxus::prelude::*;
use erpx_console_feature_identity_access::{
    IdentityAccessDetailSurface, IdentityAccessListSurface, IdentityCommandFeedback,
    IdentityPrincipalCommandIntent, IdentityRecordKind,
};
use erpx_console_feature_platform_administration::{HoldingsSurface, PlatformHoldingDetailSurface};
#[cfg(feature = "server")]
use erpx_delivery_contract::Page;
#[cfg(feature = "native")]
use erpx_delivery_contract::{BootstrapRequest, ConsoleGateway};
use erpx_delivery_contract::{
    ClientChannel, PublicMarketingPreviewRequest, QueryRequest, ResourceName, RouteData,
    RouteQuery, RouteUuid, SessionBootstrap, SessionCache, Workspace,
};
use erpx_ui_core::localized_role_label;
use erpx_ui_core::{Locale, ThemeMode, provide_corporate_identity, provide_locale, provide_theme};
use erpx_ui_kit::kit::TextArea as KitTextArea;
use erpx_ui_kit::{
    KitProvider, Link as KitLink, LocaleToggle, SharedStylesheets,
    button::{Button as KitButton, ButtonVariant},
    input::Input as KitInput,
    kit::collapsible::{Collapsible, CollapsibleContent, CollapsibleTrigger},
};
use serde::Deserialize;

#[cfg(any(feature = "web", feature = "server"))]
type RouteCache = Rc<RefCell<SessionCache<String, RouteData>>>;

fn authorized_workspaces(session: &SessionBootstrap) -> Vec<Workspace> {
    let Some(navigation) = session.authorized_navigation.as_ref() else {
        return Vec::new();
    };
    Workspace::ALL
        .into_iter()
        .filter(|workspace| workspace.available_on(session.channel))
        .filter(|workspace| {
            let expected = format!("/{workspace}");
            navigation
                .groups
                .iter()
                .flat_map(|group| group.routes.iter())
                .any(|route| route.path == expected)
        })
        .collect()
}

fn session_authorizes_path(session: &SessionBootstrap, path: &str) -> bool {
    session
        .authorized_navigation
        .as_ref()
        .is_some_and(|navigation| {
            navigation
                .groups
                .iter()
                .flat_map(|group| group.routes.iter())
                .any(|route| {
                    path == route.path
                        || (path.starts_with("/identity/") && route.path == "/identity/principal")
                        || path
                            .strip_prefix(route.path.as_str())
                            .is_some_and(|suffix| suffix.starts_with('/'))
                })
        })
}

fn platform_route_href(session: &SessionBootstrap, path: &str) -> String {
    let origin = session.platform_origin.trim_end_matches('/');
    if origin.is_empty() {
        path.to_owned()
    } else {
        format!("{origin}{path}")
    }
}

#[rustfmt::skip]
#[derive(Clone, Debug, PartialEq, Routable)]
pub enum AppRoute {
    #[redirect("/", || AppRoute::Work {})]
    #[layout(PublicLayout)]
        #[route("/public/marketing/form/:id")]
        PublicMarketingForm { id: RouteUuid },
        #[route("/public/marketing/preview/:id?:token")]
        PublicMarketingPreview {
            id: RouteUuid,
            token: Option<String>,
        },
    #[end_layout]
    #[layout(AuthLayout)]
        #[route("/auth/sign-in?:return_to&:auth_error&:add_account")]
        SignIn {
            return_to: Option<String>,
            auth_error: Option<String>,
            add_account: Option<String>,
        },
        #[route("/auth/accounts?:return_to")]
        Accounts { return_to: Option<String> },
        #[route("/auth/security")]
        Security {},
        #[route("/auth/devices?:cursor")]
        Devices { cursor: Option<String> },
        #[route("/auth/recovery")]
        Recovery {},
    #[end_layout]
    #[layout(ConsoleShell)]
        #[route("/work")]
        Work {},
        #[route("/processes")]
        Processes {},
        #[route("/insights")]
        Insights {},
        #[route("/modules")]
        Modules {},
        #[route("/platform/holdings?:cursor")]
        PlatformHoldings { cursor: Option<String> },
        #[route("/platform/holdings/:holding_id")]
        PlatformHolding { holding_id: RouteUuid },
        #[redirect("/identity", || AppRoute::IdentityPrincipals { cursor: None })]
        #[route("/identity/principal?:cursor")]
        IdentityPrincipals { cursor: Option<String> },
        #[route("/identity/principal/:id")]
        IdentityPrincipal { id: RouteUuid },
        #[route("/identity/role?:cursor")]
        IdentityRoles { cursor: Option<String> },
        #[route("/identity/role/:id")]
        IdentityRole { id: RouteUuid },
        #[route("/identity/grant?:cursor")]
        IdentityGrants { cursor: Option<String> },
        #[route("/identity/grant/:id")]
        IdentityGrant { id: RouteUuid },
        #[route("/identity/access-review?:cursor")]
        IdentityAccessReviews { cursor: Option<String> },
        #[route("/identity/access-review/:id")]
        IdentityAccessReview { id: RouteUuid },
        #[route("/identity/break-glass-session?:cursor")]
        IdentityBreakGlassSessions { cursor: Option<String> },
        #[route("/identity/break-glass-session/:id")]
        IdentityBreakGlassSession { id: RouteUuid },
        #[route("/reference/item/:id")]
        ReferenceItem { id: RouteUuid },
        #[layout(PlatformLayout)]
            #[route("/operations")]
            OperationsWorkspace {},
            #[route("/finance")]
            FinanceWorkspace {},
            #[route("/sales")]
            SalesWorkspace {},
            #[route("/procurement")]
            ProcurementWorkspace {},
            #[route("/inventory")]
            InventoryWorkspace {},
            #[route("/asset")]
            AssetWorkspace {},
            #[route("/maintenance")]
            MaintenanceWorkspace {},
            #[route("/quality")]
            QualityWorkspace {},
            #[route("/marketing")]
            MarketingWorkspace {},
            #[route("/people")]
            PeopleWorkspace {},
            #[route("/pos")]
            PosWorkspace {},
            #[route("/legal")]
            LegalWorkspace {},
            #[route("/manufacturing")]
            ManufacturingWorkspace {},
            #[route("/planning")]
            PlanningWorkspace {},
            #[route("/project")]
            ProjectWorkspace {},
            #[route("/commerce")]
            CommerceWorkspace {},
            #[route("/talent")]
            TalentWorkspace {},
            #[route("/learning")]
            LearningWorkspace {},
            #[route("/benefits")]
            BenefitsWorkspace {},
            #[route("/field-service")]
            FieldServiceWorkspace {},
            #[route("/ai")]
            AiWorkspace {},
            #[route("/:workspace/:resource")]
            WorkspaceResource { workspace: Workspace, resource: ResourceName },
            #[route("/:workspace/:resource/:id")]
            WorkspaceRecord {
                workspace: Workspace,
                resource: ResourceName,
                id: RouteUuid,
            },
        #[end_layout]
    #[end_layout]
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
pub fn App() -> Element {
    #[cfg(any(feature = "web", feature = "server"))]
    let session = {
        let loader = use_loader(|| crate::delivery::session_bootstrap("en".to_owned()))?;
        loader.read().clone()
    };

    #[cfg(feature = "native")]
    let session = {
        let gateway =
            use_context_provider(crate::native_delivery::NativeGrpcGateway::from_environment);
        let gateway = gateway.clone().map_err(render_delivery_error)?;
        use_context_provider(|| gateway.clone());
        let session = use_resource(move || {
            let gateway = gateway.clone();
            async move {
                gateway
                    .bootstrap(BootstrapRequest {
                        locale: "en".to_owned(),
                    })
                    .await
            }
        });
        session
            .suspend()?
            .read()
            .clone()
            .map_err(render_delivery_error)?
    };

    let cache = use_context_provider(|| {
        Rc::new(RefCell::new(SessionCache::<String, RouteData>::default()))
    });
    cache.borrow_mut().set_scope(session.cache_scope());
    provide_locale(Locale::from_language_tag(&session.locale).unwrap_or_default());
    #[cfg(target_arch = "wasm32")]
    {
        let locale = erpx_ui_core::use_locale();
        use_effect(move || {
            crate::platform::browser_http::persist_locale(locale.current().language_tag());
        });
    }
    provide_theme(ThemeMode::System);
    provide_corporate_identity(
        erpx_domain::corporate_identity::ResolvedCorporateIdentity::neutral_erpx(),
    );
    use_context_provider(|| session);

    rsx! {
        SharedStylesheets {}
        document::Link {
            rel: "icon",
            r#type: "image/svg+xml",
            href: asset!("/assets/favicon.svg")
        }
        document::Stylesheet { href: asset!("/assets/console-surfaces.css") }
        KitProvider {
            ErrorBoundary {
                handle_error: |errors: ErrorContext| rsx! {
                    main { class: "route-error", role: "alert",
                        h1 { "This route is temporarily unavailable" }
                        p { "Correlation details are retained by the delivery boundary." }
                        KitButton {
                            variant: ButtonVariant::Primary,
                            r#type: "button",
                            onclick: move |_| errors.clear_errors(),
                            "Try again"
                        }
                    }
                },
                SuspenseBoundary {
                    fallback: |_| rsx! {
                        main { class: "route-loading", aria_busy: "true", "Loading workspace…" }
                    },
                    Router::<AppRoute> {}
                }
            }
        }
    }
}

#[component]
fn PublicLayout() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let theme = erpx_ui_core::use_theme();
    rsx! {
        div { class: "public-layout",
            header { class: "public-layout__header",
                KitLink { href: "/auth/sign-in", class: "public-layout__brand",
                    span { class: "public-layout__mark", aria_hidden: "true", "E" }
                    span {
                        strong { "ERPX" }
                        small { {copy(locale, "Enterprise console", "คอนโซลองค์กร")} }
                    }
                }
                div { class: "public-layout__controls",
                    LocaleToggle {}
                    KitButton {
                        variant: ButtonVariant::Secondary,
                        class: "theme-toggle",
                        r#type: "button",
                        aria_label: copy(locale, "Change color mode", "เปลี่ยนโหมดสี"),
                        title: copy(locale, "Change color mode", "เปลี่ยนโหมดสี"),
                        onclick: move |_| theme.toggle(),
                        span { aria_hidden: "true", "◐" }
                        span { class: "theme-toggle__label", {theme_mode_label(locale, theme.current())} }
                    }
                }
            }
            Outlet::<AppRoute> {}
        }
    }
}

#[component]
fn AuthLayout() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let theme = erpx_ui_core::use_theme();
    rsx! {
        main { class: "auth-layout",
            aside { class: "auth-layout__brand",
                p { class: "auth-layout__eyebrow", "ERPX CONTROL PLANE" }
                h1 { {copy(locale, "Secure business operations", "ศูนย์กลางงานธุรกิจที่ปลอดภัย")} }
                p { class: "auth-layout__summary",
                    {copy(locale,
                        "One trusted workspace for finance, operations, people, and every critical decision.",
                        "พื้นที่ทำงานที่เชื่อถือได้หนึ่งเดียว สำหรับการเงิน ปฏิบัติการ บุคลากร และทุกการตัดสินใจสำคัญ",
                    )}
                }
            }
            section { class: "auth-layout__content",
                div { class: "auth-layout__toolbar",
                    LocaleToggle {}
                    KitButton {
                        variant: ButtonVariant::Secondary,
                        class: "theme-toggle",
                        r#type: "button",
                        aria_label: copy(locale, "Change color mode", "เปลี่ยนโหมดสี"),
                        title: copy(locale, "Change color mode", "เปลี่ยนโหมดสี"),
                        onclick: move |_| theme.toggle(),
                        span { aria_hidden: "true", "◐" }
                        span { class: "theme-toggle__label", {theme_mode_label(locale, theme.current())} }
                    }
                }
                Outlet::<AppRoute> {}
            }
        }
    }
}

#[component]
fn ConsoleShell() -> Element {
    let session = use_context::<SessionBootstrap>();
    if !session.authenticated {
        return rsx! {
            main { class: "route-denied", role: "alert",
                h1 { "Sign in required" }
                p { "This route requires an authenticated ERPX session." }
                KitLink { href: "/auth/sign-in?return_to=/work", "Go to sign in" }
            }
        };
    }
    if session.role_ref.is_none() || session.data_profile_key.is_none() {
        return rsx! {
            main { class: "route-denied", role: "alert",
                h1 { "Select an account role" }
                p { "Choose an authorized holding and role before opening a workspace." }
                KitLink { href: "/auth/accounts", "Open accounts" }
            }
        };
    }
    let locale = erpx_ui_core::use_locale().current();
    let theme = erpx_ui_core::use_theme();
    let active_route = use_route::<AppRoute>();
    let actor_ref = session.actor_ref.as_deref().unwrap_or("Unknown account");
    let role_ref = session.role_ref.as_deref().unwrap_or("Unknown role");
    let data_profile_key = session
        .data_profile_key
        .as_deref()
        .unwrap_or("Unknown scope");
    let actor_initial = actor_ref
        .chars()
        .find(|character| character.is_alphanumeric())
        .unwrap_or('A')
        .to_ascii_uppercase();
    let workspaces = authorized_workspaces(&session);
    let workspace_count = workspaces.len();
    let platform_holdings_authorized = session_authorizes_path(&session, "/platform/holdings");
    let platform_holdings_href = platform_route_href(&session, "/platform/holdings");
    let identity_authorized = session_authorizes_path(&session, "/identity/principal");
    rsx! {
        div { class: "console-shell",
            KitLink { class: "skip-link", href: "#main-content", {copy(locale, "Skip to content", "ข้ามไปยังเนื้อหา")} }
            aside { class: "console-nav",
                div { class: "console-nav__brand",
                    span { class: "console-nav__mark", aria_hidden: "true", "E" }
                    div {
                        strong { "ERPX" }
                        small { {copy(locale, "Enterprise console", "คอนโซลองค์กร")} }
                    }
                }
                nav { class: "console-nav__primary", aria_label: copy(locale, "Primary", "เมนูหลัก"),
                    KitLink {
                        href: "/work",
                        class: nav_link_class(matches!(&active_route, AppRoute::Work {})),
                        aria_current: nav_aria_current(matches!(&active_route, AppRoute::Work {})),
                        {copy(locale, "Work", "งาน")}
                    }
                    KitLink {
                        href: "/processes",
                        class: nav_link_class(matches!(&active_route, AppRoute::Processes {})),
                        aria_current: nav_aria_current(matches!(&active_route, AppRoute::Processes {})),
                        {copy(locale, "Processes", "กระบวนการ")}
                    }
                    KitLink {
                        href: "/insights",
                        class: nav_link_class(matches!(&active_route, AppRoute::Insights {})),
                        aria_current: nav_aria_current(matches!(&active_route, AppRoute::Insights {})),
                        {copy(locale, "Insights", "ข้อมูลเชิงลึก")}
                    }
                    KitLink {
                        href: "/modules",
                        class: nav_link_class(matches!(&active_route, AppRoute::Modules {})),
                        aria_current: nav_aria_current(matches!(&active_route, AppRoute::Modules {})),
                        {copy(locale, "Modules", "โมดูล")}
                    }
                    if platform_holdings_authorized {
                        KitLink {
                            href: platform_holdings_href,
                            class: nav_link_class(matches!(&active_route, AppRoute::PlatformHoldings { .. } | AppRoute::PlatformHolding { .. })),
                            aria_current: nav_aria_current(matches!(&active_route, AppRoute::PlatformHoldings { .. } | AppRoute::PlatformHolding { .. })),
                            {copy(locale, "Platform holdings", "โฮลดิ้งบนแพลตฟอร์ม")}
                        }
                    }
                    if identity_authorized {
                        KitLink {
                            href: "/identity/principal",
                            class: nav_link_class(route_is_identity(&active_route)),
                            aria_current: nav_aria_current(route_is_identity(&active_route)),
                            {copy(locale, "Identity & access", "ตัวตนและสิทธิ์")}
                        }
                    }
                }
                Collapsible { class: "console-workspace-menu",
                    CollapsibleTrigger { class: "console-workspace-menu__trigger",
                        span { {copy(locale, "Workspaces", "พื้นที่ทำงาน")} }
                        small { "{workspace_count}" }
                    }
                    CollapsibleContent {
                        nav { aria_label: copy(locale, "Workspaces", "พื้นที่ทำงาน"),
                            for workspace in workspaces {
                                KitLink {
                                    href: "/{workspace}",
                                    class: nav_link_class(route_uses_workspace(&active_route, workspace)),
                                    aria_current: nav_aria_current(route_uses_workspace(&active_route, workspace)),
                                    {workspace_label(workspace, locale)}
                                }
                            }
                        }
                    }
                }
                div { class: "console-nav__account",
                    small { {copy(locale, "Signed in as", "เข้าสู่ระบบในชื่อ")} }
                    strong { "{actor_ref}" }
                    span { "{localized_role_label(role_ref, locale)}" }
                    KitLink { href: "/auth/accounts", {copy(locale, "Change role or account", "เปลี่ยนบทบาทหรือบัญชี")} }
                }
            }
            main { class: "console-main", id: "main-content", tabindex: "-1",
                header { class: "console-header",
                    div { class: "console-header__context",
                        strong { "{localized_role_label(role_ref, locale)}" }
                        span { title: "{data_profile_key}", "{role_scope_label(data_profile_key, locale)}" }
                    }
                    KitLink {
                        href: "/auth/accounts",
                        class: "console-header__account",
                        aria_label: copy(locale, "Change role or account", "เปลี่ยนบทบาทหรือบัญชี"),
                        span { aria_hidden: "true", "{actor_initial}" }
                    }
                    LocaleToggle {}
                    KitButton {
                        variant: ButtonVariant::Secondary,
                        class: "console-theme-toggle",
                        r#type: "button",
                        aria_label: copy(locale, "Change color mode", "เปลี่ยนโหมดสี"),
                        title: theme_mode_label(locale, theme.current()),
                        onclick: move |_| theme.toggle(),
                        span { aria_hidden: "true", "◐" }
                    }
                    span { class: "console-header__version", "Delivery v{session.catalog_version}" }
                }
                SuspenseBoundary {
                    fallback: |_| rsx! {
                        section { class: "route-loading", aria_busy: "true", "Loading route…" }
                    },
                    Outlet::<AppRoute> {}
                }
            }
        }
    }
}

#[component]
fn PlatformLayout() -> Element {
    rsx! {
        section { class: "platform-layout", "data-renderer-boundary": "workspace",
            Outlet::<AppRoute> {}
        }
    }
}

#[component]
fn ProtectedRouteSurface(query: RouteQuery) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let mut search = use_signal(String::new);
    let normalized_search = use_memo(move || search.read().trim().to_ascii_lowercase());
    let query_for_display = query.clone();
    let record_path_prefix = match &query_for_display {
        RouteQuery::Workspace {
            workspace,
            resource: Some(resource),
        } => Some(format!("/{workspace}/{resource}")),
        RouteQuery::Workspace {
            workspace,
            resource: None,
        } => Some(format!(
            "/{workspace}/{}",
            default_workspace_resource(*workspace)
        )),
        RouteQuery::PlatformHoldings => Some("/platform/holdings".to_owned()),
        _ => None,
    };
    let detail_back_href = match &query_for_display {
        RouteQuery::PlatformHolding { .. } => Some("/platform/holdings".to_owned()),
        RouteQuery::Record {
            workspace,
            resource,
            ..
        } => Some(format!("/{workspace}/{resource}")),
        RouteQuery::Reference { .. } => Some("/work".to_owned()),
        _ => None,
    };
    let is_detail = detail_back_href.is_some();
    let data = use_route_data(query)?;
    let heading = route_heading(&query_for_display, &data.title, locale);
    let description = route_description(&query_for_display, &heading, locale);
    let has_items = !data.page.items.is_empty();
    let returned_count = format!(
        "{} {}",
        data.page.items.len(),
        copy(locale, "records returned", "รายการที่ได้รับ")
    );
    let freshness_label = match data.freshness_tag.as_str() {
        "not-connected" => copy(
            locale,
            "Authoritative projection not connected",
            "ยังไม่ได้เชื่อมต่อข้อมูล projection ที่เชื่อถือได้",
        )
        .to_owned(),
        value => format!("{}: {value}", copy(locale, "Status", "สถานะ")),
    };
    let visible_items = data
        .page
        .items
        .iter()
        .filter(|item| {
            normalized_search.read().is_empty()
                || item
                    .title
                    .to_ascii_lowercase()
                    .contains(normalized_search.read().as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    let visible_count = format!(
        "{} {}",
        visible_items.len(),
        copy(locale, "visible records", "รายการที่แสดง")
    );

    rsx! {
        document::Title { "{heading} · ERPX" }
        article { class: "workspace-route", "data-freshness": "{data.freshness_tag}",
            nav { class: "route-breadcrumb", aria_label: copy(locale, "Breadcrumb", "เส้นทางนำทาง"),
                KitLink { href: "/modules", {copy(locale, "Modules", "โมดูล")} }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", "{heading}" }
            }
            header {
                p { class: "eyebrow", {copy(locale, "AUTHORIZED WORKSPACE", "พื้นที่ทำงานตามสิทธิ์")} }
                h1 { "{heading}" }
                p { "{description}" }
            }
            div { class: "route-meta", aria_label: copy(locale, "Data status", "สถานะข้อมูล"),
                span { "{freshness_label}" }
                span { "{returned_count}" }
            }
            if has_items && !is_detail {
                div { class: "route-toolbar",
                    label { class: "route-filter",
                        span { {copy(locale, "Filter records", "กรองรายการ")} }
                        KitInput {
                            value: "{search}",
                            placeholder: copy(locale, "Search by title", "ค้นหาจากชื่อ"),
                            oninput: move |event: FormEvent| search.set(event.value()),
                        }
                    }
                    p { class: "route-result-count", "{visible_count}" }
                }
            }
            if is_detail && has_items {
                if let Some(item) = visible_items.first() {
                    section { class: "record-detail",
                        div { class: "record-detail__heading",
                            div {
                                p { class: "eyebrow", {copy(locale, "RECORD OVERVIEW", "ภาพรวมรายการ")} }
                                h2 { "{item.title}" }
                            }
                            span { class: "record-status", "{item.status}" }
                        }
                        dl { class: "record-detail__facts",
                            div {
                                dt { {copy(locale, "Record ID", "รหัสรายการ")} }
                                dd { code { "{item.id}" } }
                            }
                            div {
                                dt { {copy(locale, "Version", "เวอร์ชัน")} }
                                dd { "{item.version}" }
                            }
                            div {
                                dt { {copy(locale, "Data source", "แหล่งข้อมูล")} }
                                dd { {copy(locale, "Authoritative projection", "ข้อมูล projection ที่เชื่อถือได้")} }
                            }
                        }
                        div { class: "route-actions",
                            if let Some(back_href) = &detail_back_href {
                                KitLink { class: "route-action route-action--primary", href: "{back_href}", {copy(locale, "Back to list", "กลับไปยังรายการ")} }
                            }
                            KitLink { class: "route-action", href: "/modules", {copy(locale, "Browse modules", "ดูโมดูล")} }
                        }
                    }
                }
            } else if visible_items.is_empty() {
                section { class: "empty-state", role: "status",
                    span { class: "empty-state__icon", aria_hidden: "true", if has_items { "⌕" } else { "◇" } }
                    div { class: "empty-state__copy",
                        if has_items {
                            h2 { {copy(locale, "No matching records", "ไม่พบรายการที่ตรงกัน")} }
                            p { {copy(locale,
                                "Try a shorter title or clear the filter to show every available record.",
                                "ลองใช้คำค้นที่สั้นลง หรือล้างตัวกรองเพื่อแสดงรายการทั้งหมดที่พร้อมใช้",
                            )} }
                        } else {
                            h2 { {copy(locale, "No records available", "ยังไม่มีรายการที่พร้อมใช้")} }
                            p { {copy(locale,
                                "No records are available for the active role and scope yet. Try another workspace or review your access.",
                                "ยังไม่มีรายการสำหรับบทบาทและขอบเขตที่กำลังใช้ ลองพื้นที่ทำงานอื่นหรือตรวจสอบสิทธิ์ของคุณ",
                            )} }
                        }
                    }
                    div { class: "route-actions",
                        if has_items {
                            KitButton {
                                variant: ButtonVariant::Secondary,
                                r#type: "button",
                                onclick: move |_| search.set(String::new()),
                                {copy(locale, "Clear filter", "ล้างตัวกรอง")}
                            }
                        } else {
                            KitLink { class: "route-action route-action--primary", href: "/modules", {copy(locale, "Browse modules", "ดูโมดูล")} }
                            KitLink { class: "route-action", href: "/auth/accounts", {copy(locale, "Review access", "ตรวจสอบสิทธิ์")} }
                        }
                    }
                }
            } else {
                ul { class: "record-list",
                    for item in visible_items {
                        li { class: "record-row", key: "{item.id}",
                            if let Some(prefix) = &record_path_prefix {
                                KitLink { href: "{prefix}/{item.id}",
                                    span { class: "record-row__copy",
                                        strong { "{item.title}" }
                                        small { "v{item.version} · {item.id}" }
                                    }
                                    span { class: "record-row__arrow", aria_hidden: "true", "→" }
                                }
                            } else {
                                span { class: "record-row__copy record-row__copy--static",
                                    strong { "{item.title}" }
                                    small { "v{item.version} · {item.id}" }
                                }
                            }
                            span { class: "record-status", "{item.status}" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn use_route_data(query: RouteQuery) -> Result<RouteData, RenderError> {
    let cache = use_context::<RouteCache>();
    let locale = erpx_ui_core::use_locale().current();
    let key = format!(
        "{}?locale={}",
        canonical_path_for_query(&query),
        locale.language_tag()
    );
    let request = QueryRequest {
        route: query,
        cursor: None,
        limit: 50,
        locale: locale.language_tag().to_owned(),
    };
    let resource: Resource<Result<RouteData, String>> = use_server_future(move || {
        let cache = Rc::clone(&cache);
        let key = key.clone();
        let request = request.clone();
        async move {
            if let Some(value) = cache.borrow_mut().get_fresh(&key) {
                return Ok::<RouteData, String>(value);
            }
            let value = crate::delivery::query_route(request)
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            let byte_size = value.title.len()
                + value.description.len()
                + value
                    .page
                    .items
                    .iter()
                    .map(|item| item.title.len() + 64)
                    .sum::<usize>();
            cache.borrow_mut().insert(
                key,
                value.clone(),
                byte_size,
                Duration::from_secs(30),
                value.invalidation_tags.iter().cloned(),
            );
            Ok::<RouteData, String>(value)
        }
    })?;
    resource
        .read()
        .clone()
        .expect("resolved server future has a value")
        .map_err(|error| RenderError::from(std::io::Error::other(error)))
}

#[cfg(feature = "native")]
fn use_route_data(query: RouteQuery) -> Result<RouteData, RenderError> {
    let gateway = use_context::<crate::native_delivery::NativeGrpcGateway>();
    let locale = erpx_ui_core::use_locale().current();
    let resource = use_resource(move || {
        let gateway = gateway.clone();
        let query = query.clone();
        async move {
            gateway
                .query(QueryRequest {
                    route: query,
                    cursor: None,
                    limit: 50,
                    locale: locale.language_tag().to_owned(),
                })
                .await
        }
    });
    resource
        .suspend()?
        .read()
        .clone()
        .map_err(render_delivery_error)
}

#[component]
fn Work() -> Element {
    rsx! { ProtectedRouteSurface { query: RouteQuery::Work } }
}

#[component]
fn Processes() -> Element {
    rsx! { ProtectedRouteSurface { query: RouteQuery::Processes } }
}

#[component]
fn Insights() -> Element {
    rsx! { ProtectedRouteSurface { query: RouteQuery::Insights } }
}

#[component]
fn Modules() -> Element {
    let session = use_context::<SessionBootstrap>();
    let locale = erpx_ui_core::use_locale().current();
    let mut search = use_signal(String::new);
    let normalized_search = use_memo(move || search.read().trim().to_ascii_lowercase());
    let available = authorized_workspaces(&session);
    let available_count = format!(
        "{} {}",
        available.len(),
        copy(locale, "workspaces available", "พื้นที่ทำงานที่พร้อมใช้")
    );
    let visible = available
        .iter()
        .copied()
        .filter(|workspace| {
            normalized_search.read().is_empty()
                || workspace_label(*workspace, locale)
                    .to_ascii_lowercase()
                    .contains(normalized_search.read().as_str())
        })
        .collect::<Vec<_>>();
    let visible_count = format!(
        "{} {} {}",
        visible.len(),
        copy(locale, "of", "จาก"),
        available_count
    );
    let document_title = format!("{} · ERPX", copy(locale, "Modules", "โมดูล"));
    rsx! {
        document::Title { "{document_title}" }
        article { class: "workspace-route module-catalog",
            header {
                p { class: "eyebrow", {copy(locale, "AUTHORIZED CATALOG", "แค็ตตาล็อกตามสิทธิ์")} }
                h1 { {copy(locale, "Modules and workspaces", "โมดูลและพื้นที่ทำงาน")} }
                p { {copy(locale,
                    "Open an available workspace. Access is still checked by your active role and scope.",
                    "เปิดพื้นที่ทำงานที่พร้อมใช้ โดยระบบยังตรวจสอบบทบาทและขอบเขตที่ใช้งานอยู่ทุกครั้ง",
                )} }
            }
            div { class: "module-catalog__toolbar",
                label { class: "module-catalog__search",
                    span { {copy(locale, "Find a workspace", "ค้นหาพื้นที่ทำงาน")} }
                    KitInput {
                        value: "{search}",
                        placeholder: copy(locale, "Search modules", "ค้นหาโมดูล"),
                        oninput: move |event: FormEvent| search.set(event.value()),
                    }
                }
                p { class: "module-catalog__count", "{visible_count}" }
            }
            if visible.is_empty() {
                section { class: "empty-state", role: "status",
                    span { class: "empty-state__icon", aria_hidden: "true", "⌕" }
                    div { class: "empty-state__copy",
                        h2 { {copy(locale, "No matching workspace", "ไม่พบพื้นที่ทำงานที่ตรงกัน")} }
                        p { {copy(locale, "Try another name or clear the search.", "ลองใช้ชื่ออื่นหรือล้างคำค้น")} }
                    }
                    div { class: "route-actions",
                        KitButton {
                            variant: ButtonVariant::Secondary,
                            r#type: "button",
                            onclick: move |_| search.set(String::new()),
                            {copy(locale, "Clear search", "ล้างคำค้น")}
                        }
                    }
                }
            } else {
                nav { class: "module-catalog__grid", aria_label: copy(locale, "Available workspaces", "พื้นที่ทำงานที่พร้อมใช้"),
                    for workspace in visible {
                        KitLink { href: "/{workspace}", class: "module-card",
                            span { class: "module-card__mark", aria_hidden: "true", "{workspace_label(workspace, locale).chars().next().unwrap_or('E')}" }
                            span {
                                strong { {workspace_label(workspace, locale)} }
                                small { {copy(locale, "Open workspace", "เปิดพื้นที่ทำงาน")} }
                            }
                            span { class: "module-card__arrow", aria_hidden: "true", "→" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn feature_list_request(
    cursor: Option<String>,
    locale: Locale,
) -> crate::feature_delivery::FeatureListRequest {
    crate::feature_delivery::FeatureListRequest {
        cursor,
        page_size: 25,
        locale: locale.language_tag().to_owned(),
        consistency: crate::feature_delivery::FeatureConsistency::Eventual,
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn feature_detail_request(
    id: RouteUuid,
    locale: Locale,
) -> crate::feature_delivery::FeatureDetailRequest {
    crate::feature_delivery::FeatureDetailRequest {
        id: id.get(),
        locale: locale.language_tag().to_owned(),
        consistency: crate::feature_delivery::FeatureConsistency::Eventual,
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn feature_error(error: impl std::fmt::Display) -> RenderError {
    RenderError::from(std::io::Error::other(error.to_string()))
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn PlatformHoldings(cursor: Option<String>) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let title = format!(
        "{} · ERPX",
        copy(locale, "Platform holdings", "โฮลดิ้งบนแพลตฟอร์ม")
    );
    let request = feature_list_request(cursor, locale);
    let resource = use_server_future(move || {
        let request = request.clone();
        async move {
            crate::feature_delivery::list_platform_holdings(request)
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())
        }
    })?;
    let page = resource
        .read()
        .clone()
        .expect("resolved platform holdings future has a value")
        .map_err(feature_error)?;
    rsx! {
        document::Title { "{title}" }
        HoldingsSurface { page }
    }
}

#[cfg(feature = "native")]
#[component]
fn PlatformHoldings(_cursor: Option<String>) -> Element {
    rsx! { FeatureClosed { name: "Platform holdings" } }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn PlatformHolding(holding_id: RouteUuid) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = feature_detail_request(holding_id, locale);
    let resource = use_server_future(move || {
        let request = request.clone();
        async move {
            crate::feature_delivery::get_platform_holding(request)
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())
        }
    })?;
    let (holding, evidence) = resource
        .read()
        .clone()
        .expect("resolved platform holding future has a value")
        .map_err(feature_error)?;
    let page_title = holding.customer_name.clone();
    rsx! {
        document::Title { "{page_title} · ERPX" }
        PlatformHoldingDetailSurface { holding, evidence }
    }
}

#[cfg(feature = "native")]
#[component]
fn PlatformHolding(_holding_id: RouteUuid) -> Element {
    rsx! { FeatureClosed { name: "Platform holding detail" } }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn IdentityList(kind: IdentityRecordKind, cursor: Option<String>) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = feature_list_request(cursor, locale);
    let resource = use_server_future(move || {
        let request = request.clone();
        async move {
            crate::feature_delivery::list_identity_records(kind, request)
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())
        }
    })?;
    let page = resource
        .read()
        .clone()
        .expect("resolved identity list future has a value")
        .map_err(feature_error)?;
    rsx! { IdentityAccessListSurface { page } }
}

#[cfg(feature = "native")]
#[component]
fn IdentityList(_kind: IdentityRecordKind, _cursor: Option<String>) -> Element {
    rsx! { FeatureClosed { name: "Identity & access" } }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn IdentityDetail(kind: IdentityRecordKind, id: RouteUuid) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = feature_detail_request(id, locale);
    let mut resource = use_server_future(move || {
        let request = request.clone();
        async move {
            crate::feature_delivery::get_identity_record(kind, request)
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())
        }
    })?;
    let detail = resource
        .read()
        .clone()
        .expect("resolved identity detail future has a value")
        .map_err(feature_error)?;
    let page_title = detail.record.title.clone();
    let mut command_busy = use_signal(|| false);
    let mut command_feedback = use_signal(|| None::<IdentityCommandFeedback>);
    let on_principal_command = EventHandler::new(move |intent: IdentityPrincipalCommandIntent| {
        if kind != IdentityRecordKind::Principal || *command_busy.read() {
            return;
        }
        let principal_id = match uuid::Uuid::parse_str(&intent.principal_id) {
            Ok(value) if !value.is_nil() => value,
            _ => {
                command_feedback.set(Some(IdentityCommandFeedback {
                    state: "error".into(),
                    message: match locale {
                        Locale::En => "The principal identifier is invalid.",
                        Locale::Th => "รหัสตัวตนหลักไม่ถูกต้อง",
                    }
                    .into(),
                    audit_reference: None,
                }));
                return;
            }
        };
        command_busy.set(true);
        command_feedback.set(Some(IdentityCommandFeedback {
            state: "pending".into(),
            message: match locale {
                Locale::En => "Submitting the command and waiting for the owner projection…",
                Locale::Th => "กำลังส่งคำสั่งและรอ projection ของบริการเจ้าของ…",
            }
            .into(),
            audit_reference: None,
        }));
        spawn(async move {
            let result = crate::feature_delivery::command_identity_principal(
                crate::feature_delivery::IdentityPrincipalCommandRequest {
                    principal_id,
                    action: intent.action,
                    idempotency_key: intent.idempotency_key,
                    expected_version: intent.expected_version,
                    locale: locale.language_tag().into(),
                    purpose: intent.purpose,
                    mfa_code: intent.mfa_code,
                },
            )
            .await;
            command_busy.set(false);
            match result {
                Ok(Ok(outcome)) => {
                    let current = outcome.detail.is_some() && outcome.projection_state == "current";
                    command_feedback.set(Some(IdentityCommandFeedback {
                        state: if current { "success" } else { "pending" }.into(),
                        message: if current {
                            match (locale, outcome.receipt.replayed) {
                                (Locale::En, false) => "Saved and confirmed by the owner projection.",
                                (Locale::En, true) => "Already saved; the idempotent receipt was replayed and confirmed.",
                                (Locale::Th, false) => "บันทึกแล้วและยืนยันจาก projection ของบริการเจ้าของแล้ว",
                                (Locale::Th, true) => "คำสั่งนี้บันทึกไว้แล้ว ระบบ replay receipt เดิมและยืนยันจาก projection แล้ว",
                            }
                        } else {
                            match locale {
                                Locale::En => "Saved; the owner projection is still catching up. Refresh before treating the change as complete.",
                                Locale::Th => "บันทึกแล้ว แต่ projection ของบริการเจ้าของยังกำลังตามข้อมูล โปรดรีเฟรชก่อนถือว่าการเปลี่ยนแปลงเสร็จสมบูรณ์",
                            }
                        }
                        .into(),
                        audit_reference: Some(outcome.receipt.audit_reference.to_string()),
                    }));
                    resource.restart();
                }
                Ok(Err(error)) => {
                    let conflict =
                        matches!(error, erpx_delivery_contract::DeliveryError::Conflict(_));
                    command_feedback.set(Some(IdentityCommandFeedback {
                        state: "error".into(),
                        message: match (locale, conflict) {
                            (Locale::En, true) => "The record changed. The authoritative version has been reloaded; review it before retrying.",
                            (Locale::Th, true) => "รายการมีการเปลี่ยนแปลง ระบบกำลังโหลด version ที่เชื่อถือได้ใหม่ โปรดตรวจสอบก่อนลองอีกครั้ง",
                            (Locale::En, false) => "The command was not accepted. Verify authorization and the fresh MFA proof, then retry.",
                            (Locale::Th, false) => "คำสั่งไม่ได้รับการยอมรับ โปรดตรวจสอบสิทธิ์และหลักฐาน MFA ล่าสุดแล้วลองอีกครั้ง",
                        }
                        .into(),
                        audit_reference: None,
                    }));
                    if conflict {
                        resource.restart();
                    }
                }
                Err(_) => command_feedback.set(Some(IdentityCommandFeedback {
                    state: "error".into(),
                    message: match locale {
                        Locale::En => "The same-origin command transport is unavailable.",
                        Locale::Th => "ช่องทางคำสั่ง same-origin ยังไม่พร้อมใช้งาน",
                    }
                    .into(),
                    audit_reference: None,
                })),
            }
        });
    });
    rsx! {
        document::Title { "{page_title} · ERPX" }
        IdentityAccessDetailSurface {
            detail,
            on_principal_command,
            command_feedback: command_feedback(),
            command_busy: command_busy(),
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn IdentityDetail(_kind: IdentityRecordKind, _id: RouteUuid) -> Element {
    rsx! { FeatureClosed { name: "Identity & access detail" } }
}

#[component]
fn IdentityPrincipals(cursor: Option<String>) -> Element {
    rsx! { IdentityList { kind: IdentityRecordKind::Principal, cursor } }
}

#[component]
fn IdentityPrincipal(id: RouteUuid) -> Element {
    rsx! { IdentityDetail { kind: IdentityRecordKind::Principal, id } }
}

#[component]
fn IdentityRoles(cursor: Option<String>) -> Element {
    rsx! { IdentityList { kind: IdentityRecordKind::Role, cursor } }
}

#[component]
fn IdentityRole(id: RouteUuid) -> Element {
    rsx! { IdentityDetail { kind: IdentityRecordKind::Role, id } }
}

#[component]
fn IdentityGrants(cursor: Option<String>) -> Element {
    rsx! { IdentityList { kind: IdentityRecordKind::Grant, cursor } }
}

#[component]
fn IdentityGrant(id: RouteUuid) -> Element {
    rsx! { IdentityDetail { kind: IdentityRecordKind::Grant, id } }
}

#[component]
fn IdentityAccessReviews(cursor: Option<String>) -> Element {
    rsx! { IdentityList { kind: IdentityRecordKind::AccessReview, cursor } }
}

#[component]
fn IdentityAccessReview(id: RouteUuid) -> Element {
    rsx! { IdentityDetail { kind: IdentityRecordKind::AccessReview, id } }
}

#[component]
fn IdentityBreakGlassSessions(cursor: Option<String>) -> Element {
    rsx! { IdentityList { kind: IdentityRecordKind::BreakGlassSession, cursor } }
}

#[component]
fn IdentityBreakGlassSession(id: RouteUuid) -> Element {
    rsx! { IdentityDetail { kind: IdentityRecordKind::BreakGlassSession, id } }
}

#[component]
fn FeatureClosed(name: &'static str) -> Element {
    rsx! {
        section { class: "route-denied", role: "alert",
            h1 { "{name} is unavailable on this client" }
            p { "This client fails closed because the feature transport has not been certified here." }
        }
    }
}

#[component]
fn WorkspaceResource(workspace: Workspace, resource: ResourceName) -> Element {
    if !workspace_allowed(workspace) {
        return rsx! { ChannelRestricted { workspace } };
    }
    rsx! { ProtectedRouteSurface { query: RouteQuery::Workspace { workspace, resource: Some(resource) } } }
}

#[component]
fn WorkspaceRecord(workspace: Workspace, resource: ResourceName, id: RouteUuid) -> Element {
    if !workspace_allowed(workspace) {
        return rsx! { ChannelRestricted { workspace } };
    }
    rsx! { ProtectedRouteSurface { query: RouteQuery::Record { workspace, resource, id } } }
}

macro_rules! workspace_entry_route {
    ($name:ident, $workspace:ident) => {
        #[component]
        fn $name() -> Element {
            let workspace = Workspace::$workspace;
            if !workspace_allowed(workspace) {
                return rsx! { ChannelRestricted { workspace } };
            }
            rsx! { ProtectedRouteSurface { query: RouteQuery::Workspace { workspace, resource: None } } }
        }
    };
}

workspace_entry_route!(OperationsWorkspace, Operations);
workspace_entry_route!(FinanceWorkspace, Finance);
workspace_entry_route!(SalesWorkspace, Sales);
workspace_entry_route!(ProcurementWorkspace, Procurement);
workspace_entry_route!(InventoryWorkspace, Inventory);
workspace_entry_route!(AssetWorkspace, Asset);
workspace_entry_route!(MaintenanceWorkspace, Maintenance);
workspace_entry_route!(QualityWorkspace, Quality);
workspace_entry_route!(MarketingWorkspace, Marketing);
workspace_entry_route!(PeopleWorkspace, People);
workspace_entry_route!(PosWorkspace, Pos);
workspace_entry_route!(LegalWorkspace, Legal);
workspace_entry_route!(ManufacturingWorkspace, Manufacturing);
workspace_entry_route!(PlanningWorkspace, Planning);
workspace_entry_route!(ProjectWorkspace, Project);
workspace_entry_route!(CommerceWorkspace, Commerce);
workspace_entry_route!(TalentWorkspace, Talent);
workspace_entry_route!(LearningWorkspace, Learning);
workspace_entry_route!(BenefitsWorkspace, Benefits);
workspace_entry_route!(FieldServiceWorkspace, FieldService);
workspace_entry_route!(AiWorkspace, Ai);

#[component]
fn ReferenceItem(id: RouteUuid) -> Element {
    rsx! { ProtectedRouteSurface { query: RouteQuery::Reference { id } } }
}

#[component]
fn ChannelRestricted(workspace: Workspace) -> Element {
    rsx! {
        section { class: "route-denied", role: "alert",
            h1 { "{workspace} is available on Web only" }
            p { "Native clients do not load this workspace chunk or service graph." }
        }
    }
}

fn auth_error_message(code: Option<&str>) -> Option<&'static str> {
    match code {
        Some("invalid_request") => {
            Some("Check that both account and password are filled in, then try again.")
        }
        Some("authentication_rejected") => {
            Some("The account or password was not accepted. Check your details and try again.")
        }
        Some("authentication_unavailable") => {
            Some("Sign-in is temporarily unavailable. Wait a moment and try again.")
        }
        _ => None,
    }
}

fn post_auth_destination(return_to: Option<&str>) -> String {
    return_to
        .filter(|path| {
            is_canonical_console_path(path, ClientChannel::Web)
                && !path.starts_with("/auth/")
                && !path.starts_with("/public/")
        })
        .unwrap_or("/work")
        .to_owned()
}

fn encoded_return_to(destination: &str) -> String {
    url::form_urlencoded::byte_serialize(destination.as_bytes()).collect()
}

fn accounts_location(destination: &str) -> String {
    format!(
        "/auth/accounts?return_to={}",
        encoded_return_to(destination)
    )
}

fn add_account_location(destination: &str) -> String {
    format!(
        "/auth/sign-in?add_account=1&return_to={}",
        encoded_return_to(destination)
    )
}

#[component]
fn SignIn(
    return_to: Option<String>,
    auth_error: Option<String>,
    add_account: Option<String>,
) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let error_message = auth_error_message(auth_error.as_deref());
    let adding_account = add_account.as_deref() == Some("1");
    let destination = post_auth_destination(return_to.as_deref());
    let cancel_location = accounts_location(&destination);
    let document_title = if adding_account {
        format!("{} · ERPX", copy(locale, "Add account", "เพิ่มบัญชี"))
    } else {
        format!("{} · ERPX", copy(locale, "Sign in", "เข้าสู่ระบบ"))
    };
    rsx! {
        document::Title { "{document_title}" }
        form {
            class: "auth-page-form",
            method: "post",
            action: "/api/auth/password/form",
            "data-testid": "sign-in-form",
            aria_describedby: if error_message.is_some() { "sign-in-error" } else { "sign-in-intro" },
            p { class: "eyebrow", {copy(locale, "AUTHORIZED ACCESS", "การเข้าถึงที่ได้รับอนุญาต")} }
            h2 { if adding_account {
                {copy(locale, "Add another account", "เพิ่มบัญชีอื่น")}
            } else {
                {copy(locale, "Sign in", "เข้าสู่ระบบ")}
            } }
            p { id: "sign-in-intro", class: "auth-page-form__intro",
                if adding_account {
                    {copy(locale,
                        "Sign in with another ERPX account. Your existing accounts will stay available.",
                        "เข้าสู่ระบบด้วยบัญชี ERPX อื่น โดยบัญชีเดิมของคุณจะยังคงพร้อมใช้งาน",
                    )}
                } else {
                    {copy(locale,
                        "Use your ERPX account to continue to the requested workspace.",
                        "ใช้บัญชี ERPX ของคุณเพื่อไปยังพื้นที่ทำงานที่ร้องขอ",
                    )}
                }
            }
            if let Some(message) = error_message {
                div {
                    id: "sign-in-error",
                    class: "auth-form-error",
                    role: "alert",
                    aria_live: "assertive",
                    "data-testid": "sign-in-error",
                    strong { {copy(locale, "Sign-in failed", "เข้าสู่ระบบไม่สำเร็จ")} }
                    p { "{message}" }
                }
            }
            label { class: "auth-page-form__field",
                span { {copy(locale, "Account", "บัญชี")} }
                KitInput {
                    name: "principal_ref",
                    autocomplete: "username",
                    required: true,
                    "data-testid": "sign-in-account"
                }
            }
            label { class: "auth-page-form__field",
                span { {copy(locale, "Password", "รหัสผ่าน")} }
                KitInput {
                    name: "password",
                    r#type: "password",
                    autocomplete: "current-password",
                    required: true,
                    "data-testid": "sign-in-password"
                }
            }
            KitInput { name: "device_label", r#type: "hidden", value: "erpx-console" }
            KitInput { name: "return_to", r#type: "hidden", value: "{destination}" }
            KitButton {
                variant: ButtonVariant::Primary,
                r#type: "submit",
                "data-testid": "sign-in-submit",
                if adding_account {
                    {copy(locale, "Add account", "เพิ่มบัญชี")}
                } else {
                    {copy(locale, "Continue", "ดำเนินการต่อ")}
                }
            }
            if adding_account {
                KitLink { class: "auth-secondary-link", href: "{cancel_location}", {copy(locale, "Cancel", "ยกเลิก")} }
            }
            p { class: "auth-page-form__security",
                {copy(locale,
                    "Session scope and correlation evidence remain protected at the delivery boundary.",
                    "ขอบเขตเซสชันและหลักฐานการเชื่อมโยงยังคงได้รับการปกป้องที่ขอบเขตการส่งมอบ",
                )}
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AccountSessionView {
    session_id: String,
    principal_ref: String,
    active: bool,
    selected_role: Option<SelectedRoleView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SelectedRoleView {
    role_key: String,
    scope_key: String,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AccountsResponse {
    accounts: Vec<AccountSessionView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RoleOptionView {
    selection_id: String,
    source: String,
    role_key: String,
    scope_key: String,
    requires_mfa: bool,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RoleOptionsResponse {
    options: Vec<RoleOptionView>,
}

#[derive(Clone, Debug, PartialEq)]
struct AccountsPageData {
    accounts: Vec<AccountSessionView>,
    roles: Vec<RoleOptionView>,
}

#[cfg(target_arch = "wasm32")]
async fn browser_json_request<T>(
    method: &str,
    path: &str,
    body: Option<String>,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    crate::platform::browser_http::json_request(method, path, body).await
}

#[cfg(target_arch = "wasm32")]
async fn load_accounts_page() -> Result<AccountsPageData, String> {
    let accounts = browser_json_request::<AccountsResponse>("GET", "/api/accounts", None).await?;
    let roles = browser_json_request::<RoleOptionsResponse>("GET", "/api/roles", None).await?;
    Ok(AccountsPageData {
        accounts: accounts.accounts,
        roles: roles.options,
    })
}

#[cfg(target_arch = "wasm32")]
async fn activate_role_and_continue(
    selection_id: String,
    destination: String,
) -> Result<(), String> {
    let body = serde_json::json!({ "selection_id": selection_id }).to_string();
    let _: serde_json::Value =
        browser_json_request("POST", "/api/roles/activate", Some(body)).await?;
    crate::platform::browser_http::replace_location(&destination)
}

#[cfg(target_arch = "wasm32")]
async fn activate_session_account(session_id: String) -> Result<(), String> {
    let body = serde_json::json!({ "session_id": session_id }).to_string();
    let _: serde_json::Value =
        browser_json_request("POST", "/api/accounts/activate", Some(body)).await?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn remove_session_account(session_id: String) -> Result<bool, String> {
    #[derive(Deserialize)]
    struct SessionMutationView {
        authenticated: bool,
    }

    let snapshot = browser_json_request::<SessionMutationView>(
        "DELETE",
        &format!("/api/accounts/{session_id}"),
        None,
    )
    .await?;
    Ok(snapshot.authenticated)
}

fn role_scope_label(scope_key: &str, locale: Locale) -> &'static str {
    let scope = scope_key
        .split_once(':')
        .map_or(scope_key, |(scope, _)| scope);
    match (scope, locale) {
        ("platform", Locale::En) => "Platform scope",
        ("platform", Locale::Th) => "ขอบเขตแพลตฟอร์ม",
        ("holding", Locale::En) => "Holding scope",
        ("holding", Locale::Th) => "ขอบเขตโฮลดิ้ง",
        ("company", Locale::En) => "Company scope",
        ("company", Locale::Th) => "ขอบเขตบริษัท",
        ("branch", Locale::En) => "Branch scope",
        ("branch", Locale::Th) => "ขอบเขตสาขา",
        (_, Locale::En) => "Authorized scope",
        (_, Locale::Th) => "ขอบเขตที่ได้รับอนุญาต",
    }
}

#[component]
fn AccountRoleCard(
    option: RoleOptionView,
    locale: Locale,
    pending: bool,
    active: bool,
    busy: bool,
    on_select: EventHandler<String>,
) -> Element {
    let selection_id = option.selection_id.clone();
    let role_label = localized_role_label(&option.role_key, locale);
    let scope_label = role_scope_label(&option.scope_key, locale);
    let source_label = match option.source.as_str() {
        "directory" => "Directory",
        _ => "Local",
    };
    rsx! {
        KitButton {
            variant: ButtonVariant::Secondary,
            class: if pending {
                "work-role-card selected"
            } else if active {
                "work-role-card current"
            } else {
                "work-role-card"
            },
            r#type: "button",
            disabled: busy || active,
            aria_label: if active {
                format!("Current role: {role_label} — {scope_label}")
            } else {
                format!("Select {role_label} — {scope_label}")
            },
            "data-testid": "role-option",
            "data-selection-id": "{selection_id}",
            "data-role-key": "{option.role_key}",
            onclick: move |_| on_select.call(selection_id.clone()),
            span { class: "work-role-icon", aria_hidden: "true", "{role_label.chars().next().unwrap_or('R')}" }
            span { class: "work-role-card-copy",
                span { class: "work-role-card-title",
                    strong { "{role_label}" }
                    if active {
                        span { class: "work-role-active", {copy(locale, "Current", "ปัจจุบัน")} }
                    }
                    if option.requires_mfa {
                        span { class: "work-role-mfa", {copy(locale, "MFA required", "ต้องใช้ MFA")} }
                    }
                }
                small { "{scope_label} · {source_label}" }
            }
            span { class: "work-role-check", aria_hidden: "true", if pending { "…" } else if active { "✓" } else { "→" } }
        }
    }
}

#[component]
fn Accounts(return_to: Option<String>) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let mut role_search = use_signal(String::new);
    let normalized_role_search = use_memo(move || role_search.read().trim().to_ascii_lowercase());
    let destination = post_auth_destination(return_to.as_deref());
    let add_account_href = add_account_location(&destination);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &destination;
    // Keep the first client render identical to the server shell. Starting the
    // request in an effect prevents a fast response from changing the VDOM
    // before Dioxus has finished hydrating the SSR route.
    #[allow(unused_mut)]
    let mut page = use_signal(|| None::<Result<AccountsPageData, String>>);
    #[allow(unused_mut)]
    let mut reload = use_signal(|| 0_u64);
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let generation = *reload.read();
        spawn(async move {
            let result = load_accounts_page().await;
            if *reload.read() == generation {
                page.set(Some(result));
            }
        });
    });
    #[allow(unused_mut)]
    let mut pending_selection = use_signal(|| None::<String>);
    #[allow(unused_mut)]
    let mut pending_account = use_signal(|| None::<String>);
    #[allow(unused_mut)]
    let mut confirming_account = use_signal(|| None::<String>);
    #[allow(unused_mut)]
    let mut error = use_signal(|| None::<String>);
    let page_state = page.read().clone();
    let error_message = error.read().clone();
    #[cfg(target_arch = "wasm32")]
    let role_destination = use_signal(|| destination.clone());
    let on_select = move |selection_id: String| {
        #[cfg(target_arch = "wasm32")]
        {
            let destination = role_destination.read().clone();
            pending_selection.set(Some(selection_id.clone()));
            error.set(None);
            spawn(async move {
                if let Err(message) = activate_role_and_continue(selection_id, destination).await {
                    pending_selection.set(None);
                    error.set(Some(message));
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = selection_id;
    };
    let on_activate_account = move |session_id: String| {
        #[cfg(target_arch = "wasm32")]
        {
            pending_account.set(Some(session_id.clone()));
            error.set(None);
            spawn(async move {
                match activate_session_account(session_id).await {
                    Ok(()) => {
                        page.set(None);
                        let next_reload = *reload.read() + 1;
                        reload.set(next_reload);
                    }
                    Err(message) => error.set(Some(message)),
                }
                pending_account.set(None);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = session_id;
    };
    let on_remove_account = move |session_id: String| {
        #[cfg(target_arch = "wasm32")]
        {
            pending_account.set(Some(session_id.clone()));
            error.set(None);
            spawn(async move {
                match remove_session_account(session_id).await {
                    Ok(true) => {
                        confirming_account.set(None);
                        page.set(None);
                        let next_reload = *reload.read() + 1;
                        reload.set(next_reload);
                    }
                    Ok(false) => {
                        confirming_account.set(None);
                        let _ = crate::platform::browser_http::replace_location("/auth/sign-in");
                    }
                    Err(message) => error.set(Some(message)),
                }
                pending_account.set(None);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = session_id;
    };

    rsx! {
        document::Title { "Accounts · ERPX" }
        article { class: "auth-page-form role-selection-dialog",
            p { class: "eyebrow", "SESSION ACCOUNT" }
            h2 { {copy(locale, "Choose an account and role", "เลือกบัญชีและบทบาท")} }
            p { class: "auth-page-form__intro",
                {copy(locale,
                    "Switch accounts or select the authorized role and scope for this workspace.",
                    "สลับบัญชีหรือเลือกบทบาทและขอบเขตที่ได้รับอนุญาตสำหรับพื้นที่ทำงานนี้",
                )}
            }
            match page_state {
                None => rsx! {
                    section { class: "route-loading", aria_busy: "true", "Loading authorized roles…" }
                },
                Some(Err(message)) => rsx! {
                    section { class: "route-denied", role: "alert",
                        h3 { "Accounts unavailable" }
                        p { "{message}" }
                        KitButton {
                            variant: ButtonVariant::Secondary,
                            r#type: "button",
                            onclick: move |_| {
                                let next_reload = *reload.read() + 1;
                                reload.set(next_reload);
                            },
                            "Try again"
                        }
                    }
                },
                Some(Ok(data)) => {
                    let role_query = normalized_role_search.read().clone();
                    let active_role = data
                        .accounts
                        .iter()
                        .find(|account| account.active)
                        .and_then(|account| account.selected_role.clone());
                    let visible_roles = data
                        .roles
                        .iter()
                        .filter(|option| {
                            role_query.is_empty()
                                || localized_role_label(&option.role_key, locale)
                                    .to_ascii_lowercase()
                                    .contains(role_query.as_str())
                                || role_scope_label(&option.scope_key, locale)
                                    .to_ascii_lowercase()
                                    .contains(role_query.as_str())
                                || option.role_key.to_ascii_lowercase().contains(role_query.as_str())
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    let role_result_count = match locale {
                        Locale::En => format!(
                            "{} of {} authorized roles",
                            visible_roles.len(),
                            data.roles.len()
                        ),
                        Locale::Th => format!(
                            "{} จาก {} บทบาทที่ได้รับอนุญาต",
                            visible_roles.len(),
                            data.roles.len()
                        ),
                    };
                    rsx! {
                    section { class: "account-quick-list", aria_label: "Signed-in accounts",
                        div { class: "account-quick-section-heading",
                            span { {copy(locale, "Accounts on this device", "บัญชีบนอุปกรณ์นี้")} }
                            small { "{data.accounts.len()}" }
                        }
                        for account in data.accounts.iter() {
                            div { class: "account-quick-row", "data-state": if account.active { "selected" } else { "" },
                                span { class: "account-quick-avatar", aria_hidden: "true",
                                    "{account.principal_ref.chars().next().unwrap_or('A').to_ascii_uppercase()}"
                                }
                                span { class: "account-quick-copy",
                                    strong { "{account.principal_ref}" }
                                    small {
                                        if let Some(role) = &account.selected_role {
                                            "{localized_role_label(&role.role_key, locale)} · {role_scope_label(&role.scope_key, locale)}"
                                        } else {
                                            "Role selection required"
                                        }
                                    }
                                }
                                if account.active {
                                    span { class: "account-session-state",
                                        span { class: "account-session-dot", aria_hidden: "true" }
                                        {copy(locale, "Current", "ปัจจุบัน")}
                                    }
                                } else {
                                    div { class: "account-row-actions",
                                        KitButton {
                                            variant: ButtonVariant::Secondary,
                                            r#type: "button",
                                            disabled: pending_account.read().is_some() || pending_selection.read().is_some(),
                                            aria_label: "Switch to {account.principal_ref}",
                                            "data-testid": "account-switch",
                                            onclick: {
                                                let session_id = account.session_id.clone();
                                                #[allow(unused_mut)]
                                                let mut activate = on_activate_account;
                                                move |_| activate(session_id.clone())
                                            },
                                            {copy(locale, "Switch", "สลับ")}
                                        }
                                    }
                                }
                                if confirming_account.read().as_deref() == Some(account.session_id.as_str()) {
                                    div { class: "account-row-actions account-remove-confirmation", role: "group", aria_label: copy(locale, "Confirm account removal", "ยืนยันการนำบัญชีออก"),
                                        KitButton {
                                            variant: ButtonVariant::Secondary,
                                            r#type: "button",
                                            disabled: pending_account.read().is_some(),
                                            onclick: move |_| confirming_account.set(None),
                                            {copy(locale, "Cancel", "ยกเลิก")}
                                        }
                                        KitButton {
                                            variant: ButtonVariant::Secondary,
                                            class: "account-remove-button",
                                            r#type: "button",
                                            disabled: pending_account.read().is_some() || pending_selection.read().is_some(),
                                            aria_label: if account.active {
                                                format!("Confirm sign out {}", account.principal_ref)
                                            } else {
                                                format!("Confirm remove {}", account.principal_ref)
                                            },
                                            "data-testid": "account-remove-confirm",
                                            onclick: {
                                                let session_id = account.session_id.clone();
                                                #[allow(unused_mut)]
                                                let mut remove = on_remove_account;
                                                move |_| remove(session_id.clone())
                                            },
                                            if pending_account.read().as_deref() == Some(account.session_id.as_str()) {
                                                {copy(locale, "Working…", "กำลังดำเนินการ…")}
                                            } else {
                                                {copy(locale, "Confirm", "ยืนยัน")}
                                            }
                                        }
                                    }
                                } else {
                                    KitButton {
                                        variant: ButtonVariant::Secondary,
                                        class: "account-remove-button",
                                        r#type: "button",
                                        disabled: pending_account.read().is_some() || pending_selection.read().is_some(),
                                        aria_label: if account.active {
                                            format!("Sign out {}", account.principal_ref)
                                        } else {
                                            format!("Remove {}", account.principal_ref)
                                        },
                                        "data-testid": "account-remove",
                                        onclick: {
                                            let session_id = account.session_id.clone();
                                            move |_| confirming_account.set(Some(session_id.clone()))
                                        },
                                        if account.active {
                                            {copy(locale, "Sign out", "ออกจากระบบ")}
                                        } else {
                                            {copy(locale, "Remove", "นำออก")}
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "account-page-actions",
                        KitLink {
                            href: "{add_account_href}",
                            class: "auth-secondary-link",
                            "data-testid": "add-account",
                            {copy(locale, "Add another account", "เพิ่มบัญชีอื่น")}
                        }
                        if data.accounts.iter().any(|account| account.active && account.selected_role.is_some()) {
                            KitLink {
                                href: "{destination}",
                                class: "auth-secondary-link",
                                {copy(locale, "Continue with current role", "ดำเนินการต่อด้วยบทบาทปัจจุบัน")}
                            }
                        }
                    }
                    if let Some(message) = error_message {
                        p { class: "route-error", role: "alert", "{message}" }
                    }
                    section { class: "work-role-picker",
                        div { class: "work-role-search",
                            label { class: "work-role-search__field",
                                span { {copy(locale, "Find a role", "ค้นหาบทบาท")} }
                                KitInput {
                                    value: "{role_search}",
                                    placeholder: copy(locale, "Search by role or scope", "ค้นหาจากบทบาทหรือขอบเขต"),
                                    oninput: move |event: FormEvent| role_search.set(event.value()),
                                }
                            }
                            span { class: "work-role-result-count", "{role_result_count}" }
                        }
                        div { class: "work-role-groups",
                            div { class: "work-role-group",
                                div { class: "work-role-group-heading",
                                    h3 { {copy(locale, "Available roles", "บทบาทที่พร้อมใช้")} }
                                    span { "{visible_roles.len()}" }
                                }
                                if visible_roles.is_empty() {
                                    div { class: "work-role-empty", role: "status",
                                        span { aria_hidden: "true", "⌕" }
                                        strong { {copy(locale, "No matching roles", "ไม่พบบทบาทที่ตรงกัน")} }
                                        p { {copy(locale, "Try another name or clear the search.", "ลองใช้ชื่ออื่นหรือล้างคำค้น")} }
                                        KitButton {
                                            variant: ButtonVariant::Secondary,
                                            r#type: "button",
                                            onclick: move |_| role_search.set(String::new()),
                                            {copy(locale, "Clear search", "ล้างคำค้น")}
                                        }
                                    }
                                } else {
                                    div { class: "work-role-grid",
                                        for option in visible_roles.iter() {
                                            AccountRoleCard {
                                                key: "{option.selection_id}",
                                                option: option.clone(),
                                                locale,
                                                pending: pending_selection.read().as_deref() == Some(option.selection_id.as_str()),
                                                active: active_role.as_ref().is_some_and(|role| {
                                                    role.role_key == option.role_key && role.scope_key == option.scope_key
                                                }),
                                                busy: pending_selection.read().is_some() || pending_account.read().is_some(),
                                                on_select,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            }
            p { class: "auth-page-form__security",
                {copy(locale,
                    "Account and role changes update the protected session before the workspace opens.",
                    "การเปลี่ยนบัญชีและบทบาทจะอัปเดตเซสชันที่ได้รับการปกป้องก่อนเปิดพื้นที่ทำงาน",
                )}
            }
        }
    }
}

macro_rules! auth_page {
    ($name:ident, $title_en:literal, $title_th:literal, $description_en:literal, $description_th:literal, $icon:literal) => {
        #[component]
        fn $name() -> Element {
            let locale = erpx_ui_core::use_locale().current();
            let page_title = copy(locale, $title_en, $title_th);
            let title = format!("{page_title} · ERPX");
            rsx! {
                document::Title { "{title}" }
                article { class: "auth-page-form auth-utility-page",
                    p { class: "eyebrow", {copy(locale, "SECURITY CENTER", "ศูนย์ความปลอดภัย")} }
                    div { class: "auth-utility-heading",
                        span { aria_hidden: "true", $icon }
                        h2 { "{page_title}" }
                    }
                    p { class: "auth-page-form__intro", {copy(locale, $description_en, $description_th)} }
                    section { class: "auth-utility-status", role: "status",
                        span { class: "auth-utility-status__dot", aria_hidden: "true" }
                        div {
                            strong { {copy(locale, "Protected session", "เซสชันที่ได้รับการปกป้อง")} }
                            p { {copy(locale,
                                "This surface is session-scoped and is never stored in shared caches.",
                                "หน้านี้ผูกกับเซสชันและจะไม่ถูกเก็บไว้ในแคชที่ใช้ร่วมกัน",
                            )} }
                        }
                    }
                    div { class: "route-actions auth-utility-actions",
                        KitLink { class: "route-action route-action--primary", href: "/auth/accounts", {copy(locale, "Manage account access", "จัดการสิทธิ์บัญชี")} }
                        KitLink { class: "route-action", href: "/work", {copy(locale, "Return to work", "กลับไปยังงาน")} }
                    }
                }
            }
        }
    };
}

auth_page!(
    Security,
    "Security",
    "ความปลอดภัย",
    "Review the trusted session boundary and continue to account access when you need to change roles.",
    "ตรวจสอบขอบเขตเซสชันที่เชื่อถือได้ และไปยังสิทธิ์บัญชีเมื่อต้องการเปลี่ยนบทบาท",
    "⌾"
);

#[derive(Clone, PartialEq)]
struct DeviceConfirmation {
    device_id: uuid::Uuid,
    label: String,
    idempotency_key: String,
}

#[derive(Clone, PartialEq)]
struct DeviceFeedback {
    state: &'static str,
    message: String,
    audit_reference: Option<String>,
}

#[cfg(any(feature = "web", feature = "server"))]
fn device_error_copy(
    error: &erpx_delivery_contract::DeliveryError,
    locale: Locale,
) -> (&'static str, &'static str) {
    use erpx_delivery_contract::DeliveryError;
    match (error, locale) {
        (DeliveryError::Unauthenticated(_), Locale::En) => (
            "Session unavailable",
            "Reason: the authenticated web session is no longer valid. Sign in again.",
        ),
        (DeliveryError::Unauthenticated(_), Locale::Th) => (
            "เซสชันไม่พร้อมใช้งาน",
            "เหตุผล: เซสชันเว็บที่ยืนยันตัวตนแล้วไม่ถูกต้องอีกต่อไป โปรดเข้าสู่ระบบใหม่",
        ),
        (DeliveryError::Forbidden(_), Locale::En) => (
            "Access denied",
            "Reason: the active account is not authorized to review these devices.",
        ),
        (DeliveryError::Forbidden(_), Locale::Th) => (
            "ไม่มีสิทธิ์เข้าถึง",
            "เหตุผล: บัญชีที่ใช้งานอยู่ไม่ได้รับอนุญาตให้ตรวจสอบอุปกรณ์เหล่านี้",
        ),
        (DeliveryError::Conflict(_), Locale::En) => (
            "Device changed",
            "Reason: the current device must be signed out through account management.",
        ),
        (DeliveryError::Conflict(_), Locale::Th) => (
            "สถานะอุปกรณ์เปลี่ยนแล้ว",
            "เหตุผล: ต้องออกจากระบบอุปกรณ์ปัจจุบันผ่านหน้าจัดการบัญชี",
        ),
        (DeliveryError::Unavailable(_), Locale::En) => (
            "Device service unavailable",
            "Reason: the identity owner or its device projection could not be reached.",
        ),
        (DeliveryError::Unavailable(_), Locale::Th) => (
            "บริการอุปกรณ์ไม่พร้อมใช้งาน",
            "เหตุผล: ไม่สามารถเชื่อมต่อบริการเจ้าของข้อมูลตัวตนหรือ projection ของอุปกรณ์ได้",
        ),
        (_, Locale::En) => (
            "Device request failed",
            "Reason: the identity owner did not accept or could not complete the request.",
        ),
        (_, Locale::Th) => (
            "คำขออุปกรณ์ไม่สำเร็จ",
            "เหตุผล: บริการเจ้าของข้อมูลตัวตนไม่ยอมรับหรือไม่สามารถทำคำขอให้เสร็จได้",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn Devices(cursor: Option<String>) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = crate::security_delivery::DeviceListRequest {
        cursor,
        page_size: 25,
        locale: locale.language_tag().into(),
    };
    let mut resource = use_server_future(move || {
        let request = request.clone();
        async move {
            match crate::security_delivery::list_security_devices(request).await {
                Ok(result) => result,
                Err(_) => Err(erpx_delivery_contract::DeliveryError::unavailable(Some(
                    500,
                ))),
            }
        }
    })?;
    let page_state = resource
        .read()
        .clone()
        .expect("resolved device future has a value");
    let mut confirmation = use_signal(|| None::<DeviceConfirmation>);
    let mut command_busy = use_signal(|| false);
    let mut feedback = use_signal(|| None::<DeviceFeedback>);
    let page_title = format!("{} · ERPX", copy(locale, "Devices", "อุปกรณ์"));

    let confirm_revoke = move |_| {
        let Some(selected) = confirmation.read().clone() else {
            return;
        };
        if *command_busy.read() {
            return;
        }
        command_busy.set(true);
        feedback.set(Some(DeviceFeedback {
            state: "pending",
            message: copy(
                locale,
                "Revoking every session on this device and refetching the authoritative state…",
                "กำลังเพิกถอนทุกเซสชันบนอุปกรณ์นี้และโหลดสถานะที่เชื่อถือได้ใหม่…",
            )
            .into(),
            audit_reference: None,
        }));
        spawn(async move {
            let result = crate::security_delivery::revoke_security_device(
                crate::security_delivery::RevokeDeviceRequest {
                    device_id: selected.device_id,
                    idempotency_key: selected.idempotency_key,
                    locale: locale.language_tag().into(),
                    purpose: format!("revoke unrecognized device: {}", selected.label),
                    confirmed: true,
                },
            )
            .await;
            command_busy.set(false);
            match result {
                Ok(Ok(outcome)) => {
                    let confirmed = outcome.authoritative_state == "confirmed";
                    feedback.set(Some(DeviceFeedback {
                        state: if confirmed { "success" } else { "pending" },
                        message: match (locale, confirmed, outcome.receipt.replayed) {
                            (Locale::En, true, false) => "Device sessions were revoked and confirmed by the identity owner.",
                            (Locale::En, true, true) => "The earlier revocation was replayed safely and confirmed by the identity owner.",
                            (Locale::En, false, _) => "The revocation was accepted; authoritative confirmation is still pending.",
                            (Locale::Th, true, false) => "เพิกถอนเซสชันบนอุปกรณ์แล้ว และยืนยันจากบริการเจ้าของข้อมูลตัวตนแล้ว",
                            (Locale::Th, true, true) => "ระบบ replay การเพิกถอนเดิมอย่างปลอดภัย และยืนยันจากบริการเจ้าของข้อมูลตัวตนแล้ว",
                            (Locale::Th, false, _) => "ระบบรับคำสั่งเพิกถอนแล้ว แต่ยังรอการยืนยันสถานะที่เชื่อถือได้",
                        }
                        .into(),
                        audit_reference: Some(outcome.receipt.audit_reference.to_string()),
                    }));
                    confirmation.set(None);
                    resource.restart();
                }
                Ok(Err(error)) => {
                    let (title, reason) = device_error_copy(&error, locale);
                    feedback.set(Some(DeviceFeedback {
                        state: "error",
                        message: format!(
                            "{title}. {reason} {}: {}",
                            copy(locale, "Reference", "รหัสอ้างอิง"),
                            error.meta().correlation_id
                        ),
                        audit_reference: None,
                    }));
                    if matches!(error, erpx_delivery_contract::DeliveryError::Conflict(_)) {
                        resource.restart();
                    }
                }
                Err(_) => feedback.set(Some(DeviceFeedback {
                    state: "error",
                    message: copy(
                        locale,
                        "The same-origin device command transport is unavailable.",
                        "ช่องทางคำสั่งอุปกรณ์แบบ same-origin ไม่พร้อมใช้งาน",
                    )
                    .into(),
                    audit_reference: None,
                })),
            }
        });
    };

    rsx! {
        document::Title { "{page_title}" }
        article { class: "auth-page-form auth-utility-page device-management-page",
            p { class: "eyebrow", {copy(locale, "SECURITY CENTER", "ศูนย์ความปลอดภัย")} }
            div { class: "auth-utility-heading",
                span { aria_hidden: "true", "▣" }
                div {
                    h2 { {copy(locale, "Devices and sessions", "อุปกรณ์และเซสชัน")} }
                    p { class: "auth-page-form__intro",
                        {copy(locale,
                            "Review devices from the identity owner. Revoking a device ends every session attached to it.",
                            "ตรวจสอบอุปกรณ์จากบริการเจ้าของข้อมูลตัวตน การเพิกถอนอุปกรณ์จะสิ้นสุดทุกเซสชันที่ผูกอยู่",
                        )}
                    }
                }
            }
            match page_state {
                Err(error) => {
                    let (error_title, reason) = device_error_copy(&error, locale);
                    rsx! {
                        section { class: "route-denied", role: "alert", "data-reason": "identity_device_query_failed",
                            h3 { "{error_title}" }
                            p { "{reason}" }
                            small {
                                {copy(locale, "Reference", "รหัสอ้างอิง")}
                                ": {error.meta().correlation_id}"
                            }
                            div { class: "route-actions",
                                KitButton {
                                    variant: ButtonVariant::Secondary,
                                    r#type: "button",
                                    onclick: move |_| resource.restart(),
                                    {copy(locale, "Try again", "ลองอีกครั้ง")}
                                }
                                KitLink { href: "/auth/accounts", {copy(locale, "Manage accounts", "จัดการบัญชี")} }
                            }
                        }
                    }
                },
                Ok(page) => rsx! {
                    section { class: "auth-utility-status", role: "status",
                        span { class: "auth-utility-status__dot", aria_hidden: "true" }
                        div {
                            strong { {copy(locale, "Authoritative identity data", "ข้อมูลที่เชื่อถือได้จากบริการตัวตน")} }
                            p {
                                {copy(locale, "Measured at", "วัดเมื่อ")}
                                " {page.measured_at} · "
                                {copy(locale, "Freshness", "ความสดใหม่")}
                                ": {page.freshness}"
                            }
                            small {
                                {copy(locale, "Reference", "รหัสอ้างอิง")}
                                ": {page.correlation_id}"
                            }
                        }
                    }
                    if page.items.is_empty() {
                        section { class: "device-authoritative-empty", role: "status",
                            h3 { {copy(locale, "No device records", "ไม่มีรายการอุปกรณ์")} }
                            p { {copy(locale,
                                "The identity owner returned an authoritative empty result; no sample devices were created.",
                                "บริการเจ้าของข้อมูลตัวตนส่งผลลัพธ์ว่างที่เชื่อถือได้ ระบบไม่ได้สร้างอุปกรณ์ตัวอย่าง",
                            )} }
                        }
                    } else {
                        section { class: "security-device-list", aria_label: copy(locale, "Registered devices", "อุปกรณ์ที่ลงทะเบียน"),
                            for device in page.items.iter() {
                                article { class: "security-device-card", "data-current": device.current, "data-revoked": device.revoked_at.is_some(),
                                    div { class: "security-device-card__icon", aria_hidden: "true", if device.current { "●" } else { "▣" } }
                                    div { class: "security-device-card__copy",
                                        div { class: "security-device-card__title",
                                            h3 { "{device.label}" }
                                            if device.current {
                                                span { class: "security-device-state current", {copy(locale, "Current", "ปัจจุบัน")} }
                                            } else if device.revoked_at.is_some() {
                                                span { class: "security-device-state revoked", {copy(locale, "Revoked", "เพิกถอนแล้ว")} }
                                            } else {
                                                span { class: "security-device-state", {copy(locale, "Registered", "ลงทะเบียนแล้ว")} }
                                            }
                                        }
                                        p {
                                            {copy(locale, "Last seen", "ใช้งานล่าสุด")}
                                            ": {device.last_seen_at}"
                                        }
                                        small {
                                            {copy(locale, "Registered", "ลงทะเบียนเมื่อ")}
                                            ": {device.created_at}"
                                        }
                                        if let Some(revoked_at) = device.revoked_at.as_ref() {
                                            small {
                                                {copy(locale, "Revoked at", "เพิกถอนเมื่อ")}
                                                ": {revoked_at}"
                                            }
                                        }
                                    }
                                    div { class: "security-device-card__actions",
                                        if device.current {
                                            KitLink { href: "/auth/accounts", class: "route-action", {copy(locale, "Sign out safely", "ออกจากระบบอย่างปลอดภัย")} }
                                        } else if device.revoked_at.is_none() {
                                            KitButton {
                                                variant: ButtonVariant::Secondary,
                                                r#type: "button",
                                                disabled: *command_busy.read(),
                                                "data-testid": "device-revoke",
                                                onclick: {
                                                    let device_id = device.id;
                                                    let label = device.label.clone();
                                                    move |_| confirmation.set(Some(DeviceConfirmation {
                                                        device_id,
                                                        label: label.clone(),
                                                        idempotency_key: uuid::Uuid::new_v4().to_string(),
                                                    }))
                                                },
                                                {copy(locale, "Revoke", "เพิกถอน")}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(next_cursor) = page.next_cursor.as_ref() {
                        div { class: "route-actions auth-utility-actions",
                            KitLink { href: "/auth/devices?cursor={next_cursor}", {copy(locale, "Next page", "หน้าถัดไป")} }
                            KitLink { href: "/auth/devices", {copy(locale, "First page", "หน้าแรก")} }
                        }
                    }
                }
            }
            if let Some(selected) = confirmation.read().as_ref() {
                section { class: "device-revoke-confirmation", role: "alertdialog", aria_modal: "true", aria_labelledby: "device-revoke-title", aria_describedby: "device-revoke-description",
                    strong { id: "device-revoke-title", {copy(locale, "Revoke this device?", "เพิกถอนอุปกรณ์นี้หรือไม่")} }
                    p { "{selected.label}" }
                    p { id: "device-revoke-description", {copy(locale,
                        "Every session on this device will end. This does not delete ERPX business data.",
                        "ทุกเซสชันบนอุปกรณ์นี้จะสิ้นสุด การดำเนินการนี้ไม่ลบข้อมูลธุรกิจใน ERPX",
                    )} }
                    div { class: "route-actions",
                        KitButton {
                            variant: ButtonVariant::Secondary,
                            r#type: "button",
                            disabled: *command_busy.read(),
                            onclick: move |_| confirmation.set(None),
                            {copy(locale, "Cancel", "ยกเลิก")}
                        }
                        KitButton {
                            variant: ButtonVariant::Secondary,
                            class: "account-remove-button",
                            r#type: "button",
                            disabled: *command_busy.read(),
                            "data-testid": "device-revoke-confirm",
                            onclick: confirm_revoke,
                            if *command_busy.read() {
                                {copy(locale, "Revoking…", "กำลังเพิกถอน…")}
                            } else {
                                {copy(locale, "Confirm revoke", "ยืนยันการเพิกถอน")}
                            }
                        }
                    }
                }
            }
            if let Some(status) = feedback.read().as_ref() {
                section { class: "identity-command-feedback", "data-state": status.state, role: "status",
                    p { "{status.message}" }
                    if let Some(reference) = status.audit_reference.as_ref() {
                        small {
                            {copy(locale, "Audit reference", "รหัสอ้างอิงการตรวจสอบ")}
                            ": {reference}"
                        }
                    }
                }
            }
            div { class: "route-actions auth-utility-actions",
                KitLink { href: "/auth/security", {copy(locale, "Security settings", "การตั้งค่าความปลอดภัย")} }
                KitLink { href: "/work", {copy(locale, "Return to work", "กลับไปยังงาน")} }
            }
            p { class: "auth-page-form__security",
                {copy(locale,
                    "Session identifiers and credential material are redacted by contract and never reach this page.",
                    "รหัสเซสชันและข้อมูลรับรองถูกปกปิดตาม contract และจะไม่ถูกส่งมายังหน้านี้",
                )}
            }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn Devices(_cursor: Option<String>) -> Element {
    rsx! { FeatureClosed { name: "Device management" } }
}

auth_page!(
    Recovery,
    "Recovery",
    "การกู้คืน",
    "Recovery controls remain inside the protected account boundary so workspace data is never exposed here.",
    "การควบคุมการกู้คืนอยู่ภายในขอบเขตบัญชีที่ได้รับการปกป้อง จึงไม่เปิดเผยข้อมูลพื้นที่ทำงานในหน้านี้",
    "↻"
);

#[derive(Clone, PartialEq)]
enum PublicFormSubmissionState {
    Idle,
    Submitting,
    Succeeded(String),
    Failed(String),
}

#[component]
fn PublicMarketingForm(id: RouteUuid) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let mut name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut consent = use_signal(|| false);
    let idempotency_key = use_signal(|| uuid::Uuid::new_v4().hyphenated().to_string());
    let mut submission = use_signal(|| PublicFormSubmissionState::Idle);
    let submission_state = submission.read().clone();
    let submitting = matches!(submission_state, PublicFormSubmissionState::Submitting);
    rsx! {
        document::Title { "Marketing form · ERPX" }
        document::Meta { name: "description", content: "Consent-aware ERPX marketing form" }
        article { class: "public-marketing-card",
            p { class: "eyebrow", {copy(locale, "CONTACT ERPX", "ติดต่อ ERPX")} }
            h1 { {copy(locale, "Marketing form", "แบบฟอร์มการตลาด")} }
            p { class: "public-marketing-description",
                {copy(locale,
                    "Share your details and message. Required fields are clearly marked before submission.",
                    "แจ้งรายละเอียดและข้อความของคุณ โดยช่องที่จำเป็นจะแสดงไว้อย่างชัดเจนก่อนส่ง",
                )}
            }
            form {
                class: "public-marketing-form",
                aria_label: copy(locale, "Marketing contact form", "แบบฟอร์มติดต่อฝ่ายการตลาด"),
                method: "post",
                onsubmit: move |event: FormEvent| {
                    event.prevent_default();
                    if name.read().trim().is_empty()
                        || email.read().trim().is_empty()
                        || message.read().trim().is_empty()
                        || !*consent.read()
                    {
                        submission.set(PublicFormSubmissionState::Failed(
                            copy(locale, "Complete every required field and consent before submitting.", "กรอกข้อมูลที่จำเป็นและให้ความยินยอมก่อนส่ง").to_owned(),
                        ));
                        return;
                    }
                    let request = erpx_delivery_contract::PublicMarketingFormRequest {
                        form_id: id,
                        name: name.read().trim().to_owned(),
                        email: email.read().trim().to_owned(),
                        message: message.read().trim().to_owned(),
                        consent: *consent.read(),
                        idempotency_key: idempotency_key.read().clone(),
                        locale: locale.language_tag().to_owned(),
                        purpose: "marketing_contact".to_owned(),
                    };
                    submission.set(PublicFormSubmissionState::Submitting);
                    spawn(async move {
                        match crate::delivery::submit_public_marketing_form(request).await {
                            Ok(Ok(receipt)) => submission.set(PublicFormSubmissionState::Succeeded(
                                receipt.audit_reference,
                            )),
                            Ok(Err(error)) => submission.set(PublicFormSubmissionState::Failed(
                                error.to_string(),
                            )),
                            Err(_) => submission.set(PublicFormSubmissionState::Failed(
                                copy(locale, "The submission service is unavailable. Retry safely with the same request.", "บริการรับข้อมูลไม่พร้อมใช้งาน ลองส่งคำขอเดิมอีกครั้งได้อย่างปลอดภัย").to_owned(),
                            )),
                        }
                    });
                },
                label {
                    span { {copy(locale, "Name", "ชื่อ")} }
                    KitInput {
                        name: "name",
                        autocomplete: "name",
                        required: true,
                        maxlength: "120",
                        value: "{name}",
                        disabled: submitting,
                        oninput: move |event: FormEvent| name.set(event.value()),
                    }
                }
                label {
                    span { {copy(locale, "Email", "อีเมล")} }
                    KitInput {
                        name: "email",
                        r#type: "email",
                        autocomplete: "email",
                        required: true,
                        maxlength: "254",
                        value: "{email}",
                        disabled: submitting,
                        oninput: move |event: FormEvent| email.set(event.value()),
                    }
                }
                label {
                    span { {copy(locale, "Message", "ข้อความ")} }
                    KitTextArea {
                        value: message(),
                        name: "message".to_owned(),
                        required: true,
                        maxlength: 4000,
                        disabled: submitting,
                        aria_label: copy(locale, "Message", "ข้อความ").to_owned(),
                        on_value_change: move |value| message.set(value),
                    }
                }
                label { class: "public-consent",
                    KitInput {
                        name: "consent",
                        r#type: "checkbox",
                        required: true,
                        checked: consent(),
                        disabled: submitting,
                        onchange: move |event: FormEvent| consent.set(event.checked()),
                    }
                    span { {copy(locale, "I consent to this submission", "ฉันยินยอมให้ส่งข้อมูลนี้")} }
                }
                KitButton {
                    variant: ButtonVariant::Primary,
                    r#type: "submit",
                    disabled: submitting,
                    if submitting {
                        {copy(locale, "Submitting…", "กำลังส่ง…")}
                    } else {
                        {copy(locale, "Submit", "ส่งข้อมูล")}
                    }
                }
                match submission_state {
                    PublicFormSubmissionState::Succeeded(reference) => rsx! {
                        div { class: "public-form-status public-form-status--success", role: "status",
                            strong { {copy(locale, "Submission received", "รับข้อมูลเรียบร้อยแล้ว")} }
                            small { "{reference}" }
                        }
                    },
                    PublicFormSubmissionState::Failed(message) => rsx! {
                        div { class: "public-form-status public-form-status--error", role: "alert",
                            strong { {copy(locale, "Submission not completed", "ยังส่งข้อมูลไม่สำเร็จ")} }
                            small { "{message}" }
                        }
                    },
                    PublicFormSubmissionState::Idle | PublicFormSubmissionState::Submitting => rsx! {},
                }
            }
            small { class: "public-marketing-form-id", "Form {id}" }
        }
    }
}

#[component]
fn PublicMarketingPreview(id: RouteUuid, token: Option<String>) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = PublicMarketingPreviewRequest {
        preview_id: id,
        access_code: token.unwrap_or_default(),
        locale: locale.language_tag().to_owned(),
    };
    #[cfg(any(feature = "web", feature = "server"))]
    let data = {
        let loader =
            use_loader(move || crate::delivery::public_marketing_preview(request.clone()))?;
        loader.read().clone().map_err(RenderError::from)?
    };
    #[cfg(feature = "native")]
    let data = {
        let gateway = use_context::<crate::native_delivery::NativeGrpcGateway>();
        let preview = use_resource(move || {
            let gateway = gateway.clone();
            let request = request.clone();
            async move { gateway.public_marketing_preview(request).await }
        });
        preview
            .suspend()?
            .read()
            .clone()
            .map_err(render_delivery_error)?
    };
    rsx! {
        document::Title { "{data.title} · ERPX" }
        document::Meta { name: "description", content: "{data.description}" }
        article { class: "public-marketing-card public-preview",
            p { class: "eyebrow", {copy(locale, "PREVIEW", "ตัวอย่างเผยแพร่")} }
            h1 { "{data.title}" }
            p { class: "public-preview-body", "{data.description}" }
            if !data.page.items.is_empty() {
                section { class: "public-preview-highlights", aria_label: copy(locale, "Preview highlights", "ไฮไลต์ของตัวอย่าง"),
                    for item in &data.page.items {
                        article {
                            strong { "{item.title}" }
                            small { "{item.status}" }
                        }
                    }
                }
            }
            div { class: "route-actions",
                KitLink { class: "route-action", href: "/auth/sign-in", {copy(locale, "Return to ERPX", "กลับไปยัง ERPX")} }
            }
        }
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    #[cfg(feature = "server")]
    dioxus::fullstack::FullstackContext::commit_http_status(
        dioxus::prelude::StatusCode::NOT_FOUND,
        None,
    );
    let locale = erpx_ui_core::use_locale().current();
    let attempted = format!("/{}", segments.join("/"));
    let explanation = match locale {
        Locale::En => format!(
            "We could not find “{attempted}”. It may have moved, or your current role may not expose it."
        ),
        Locale::Th => {
            format!("เราไม่พบ “{attempted}” หน้านี้อาจถูกย้าย หรือบทบาทปัจจุบันของคุณอาจไม่เปิดให้เข้าถึง")
        }
    };
    let document_title = format!("{} · ERPX", copy(locale, "Not found", "ไม่พบหน้า"));
    rsx! {
        document::Title { "{document_title}" }
        main { class: "route-not-found",
            span { class: "route-not-found__code", aria_hidden: "true", "404" }
            p { class: "eyebrow", "ERPX" }
            h1 { {copy(locale, "Route not found", "ไม่พบหน้าที่ต้องการ")} }
            p { "{explanation}" }
            div { class: "route-actions",
                KitLink { class: "route-action route-action--primary", href: "/work", {copy(locale, "Return to work", "กลับไปยังงาน")} }
                KitLink { class: "route-action", href: "/modules", {copy(locale, "Browse modules", "ดูโมดูล")} }
            }
        }
    }
}

fn workspace_allowed(workspace: Workspace) -> bool {
    let session = use_context::<SessionBootstrap>();
    workspace.available_on(session.channel)
}

const fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

const fn theme_mode_label(locale: Locale, mode: ThemeMode) -> &'static str {
    match (locale, mode) {
        (Locale::En, ThemeMode::System) => "System",
        (Locale::En, ThemeMode::Light) => "Light",
        (Locale::En, ThemeMode::Dark) => "Dark",
        (Locale::Th, ThemeMode::System) => "ตามระบบ",
        (Locale::Th, ThemeMode::Light) => "สว่าง",
        (Locale::Th, ThemeMode::Dark) => "มืด",
    }
}

const fn nav_link_class(active: bool) -> &'static str {
    if active {
        "console-nav__link is-active"
    } else {
        "console-nav__link"
    }
}

const fn nav_aria_current(active: bool) -> Option<&'static str> {
    if active { Some("page") } else { None }
}

fn route_is_identity(route: &AppRoute) -> bool {
    matches!(
        route,
        AppRoute::IdentityPrincipals { .. }
            | AppRoute::IdentityPrincipal { .. }
            | AppRoute::IdentityRoles { .. }
            | AppRoute::IdentityRole { .. }
            | AppRoute::IdentityGrants { .. }
            | AppRoute::IdentityGrant { .. }
            | AppRoute::IdentityAccessReviews { .. }
            | AppRoute::IdentityAccessReview { .. }
            | AppRoute::IdentityBreakGlassSessions { .. }
            | AppRoute::IdentityBreakGlassSession { .. }
    )
}

fn route_uses_workspace(route: &AppRoute, expected: Workspace) -> bool {
    let path = route.to_string();
    let workspace_path = format!("/{expected}");
    path == workspace_path || path.starts_with(&format!("{workspace_path}/"))
}

const fn workspace_label(workspace: Workspace, locale: Locale) -> &'static str {
    match (workspace, locale) {
        (Workspace::Operations, Locale::En) => "Operations",
        (Workspace::Operations, Locale::Th) => "ปฏิบัติการ",
        (Workspace::Finance, Locale::En) => "Finance",
        (Workspace::Finance, Locale::Th) => "การเงิน",
        (Workspace::Sales, Locale::En) => "Sales",
        (Workspace::Sales, Locale::Th) => "การขาย",
        (Workspace::Procurement, Locale::En) => "Procurement",
        (Workspace::Procurement, Locale::Th) => "จัดซื้อ",
        (Workspace::Inventory, Locale::En) => "Inventory",
        (Workspace::Inventory, Locale::Th) => "สินค้าคงคลัง",
        (Workspace::Asset, Locale::En) => "Assets",
        (Workspace::Asset, Locale::Th) => "สินทรัพย์",
        (Workspace::Maintenance, Locale::En) => "Maintenance",
        (Workspace::Maintenance, Locale::Th) => "ซ่อมบำรุง",
        (Workspace::Quality, Locale::En) => "Quality",
        (Workspace::Quality, Locale::Th) => "คุณภาพ",
        (Workspace::Marketing, Locale::En) => "Marketing",
        (Workspace::Marketing, Locale::Th) => "การตลาด",
        (Workspace::People, Locale::En) => "People",
        (Workspace::People, Locale::Th) => "บุคลากร",
        (Workspace::Pos, Locale::En) => "Point of sale",
        (Workspace::Pos, Locale::Th) => "จุดขาย",
        (Workspace::Legal, Locale::En) => "Legal",
        (Workspace::Legal, Locale::Th) => "กฎหมาย",
        (Workspace::Manufacturing, Locale::En) => "Manufacturing",
        (Workspace::Manufacturing, Locale::Th) => "การผลิต",
        (Workspace::Planning, Locale::En) => "Planning",
        (Workspace::Planning, Locale::Th) => "การวางแผน",
        (Workspace::Project, Locale::En) => "Projects",
        (Workspace::Project, Locale::Th) => "โครงการ",
        (Workspace::Commerce, Locale::En) => "Commerce",
        (Workspace::Commerce, Locale::Th) => "อีคอมเมิร์ซ",
        (Workspace::Talent, Locale::En) => "Talent",
        (Workspace::Talent, Locale::Th) => "สรรหาบุคลากร",
        (Workspace::Learning, Locale::En) => "Learning",
        (Workspace::Learning, Locale::Th) => "การเรียนรู้",
        (Workspace::Benefits, Locale::En) => "Benefits",
        (Workspace::Benefits, Locale::Th) => "สวัสดิการ",
        (Workspace::FieldService, Locale::En) => "Field service",
        (Workspace::FieldService, Locale::Th) => "บริการภาคสนาม",
        (Workspace::Ai, Locale::En) => "AI collaboration",
        (Workspace::Ai, Locale::Th) => "ผู้ช่วย AI",
    }
}

const fn default_workspace_resource(workspace: Workspace) -> &'static str {
    match workspace {
        Workspace::Operations => "exceptions",
        Workspace::Finance => "journal-entries",
        Workspace::Sales => "orders",
        Workspace::Procurement => "purchase-orders",
        Workspace::Inventory => "stock-items",
        Workspace::Asset => "assets",
        Workspace::Maintenance => "work-orders",
        Workspace::Quality => "inspections",
        Workspace::Marketing => "campaigns",
        Workspace::People => "employees",
        Workspace::Pos => "sales",
        Workspace::Legal => "contracts",
        Workspace::Manufacturing => "production-orders",
        Workspace::Planning => "demand-plans",
        Workspace::Project => "projects",
        Workspace::Commerce => "listings",
        Workspace::Talent => "candidates",
        Workspace::Learning => "enrollments",
        Workspace::Benefits => "benefit-plans",
        Workspace::FieldService => "service-orders",
        Workspace::Ai => "conversations",
    }
}

fn humanize_resource(value: &str) -> String {
    value
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + characters.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn localized_resource_label(value: &str, locale: Locale) -> String {
    let label = match (value, locale) {
        ("exceptions", Locale::Th) => "ข้อยกเว้น",
        ("journal-entries", Locale::Th) => "รายการสมุดรายวัน",
        ("orders", Locale::Th) => "คำสั่งซื้อ",
        ("purchase-orders", Locale::Th) => "ใบสั่งซื้อ",
        ("stock-items", Locale::Th) => "รายการสินค้าคงคลัง",
        ("assets", Locale::Th) => "สินทรัพย์",
        ("work-orders", Locale::Th) => "ใบงาน",
        ("inspections", Locale::Th) => "การตรวจสอบ",
        ("campaigns", Locale::Th) => "แคมเปญ",
        ("employees", Locale::Th) => "พนักงาน",
        ("sales", Locale::Th) => "รายการขาย",
        ("contracts", Locale::Th) => "สัญญา",
        ("production-orders", Locale::Th) => "คำสั่งผลิต",
        ("demand-plans", Locale::Th) => "แผนอุปสงค์",
        ("projects", Locale::Th) => "โครงการ",
        ("listings", Locale::Th) => "รายการสินค้าออนไลน์",
        ("candidates", Locale::Th) => "ผู้สมัคร",
        ("enrollments", Locale::Th) => "การลงทะเบียน",
        ("benefit-plans", Locale::Th) => "แผนสวัสดิการ",
        ("service-orders", Locale::Th) => "ใบงานบริการ",
        ("conversations", Locale::Th) => "บทสนทนา",
        _ => return humanize_resource(value),
    };
    label.to_owned()
}

fn route_heading(query: &RouteQuery, fallback: &str, locale: Locale) -> String {
    match query {
        RouteQuery::Work => copy(locale, "Work", "งาน").to_owned(),
        RouteQuery::Processes => copy(locale, "Processes", "กระบวนการ").to_owned(),
        RouteQuery::Insights => copy(locale, "Insights", "ข้อมูลเชิงลึก").to_owned(),
        RouteQuery::Modules => copy(locale, "Modules", "โมดูล").to_owned(),
        RouteQuery::PlatformHoldings => {
            copy(locale, "Platform holdings", "โฮลดิ้งบนแพลตฟอร์ม").to_owned()
        }
        RouteQuery::PlatformHolding { .. } => fallback.to_owned(),
        RouteQuery::Workspace {
            workspace,
            resource: None,
        } => workspace_label(*workspace, locale).to_owned(),
        RouteQuery::Workspace {
            workspace,
            resource: Some(resource),
        } => format!(
            "{} · {}",
            workspace_label(*workspace, locale),
            localized_resource_label(resource.as_str(), locale)
        ),
        RouteQuery::Record { .. } => fallback.to_owned(),
        RouteQuery::Reference { .. } => fallback.to_owned(),
    }
}

fn route_description(query: &RouteQuery, heading: &str, locale: Locale) -> String {
    match (query, locale) {
        (RouteQuery::Work, Locale::En) => {
            "Review the work available to your active account, role, and organization scope."
                .to_owned()
        }
        (RouteQuery::Work, Locale::Th) => {
            "ตรวจสอบงานที่พร้อมใช้สำหรับบัญชี บทบาท และขอบเขตองค์กรที่กำลังใช้งาน".to_owned()
        }
        (RouteQuery::Processes, Locale::En) => {
            "Follow the business processes visible to your current access scope.".to_owned()
        }
        (RouteQuery::Processes, Locale::Th) => {
            "ติดตามกระบวนการธุรกิจที่มองเห็นได้ภายใต้ขอบเขตสิทธิ์ปัจจุบัน".to_owned()
        }
        (RouteQuery::Insights, Locale::En) => {
            "Review trusted operational insights without leaving your authorized scope.".to_owned()
        }
        (RouteQuery::Insights, Locale::Th) => {
            "ตรวจสอบข้อมูลเชิงลึกด้านปฏิบัติการภายในขอบเขตที่คุณได้รับอนุญาต".to_owned()
        }
        (RouteQuery::PlatformHoldings, Locale::En) => {
            "Review the platform holdings available to the active administrator role.".to_owned()
        }
        (RouteQuery::PlatformHoldings, Locale::Th) => {
            "ตรวจสอบโฮลดิ้งบนแพลตฟอร์มที่พร้อมใช้สำหรับบทบาทผู้ดูแลปัจจุบัน".to_owned()
        }
        (RouteQuery::PlatformHolding { .. }, Locale::En) => {
            "Review this holding and the records exposed to the active platform scope.".to_owned()
        }
        (RouteQuery::PlatformHolding { .. }, Locale::Th) => {
            "ตรวจสอบโฮลดิ้งนี้และรายการที่เปิดให้กับขอบเขตแพลตฟอร์มปัจจุบัน".to_owned()
        }
        (RouteQuery::Workspace { .. }, Locale::En) => {
            format!("Browse {heading} records available to your current role and scope.")
        }
        (RouteQuery::Workspace { .. }, Locale::Th) => {
            format!("ดูรายการ {heading} ที่พร้อมใช้สำหรับบทบาทและขอบเขตปัจจุบัน")
        }
        (RouteQuery::Record { .. }, Locale::En) => {
            "Review this record within the permissions of your active role and scope.".to_owned()
        }
        (RouteQuery::Record { .. }, Locale::Th) => {
            "ตรวจสอบรายการนี้ภายใต้สิทธิ์ของบทบาทและขอบเขตที่กำลังใช้งาน".to_owned()
        }
        (RouteQuery::Reference { .. }, Locale::En) => {
            "Review this shared reference from an authorized workspace.".to_owned()
        }
        (RouteQuery::Reference { .. }, Locale::Th) => {
            "ตรวจสอบข้อมูลอ้างอิงร่วมนี้จากพื้นที่ทำงานที่ได้รับอนุญาต".to_owned()
        }
        (RouteQuery::Modules, Locale::En) => "Browse authorized ERPX modules.".to_owned(),
        (RouteQuery::Modules, Locale::Th) => "ดูโมดูล ERPX ที่ได้รับอนุญาต".to_owned(),
    }
}

#[cfg(feature = "native")]
fn render_delivery_error(error: erpx_delivery_contract::DeliveryError) -> RenderError {
    RenderError::from(std::io::Error::other(error.to_string()))
}

pub fn is_canonical_console_path(path: &str, channel: ClientChannel) -> bool {
    if !path.starts_with('/') || path.contains(['?', '#']) || path != path.trim() {
        return false;
    }
    match AppRoute::from_str(path) {
        Ok(AppRoute::NotFound { .. }) | Err(_) => false,
        Ok(AppRoute::SignIn { .. }) => path == "/auth/sign-in",
        Ok(AppRoute::Accounts { .. }) => path == "/auth/accounts",
        Ok(route @ (AppRoute::CommerceWorkspace {} | AppRoute::TalentWorkspace {})) => {
            route_path(&route) == path && channel == ClientChannel::Web
        }
        Ok(
            route @ (AppRoute::WorkspaceResource { workspace, .. }
            | AppRoute::WorkspaceRecord { workspace, .. }),
        ) => route_path(&route) == path && workspace.available_on(channel),
        Ok(route) => route_path(&route) == path,
    }
}

fn route_path(route: &AppRoute) -> String {
    route
        .to_string()
        .split_once('?')
        .map_or_else(|| route.to_string(), |(path, _)| path.to_owned())
}

#[cfg(feature = "server")]
pub(crate) fn route_data_for_query(request: &QueryRequest) -> RouteData {
    let canonical_path = canonical_path_for_query(&request.route);
    let items = Vec::<erpx_delivery_contract::ResourceSummary>::new();
    let title = match &request.route {
        RouteQuery::Work => "Work".to_owned(),
        RouteQuery::Processes => "Processes".to_owned(),
        RouteQuery::Insights => "Insights".to_owned(),
        RouteQuery::Modules => "Modules".to_owned(),
        RouteQuery::PlatformHoldings => "Platform Holdings".to_owned(),
        RouteQuery::PlatformHolding { holding_id } => items.first().map_or_else(
            || format!("Platform Holding · {holding_id}"),
            |item| item.title.clone(),
        ),
        RouteQuery::Workspace {
            workspace,
            resource,
        } => resource
            .as_ref()
            .map_or_else(|| workspace.to_string(), ToString::to_string),
        RouteQuery::Record { resource, id, .. } => items
            .first()
            .map_or_else(|| format!("{resource} · {id}"), |item| item.title.clone()),
        RouteQuery::Reference { id } => items
            .first()
            .map_or_else(|| format!("Reference · {id}"), |item| item.title.clone()),
    };
    let tag = match &request.route {
        RouteQuery::Workspace { workspace, .. } | RouteQuery::Record { workspace, .. } => {
            format!("workspace:{workspace}")
        }
        _ => format!(
            "route:{}",
            canonical_path.trim_start_matches('/').replace('/', ":")
        ),
    };
    RouteData {
        canonical_path,
        title,
        description: "This route has no feature-owned authoritative projection connected yet."
            .to_owned(),
        page: Page {
            items,
            next_cursor: None,
        },
        freshness_tag: "not-connected".to_owned(),
        invalidation_tags: BTreeSet::from([tag]),
        public_validator: None,
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn canonical_path_for_query(route: &RouteQuery) -> String {
    match route {
        RouteQuery::Work => "/work".to_owned(),
        RouteQuery::Processes => "/processes".to_owned(),
        RouteQuery::Insights => "/insights".to_owned(),
        RouteQuery::Modules => "/modules".to_owned(),
        RouteQuery::PlatformHoldings => "/platform/holdings".to_owned(),
        RouteQuery::PlatformHolding { holding_id } => {
            format!("/platform/holdings/{holding_id}")
        }
        RouteQuery::Workspace {
            workspace,
            resource,
        } => resource.as_ref().map_or_else(
            || format!("/{workspace}"),
            |resource| format!("/{workspace}/{resource}"),
        ),
        RouteQuery::Record {
            workspace,
            resource,
            id,
        } => format!("/{workspace}/{resource}/{id}"),
        RouteQuery::Reference { id } => format!("/reference/item/{id}"),
    }
}

#[cfg(test)]
fn workspace_home_route(workspace: Workspace) -> AppRoute {
    match workspace {
        Workspace::Operations => AppRoute::OperationsWorkspace {},
        Workspace::Finance => AppRoute::FinanceWorkspace {},
        Workspace::Sales => AppRoute::SalesWorkspace {},
        Workspace::Procurement => AppRoute::ProcurementWorkspace {},
        Workspace::Inventory => AppRoute::InventoryWorkspace {},
        Workspace::Asset => AppRoute::AssetWorkspace {},
        Workspace::Maintenance => AppRoute::MaintenanceWorkspace {},
        Workspace::Quality => AppRoute::QualityWorkspace {},
        Workspace::Marketing => AppRoute::MarketingWorkspace {},
        Workspace::People => AppRoute::PeopleWorkspace {},
        Workspace::Pos => AppRoute::PosWorkspace {},
        Workspace::Legal => AppRoute::LegalWorkspace {},
        Workspace::Manufacturing => AppRoute::ManufacturingWorkspace {},
        Workspace::Planning => AppRoute::PlanningWorkspace {},
        Workspace::Project => AppRoute::ProjectWorkspace {},
        Workspace::Commerce => AppRoute::CommerceWorkspace {},
        Workspace::Talent => AppRoute::TalentWorkspace {},
        Workspace::Learning => AppRoute::LearningWorkspace {},
        Workspace::Benefits => AppRoute::BenefitsWorkspace {},
        Workspace::FieldService => AppRoute::FieldServiceWorkspace {},
        Workspace::Ai => AppRoute::AiWorkspace {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_routes_round_trip_for_every_workspace_and_channel() {
        let id = RouteUuid::try_new(uuid::Uuid::new_v4()).unwrap();
        for workspace in Workspace::ALL {
            let home = workspace_home_route(workspace);
            assert_eq!(
                AppRoute::from_str(&home.to_string()).expect("workspace home parses"),
                home
            );
            let resource = AppRoute::WorkspaceResource {
                workspace,
                resource: "orders".parse().unwrap(),
            };
            assert_eq!(
                AppRoute::from_str(&resource.to_string()).expect("workspace resource parses"),
                resource
            );
            let record = AppRoute::WorkspaceRecord {
                workspace,
                resource: "orders".parse().unwrap(),
                id,
            };
            assert_eq!(
                AppRoute::from_str(&record.to_string()).expect("workspace record parses"),
                record
            );
            assert_eq!(
                is_canonical_console_path(&home.to_string(), ClientChannel::MacOs),
                !workspace.is_web_only()
            );
        }
    }

    #[test]
    fn malformed_and_legacy_routes_have_no_compatibility_alias() {
        for path in [
            "/login",
            "/crm",
            "/payroll",
            "/commerce/",
            "/finance/orders/not-a-uuid",
            "/finance/PurchaseOrders",
        ] {
            assert!(
                !is_canonical_console_path(path, ClientChannel::Web),
                "{path}"
            );
        }
        assert!(is_canonical_console_path(
            "/auth/sign-in",
            ClientChannel::Web
        ));
        assert!(is_canonical_console_path(
            "/auth/accounts",
            ClientChannel::Web
        ));
    }

    #[test]
    fn platform_holding_routes_are_canonical_uuid_routes() {
        let holding_id = RouteUuid::try_new(uuid::Uuid::from_u128(1)).unwrap();
        let list = AppRoute::PlatformHoldings { cursor: None };
        let detail = AppRoute::PlatformHolding { holding_id };
        assert_eq!(route_path(&list), "/platform/holdings");
        assert_eq!(
            detail.to_string(),
            "/platform/holdings/00000000-0000-0000-0000-000000000001"
        );
        assert!(is_canonical_console_path(
            &route_path(&list),
            ClientChannel::Web
        ));
        assert!(is_canonical_console_path(
            &detail.to_string(),
            ClientChannel::Web
        ));
        assert!(!is_canonical_console_path(
            "/platform/holdings/not-a-uuid",
            ClientChannel::Web
        ));
    }

    #[test]
    fn identity_catalog_routes_and_details_are_canonical() {
        let id = RouteUuid::try_new(uuid::Uuid::from_u128(9)).unwrap();
        for path in [
            "/identity/principal",
            "/identity/role",
            "/identity/grant",
            "/identity/access-review",
            "/identity/break-glass-session",
        ] {
            assert!(
                is_canonical_console_path(path, ClientChannel::Web),
                "{path}"
            );
            assert!(is_canonical_console_path(
                &format!("{path}/{id}"),
                ClientChannel::Web
            ));
        }
        assert!(!is_canonical_console_path(
            "/identity/principal/not-a-uuid",
            ClientChannel::Web
        ));
    }

    #[test]
    fn canonical_uuid_and_reference_routes_round_trip() {
        let id = RouteUuid::try_new(uuid::Uuid::new_v4()).unwrap();
        let route = AppRoute::ReferenceItem { id };
        assert_eq!(
            AppRoute::from_str(&route.to_string()).expect("reference route parses"),
            route
        );
    }

    #[cfg(feature = "server")]
    #[test]
    fn unconnected_generic_routes_are_honest_empty_states() {
        let mut routes = vec![
            RouteQuery::Work,
            RouteQuery::Processes,
            RouteQuery::Insights,
        ];
        routes.extend(Workspace::ALL.map(|workspace| RouteQuery::Workspace {
            workspace,
            resource: None,
        }));

        for route in routes {
            let data = route_data_for_query(&QueryRequest {
                route,
                cursor: None,
                limit: 50,
                locale: "en".to_owned(),
            });
            assert!(data.page.items.is_empty());
            assert_eq!(data.freshness_tag, "not-connected");
            assert!(data.public_validator.is_none());
        }
    }
}
