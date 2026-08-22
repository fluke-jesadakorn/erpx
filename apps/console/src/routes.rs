#[cfg(feature = "server")]
use std::collections::BTreeSet;
#[cfg(any(feature = "web", feature = "server"))]
use std::time::Duration;
use std::{cell::RefCell, rc::Rc, str::FromStr};

use dioxus::prelude::*;
#[cfg(feature = "native")]
use erpx_console_feature_ai_collaboration::CollaborationGateway;
use erpx_console_feature_ai_collaboration::{
    ActivityRailState, ActivityRequest, AuthorizedRecordActivityRail, CollaborationMutation,
    FieldSuggestionReviewRequest, RecordReferences, RecordReferencesState, RecordResolutionState,
    ReferencesRequest, ResolveRecordRequest,
};
use erpx_console_feature_identity_access::{
    IdentityAccessDetailSurface, IdentityAccessListSurface, IdentityCommandFeedback,
    IdentityPrincipalCommandIntent, IdentityRecordKind,
};
use erpx_console_feature_platform_administration::{
    HoldingControlHistoryState, HoldingsSurface, PlatformHoldingDetailSurface,
};
use erpx_console_transport_core::CatalogRecord;
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
use erpx_ui_support::{
    IncidentDetailState, IncidentDetailSurface, IncidentListState, IncidentListSurface,
    SupportRoute, SupportState, SupportSurface, SupportTicketListState, SupportTicketListSurface,
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
    #[redirect("/", || AppRoute::Work { view: None, domain: None, severity: None, cursor: None })]
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
        #[route("/work?:view&:domain&:severity&:cursor")]
        Work {
            view: Option<String>,
            domain: Option<String>,
            severity: Option<String>,
            cursor: Option<String>,
        },
        #[route("/processes?:cursor")]
        Processes { cursor: Option<String> },
        #[route("/insights")]
        Insights {},
        #[route("/modules")]
        Modules {},
        #[route("/platform/holdings?:cursor")]
        PlatformHoldings { cursor: Option<String> },
        #[route("/platform/holdings/:holding_id?:history_cursor")]
        PlatformHolding {
            holding_id: RouteUuid,
            history_cursor: Option<String>,
        },
        #[route("/platform/holding/:holding_id?:history_cursor")]
        PlatformHoldingCanonical {
            holding_id: RouteUuid,
            history_cursor: Option<String>,
        },
        #[route("/platform/support")]
        PlatformSupport {},
        #[route("/platform/support/:id")]
        PlatformSupportTicket { id: RouteUuid },
        #[route("/platform/incidents")]
        PlatformIncidents {},
        #[route("/platform/incidents/:id")]
        PlatformIncident { id: RouteUuid },
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
        #[route("/:module/:record/:id")]
        CatalogRecordDetail {
            module: ResourceName,
            record: ResourceName,
            id: RouteUuid,
        },
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
                        class: nav_link_class(matches!(&active_route, AppRoute::Work { .. })),
                        aria_current: nav_aria_current(matches!(&active_route, AppRoute::Work { .. })),
                        {copy(locale, "Work", "งาน")}
                    }
                    KitLink {
                        href: "/processes",
                        class: nav_link_class(matches!(&active_route, AppRoute::Processes { .. })),
                        aria_current: nav_aria_current(matches!(&active_route, AppRoute::Processes { .. })),
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
        } if *workspace != Workspace::Ai => Some(format!("/{workspace}/{resource}")),
        RouteQuery::Workspace {
            workspace: Workspace::Ai,
            resource: Some(_),
        } => None,
        RouteQuery::Workspace {
            workspace,
            resource: None,
        } if *workspace != Workspace::Ai => Some(format!(
            "/{workspace}/{}",
            default_workspace_resource(*workspace)
        )),
        RouteQuery::Workspace {
            workspace: Workspace::Ai,
            resource: None,
        } => None,
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
    let authoritative_unavailable = route_freshness_is_unavailable(&data.freshness_tag);
    let has_items = !authoritative_unavailable && !data.page.items.is_empty();
    let unavailable_copy = authoritative_unavailable
        .then(|| route_unavailable_copy(&query_for_display, &heading, locale));
    let empty_state_class = if authoritative_unavailable {
        "empty-state empty-state--unavailable"
    } else {
        "empty-state"
    };
    let empty_state_role = if authoritative_unavailable {
        "alert"
    } else {
        "status"
    };
    let returned_count = if authoritative_unavailable {
        copy(locale, "Record count unavailable", "ยังไม่ทราบจำนวนรายการ").to_owned()
    } else {
        format!(
            "{} {}",
            data.page.items.len(),
            copy(locale, "records returned", "รายการที่ได้รับ")
        )
    };
    let freshness_label = match data.freshness_tag.as_str() {
        "not-connected" | "unavailable" => copy(
            locale,
            "Authoritative projection unavailable",
            "ข้อมูล projection ที่เชื่อถือได้ยังไม่พร้อมใช้งาน",
        )
        .to_owned(),
        value => format!("{}: {value}", copy(locale, "Status", "สถานะ")),
    };
    let visible_items = if authoritative_unavailable {
        Vec::new()
    } else {
        data.page
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
            .collect::<Vec<_>>()
    };
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
                section {
                    class: "{empty_state_class}",
                    role: "{empty_state_role}",
                    "data-state": if authoritative_unavailable { "unavailable" } else if has_items { "filtered-empty" } else { "empty" },
                    span { class: "empty-state__icon", aria_hidden: "true", if authoritative_unavailable { "!" } else if has_items { "⌕" } else { "◇" } }
                    div { class: "empty-state__copy",
                        if let Some(unavailable) = &unavailable_copy {
                            h2 { "{unavailable.title}" }
                            p { "{unavailable.reason}" }
                        } else if has_items {
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

#[cfg(any(feature = "web", feature = "server"))]
fn parse_work_view(value: Option<&str>) -> Option<erpx_ui_core::WorkQueueView> {
    match value {
        None | Some("") => Some(erpx_ui_core::WorkQueueView::All),
        Some(value) => erpx_ui_core::WorkQueueView::parse(value),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn parse_work_domain(value: Option<&str>) -> Option<Option<erpx_ui_core::WorkDomain>> {
    Some(match value {
        None | Some("") | Some("all") => None,
        Some("operations") => Some(erpx_ui_core::WorkDomain::Operations),
        Some("support") => Some(erpx_ui_core::WorkDomain::Support),
        Some("finance") => Some(erpx_ui_core::WorkDomain::Finance),
        Some("people") => Some(erpx_ui_core::WorkDomain::People),
        Some("maintenance") => Some(erpx_ui_core::WorkDomain::Maintenance),
        Some("procurement") => Some(erpx_ui_core::WorkDomain::Procurement),
        Some(_) => return None,
    })
}

#[cfg(any(feature = "web", feature = "server"))]
fn parse_work_severity(value: Option<&str>) -> Option<Option<erpx_ui_core::WorkItemSeverity>> {
    Some(match value {
        None | Some("") | Some("all") => None,
        Some("critical") => Some(erpx_ui_core::WorkItemSeverity::Critical),
        Some("high") => Some(erpx_ui_core::WorkItemSeverity::High),
        Some("medium") => Some(erpx_ui_core::WorkItemSeverity::Medium),
        Some("low") => Some(erpx_ui_core::WorkItemSeverity::Low),
        Some("informational") => Some(erpx_ui_core::WorkItemSeverity::Informational),
        Some(_) => return None,
    })
}

#[cfg(any(feature = "web", feature = "server"))]
fn work_filter_href(
    view: erpx_ui_core::WorkQueueView,
    domain: Option<erpx_ui_core::WorkDomain>,
    severity: Option<erpx_ui_core::WorkItemSeverity>,
    cursor: Option<&str>,
) -> String {
    let mut values = vec![format!("view={}", view.query_value())];
    if let Some(domain) = domain {
        values.push(format!("domain={}", domain.as_str()));
    }
    if let Some(severity) = severity {
        let value = match severity {
            erpx_ui_core::WorkItemSeverity::Critical => "critical",
            erpx_ui_core::WorkItemSeverity::High => "high",
            erpx_ui_core::WorkItemSeverity::Medium => "medium",
            erpx_ui_core::WorkItemSeverity::Low => "low",
            erpx_ui_core::WorkItemSeverity::Informational => "informational",
        };
        values.push(format!("severity={value}"));
    }
    if let Some(cursor) = cursor {
        values.push(format!("cursor={cursor}"));
    }
    format!("/work?{}", values.join("&"))
}

#[cfg(any(feature = "web", feature = "server"))]
fn work_view_label(value: erpx_ui_core::WorkQueueView, locale: Locale) -> &'static str {
    match value {
        erpx_ui_core::WorkQueueView::All => copy(locale, "All", "ทั้งหมด"),
        erpx_ui_core::WorkQueueView::Mine => copy(locale, "Mine", "ของฉัน"),
        erpx_ui_core::WorkQueueView::Team => copy(locale, "Team", "ทีม"),
        erpx_ui_core::WorkQueueView::Overdue => copy(locale, "Overdue", "เกินกำหนด"),
        erpx_ui_core::WorkQueueView::Approvals => copy(locale, "Approvals", "รออนุมัติ"),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn work_domain_label(value: erpx_ui_core::WorkDomain, locale: Locale) -> &'static str {
    match value {
        erpx_ui_core::WorkDomain::Operations => copy(locale, "Operations", "ปฏิบัติการ"),
        erpx_ui_core::WorkDomain::Support => copy(locale, "Support", "สนับสนุน"),
        erpx_ui_core::WorkDomain::Finance => copy(locale, "Finance", "การเงิน"),
        erpx_ui_core::WorkDomain::People => copy(locale, "People", "บุคลากร"),
        erpx_ui_core::WorkDomain::Maintenance => copy(locale, "Maintenance", "บำรุงรักษา"),
        erpx_ui_core::WorkDomain::Procurement => copy(locale, "Procurement", "จัดซื้อ"),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn work_severity_label(value: erpx_ui_core::WorkItemSeverity, locale: Locale) -> &'static str {
    match value {
        erpx_ui_core::WorkItemSeverity::Critical => copy(locale, "Critical", "วิกฤต"),
        erpx_ui_core::WorkItemSeverity::High => copy(locale, "High", "สูง"),
        erpx_ui_core::WorkItemSeverity::Medium => copy(locale, "Medium", "ปานกลาง"),
        erpx_ui_core::WorkItemSeverity::Low => copy(locale, "Low", "ต่ำ"),
        erpx_ui_core::WorkItemSeverity::Informational => copy(locale, "Info", "ข้อมูล"),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn work_availability_label(
    value: erpx_ui_core::ReadModelAvailability,
    locale: Locale,
) -> &'static str {
    match value {
        erpx_ui_core::ReadModelAvailability::Available => copy(locale, "Available", "พร้อมใช้"),
        erpx_ui_core::ReadModelAvailability::Partial => copy(locale, "Partial", "บางส่วน"),
        erpx_ui_core::ReadModelAvailability::Empty => {
            copy(locale, "Authoritative empty", "ว่างตามข้อมูลที่เชื่อถือได้")
        }
        erpx_ui_core::ReadModelAvailability::Unavailable => {
            copy(locale, "Unavailable", "ไม่พร้อมใช้งาน")
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn work_error_copy(
    error: &erpx_delivery_contract::DeliveryError,
    locale: Locale,
) -> (&'static str, &'static str) {
    use erpx_delivery_contract::DeliveryError;
    match (error, locale) {
        (DeliveryError::Unauthenticated(_), Locale::En) => (
            "Work session unavailable",
            "Reason: the authenticated web session is no longer valid. Sign in again.",
        ),
        (DeliveryError::Unauthenticated(_), Locale::Th) => (
            "เซสชันงานไม่พร้อมใช้งาน",
            "เหตุผล: เซสชันเว็บที่ยืนยันตัวตนแล้วไม่ถูกต้องอีกต่อไป โปรดเข้าสู่ระบบใหม่",
        ),
        (DeliveryError::Forbidden(_), Locale::En) => (
            "Work access denied",
            "Reason: the active role or authority scope cannot read this work queue.",
        ),
        (DeliveryError::Forbidden(_), Locale::Th) => (
            "ไม่มีสิทธิ์เข้าถึงงาน",
            "เหตุผล: บทบาทหรือขอบเขตสิทธิ์ที่ใช้งานอยู่ไม่สามารถอ่านคิวงานนี้ได้",
        ),
        (DeliveryError::Validation(_), Locale::En) => (
            "Work filter invalid",
            "Reason: a cursor or bounded work filter was not accepted.",
        ),
        (DeliveryError::Validation(_), Locale::Th) => (
            "ตัวกรองงานไม่ถูกต้อง",
            "เหตุผล: ระบบไม่ยอมรับ cursor หรือตัวกรองงานแบบมีขอบเขต",
        ),
        (DeliveryError::Unavailable(_), Locale::En) => (
            "Work contributors unavailable",
            "Reason: authorized owner projections or the session authority service could not be reached.",
        ),
        (DeliveryError::Unavailable(_), Locale::Th) => (
            "แหล่งข้อมูลงานไม่พร้อมใช้งาน",
            "เหตุผล: ไม่สามารถเชื่อมต่อ projection ของเจ้าของข้อมูลที่ได้รับอนุญาตหรือบริการสิทธิ์ของเซสชันได้",
        ),
        (_, Locale::En) => (
            "Work request failed",
            "Reason: an owner returned invalid evidence or could not complete the query.",
        ),
        (_, Locale::Th) => (
            "คำของานไม่สำเร็จ",
            "เหตุผล: บริการเจ้าของข้อมูลส่งหลักฐานที่ไม่ถูกต้องหรือไม่สามารถทำคำขอให้เสร็จได้",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn Work(
    view: Option<String>,
    domain: Option<String>,
    severity: Option<String>,
    cursor: Option<String>,
) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let Some(view) = parse_work_view(view.as_deref()) else {
        return rsx! { section { class: "route-denied", role: "alert",
            h1 { {copy(locale, "Work filter invalid", "ตัวกรองงานไม่ถูกต้อง")} }
            p { {copy(locale, "Reason: the requested work view is not recognized.", "เหตุผล: ไม่รู้จักมุมมองงานที่ร้องขอ")} }
            KitLink { href: "/work", {copy(locale, "Reset filters", "ล้างตัวกรอง")} }
        } };
    };
    let Some(domain) = parse_work_domain(domain.as_deref()) else {
        return rsx! { section { class: "route-denied", role: "alert",
            h1 { {copy(locale, "Work filter invalid", "ตัวกรองงานไม่ถูกต้อง")} }
            p { {copy(locale, "Reason: the requested work domain is not recognized.", "เหตุผล: ไม่รู้จักโดเมนงานที่ร้องขอ")} }
            KitLink { href: "/work", {copy(locale, "Reset filters", "ล้างตัวกรอง")} }
        } };
    };
    let Some(severity) = parse_work_severity(severity.as_deref()) else {
        return rsx! { section { class: "route-denied", role: "alert",
            h1 { {copy(locale, "Work filter invalid", "ตัวกรองงานไม่ถูกต้อง")} }
            p { {copy(locale, "Reason: the requested severity is not recognized.", "เหตุผล: ไม่รู้จักระดับความรุนแรงที่ร้องขอ")} }
            KitLink { href: "/work", {copy(locale, "Reset filters", "ล้างตัวกรอง")} }
        } };
    };
    if cursor
        .as_deref()
        .is_some_and(|value| !erpx_ui_core::valid_work_cursor(value))
    {
        return rsx! { section { class: "route-denied", role: "alert",
            h1 { {copy(locale, "Work cursor invalid", "cursor ของงานไม่ถูกต้อง")} }
            p { {copy(locale, "Reason: the page cursor is malformed or outside its bounded range.", "เหตุผล: cursor ของหน้ามีรูปแบบไม่ถูกต้องหรืออยู่นอกขอบเขต")} }
            KitLink { href: "/work", {copy(locale, "Return to first page", "กลับไปหน้าแรก")} }
        } };
    }
    let request = crate::work_delivery::AuthorizedWorkRequest {
        view,
        domain,
        severity,
        cursor: cursor.clone(),
        page_size: 24,
        locale: locale.language_tag().into(),
        consistency: crate::work_delivery::WorkConsistency::Eventual,
    };
    let mut resource = use_server_future(move || {
        let request = request.clone();
        async move {
            match crate::work_delivery::list_authorized_work(request).await {
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
        .expect("resolved authorized work future has a value");
    let title = format!("{} · ERPX", copy(locale, "Work", "งาน"));
    let view_links = [
        erpx_ui_core::WorkQueueView::All,
        erpx_ui_core::WorkQueueView::Mine,
        erpx_ui_core::WorkQueueView::Team,
        erpx_ui_core::WorkQueueView::Overdue,
        erpx_ui_core::WorkQueueView::Approvals,
    ];
    let domain_links = [
        erpx_ui_core::WorkDomain::Operations,
        erpx_ui_core::WorkDomain::Support,
        erpx_ui_core::WorkDomain::Finance,
        erpx_ui_core::WorkDomain::People,
        erpx_ui_core::WorkDomain::Maintenance,
        erpx_ui_core::WorkDomain::Procurement,
    ];
    let severity_links = [
        erpx_ui_core::WorkItemSeverity::Critical,
        erpx_ui_core::WorkItemSeverity::High,
        erpx_ui_core::WorkItemSeverity::Medium,
        erpx_ui_core::WorkItemSeverity::Low,
        erpx_ui_core::WorkItemSeverity::Informational,
    ];

    rsx! {
        document::Title { "{title}" }
        article { class: "workspace-route authorized-work-page",
            nav { class: "route-breadcrumb", aria_label: copy(locale, "Breadcrumb", "เส้นทางนำทาง"),
                KitLink { href: "/modules", {copy(locale, "Modules", "โมดูล")} }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", {copy(locale, "Work", "งาน")} }
            }
            header {
                p { class: "eyebrow", {copy(locale, "AUTHORIZED WORK SHELL", "ศูนย์งานตามสิทธิ์")} }
                h1 { {copy(locale, "Work requiring attention", "งานที่ต้องดำเนินการ")} }
                p { {copy(locale,
                    "Owner services contribute only work authorized for the active role and scope. Every item returns to its canonical record.",
                    "บริการเจ้าของข้อมูลส่งเฉพาะงานที่ได้รับอนุญาตสำหรับบทบาทและขอบเขตปัจจุบัน แต่ละรายการเชื่อมกลับไปยังระเบียนหลัก",
                )} }
            }
            match page_state {
                Err(error) => {
                    let (error_title, reason) = work_error_copy(&error, locale);
                    rsx! {
                        section { class: "route-denied", role: "alert", "data-reason": "authorized_work_query_failed",
                            h2 { "{error_title}" }
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
                                KitLink { href: "/auth/accounts", {copy(locale, "Review access", "ตรวจสอบสิทธิ์")} }
                            }
                        }
                    }
                },
                Ok(page) => {
                    let queue = &page.queue;
                    let incomplete = queue.meta.availability == erpx_ui_core::ReadModelAvailability::Partial;
                    let unavailable = queue.meta.availability == erpx_ui_core::ReadModelAvailability::Unavailable;
                    let next_href = queue.next_cursor.as_deref().map(|value| work_filter_href(view, domain, severity, Some(value)));
                    let first_href = work_filter_href(view, domain, severity, None);
                    rsx! {
                        section { class: "work-evidence", role: "status", "data-availability": "{work_availability_label(queue.meta.availability, Locale::En).to_ascii_lowercase()}",
                            div {
                                strong { {work_availability_label(queue.meta.availability, locale)} }
                                p {
                                    {copy(locale, "Measured at", "วัดเมื่อ")}
                                    " {queue.meta.as_of} · "
                                    {copy(locale, "Scope", "ขอบเขต")}
                                    ": {queue.meta.scope}"
                                }
                            }
                            small {
                                {copy(locale, "Reference", "รหัสอ้างอิง")}
                                ": {page.correlation_id}"
                                if let Some(version) = page.authorization_context_version {
                                    " · auth-context v{version}"
                                }
                            }
                        }
                        section { class: "work-filter-panel", aria_label: copy(locale, "Work filters", "ตัวกรองงาน"),
                            div { class: "work-filter-group",
                                strong { {copy(locale, "View", "มุมมอง")} }
                                nav {
                                    for option in view_links {
                                        KitLink {
                                            class: if option == view { "work-filter active" } else { "work-filter" },
                                            href: work_filter_href(option, domain, severity, None),
                                            aria_current: if option == view { "page" } else { "false" },
                                            {work_view_label(option, locale)}
                                        }
                                    }
                                }
                            }
                            div { class: "work-filter-group",
                                strong { {copy(locale, "Domain", "โดเมน")} }
                                nav {
                                    KitLink {
                                        class: if domain.is_none() { "work-filter active" } else { "work-filter" },
                                        href: work_filter_href(view, None, severity, None),
                                        {copy(locale, "All domains", "ทุกโดเมน")}
                                    }
                                    for option in domain_links {
                                        KitLink {
                                            class: if domain == Some(option) { "work-filter active" } else { "work-filter" },
                                            href: work_filter_href(view, Some(option), severity, None),
                                            {work_domain_label(option, locale)}
                                        }
                                    }
                                }
                            }
                            div { class: "work-filter-group",
                                strong { {copy(locale, "Severity", "ความรุนแรง")} }
                                nav {
                                    KitLink {
                                        class: if severity.is_none() { "work-filter active" } else { "work-filter" },
                                        href: work_filter_href(view, domain, None, None),
                                        {copy(locale, "All levels", "ทุกระดับ")}
                                    }
                                    for option in severity_links {
                                        KitLink {
                                            class: if severity == Some(option) { "work-filter active" } else { "work-filter" },
                                            href: work_filter_href(view, domain, Some(option), None),
                                            {work_severity_label(option, locale)}
                                        }
                                    }
                                }
                            }
                        }
                        if unavailable {
                            section { class: "work-unavailable", role: "alert",
                                h2 { {copy(locale, "No authoritative contributor is available", "ไม่มีแหล่งข้อมูลที่เชื่อถือได้พร้อมใช้งาน")} }
                                p { {copy(locale,
                                    "The queue cannot report a successful empty result or a zero total while every authorized contributor is unavailable.",
                                    "คิวงานจะไม่รายงานผลลัพธ์ว่างหรือยอดรวมศูนย์เป็นผลสำเร็จ ในขณะที่ contributor ที่ได้รับอนุญาตทั้งหมดไม่พร้อมใช้งาน",
                                )} }
                            }
                        } else {
                            section { class: "work-counter-grid", aria_label: copy(locale, "Work counts", "จำนวนงาน"),
                                if incomplete && queue.counters.total == 0 {
                                    article { class: "work-counter incomplete",
                                        strong { "—" }
                                        span { {copy(locale, "Complete total unavailable", "ยอดรวมที่สมบูรณ์ยังไม่พร้อม")} }
                                    }
                                } else {
                                    article { class: "work-counter", strong { "{queue.counters.total}" } span { if incomplete { {copy(locale, "Known active items", "งานที่ทราบ")} } else { {copy(locale, "Active items", "งานที่กำลังดำเนินการ")} } } }
                                    article { class: "work-counter", strong { "{queue.counters.overdue}" } span { {copy(locale, "Overdue", "เกินกำหนด")} } }
                                    article { class: "work-counter", strong { "{queue.counters.approvals}" } span { {copy(locale, "Approvals", "รออนุมัติ")} } }
                                    article { class: "work-counter", strong { "{queue.counters.exceptions}" } span { {copy(locale, "Exceptions", "ข้อยกเว้น")} } }
                                }
                            }
                            if queue.items.is_empty() {
                                section { class: "empty-state", role: "status",
                                    span { class: "empty-state__icon", aria_hidden: "true", "◇" }
                                    div { class: "empty-state__copy",
                                        if queue.meta.availability == erpx_ui_core::ReadModelAvailability::Empty {
                                            h2 { {copy(locale, "Authoritative queue is empty", "คิวงานว่างตามข้อมูลที่เชื่อถือได้")} }
                                            p { {copy(locale,
                                                "Every responding authorized contributor returned no active work; no sample records were created.",
                                                "contributor ที่ได้รับอนุญาตและตอบกลับทั้งหมดไม่พบงานที่กำลังดำเนินการ ระบบไม่ได้สร้างข้อมูลตัวอย่าง",
                                            )} }
                                        } else {
                                            h2 { {copy(locale, "No items match this bounded view", "ไม่มีงานตรงกับมุมมองที่กำหนด")} }
                                            p { {copy(locale,
                                                "Responding contributors returned no matching items. Unavailable contributors are not counted as zero.",
                                                "contributor ที่ตอบกลับไม่พบรายการที่ตรงกัน ส่วน contributor ที่ไม่พร้อมใช้งานจะไม่ถูกนับเป็นศูนย์",
                                            )} }
                                        }
                                    }
                                    KitLink { class: "route-action", href: "/work", {copy(locale, "Clear filters", "ล้างตัวกรอง")} }
                                }
                            } else {
                                ul { class: "work-item-list", aria_label: copy(locale, "Authorized work items", "รายการงานตามสิทธิ์"),
                                    for item in queue.items.iter() {
                                        li { class: "work-item-card", key: "{item.key}", "data-severity": "{work_severity_label(item.severity, Locale::En).to_ascii_lowercase()}",
                                            div { class: "work-item-card__main",
                                                div { class: "work-item-card__badges",
                                                    span { class: "work-domain-badge", {work_domain_label(item.domain, locale)} }
                                                    span { class: "record-status", "{item.status_label}" }
                                                    if item.freshness == erpx_ui_core::ReadModelFreshness::Stale {
                                                        span { class: "work-stale-badge", {copy(locale, "Stale · read only", "ข้อมูลเก่า · อ่านอย่างเดียว")} }
                                                    }
                                                }
                                                h2 { "{item.title}" }
                                                p { "{item.summary}" }
                                                small {
                                                    {copy(locale, "Version", "เวอร์ชัน")}
                                                    " {item.record_version} · "
                                                    {copy(locale, "Severity", "ความรุนแรง")}
                                                    ": "
                                                    {work_severity_label(item.severity, locale)}
                                                    if let Some(due_at) = item.due_at.as_ref() {
                                                        " · "
                                                        {copy(locale, "Due", "ครบกำหนด")}
                                                        ": {due_at}"
                                                    }
                                                }
                                            }
                                            if let Some(record_ref) = item.target.record_ref.as_ref() {
                                                KitLink {
                                                    class: "route-action route-action--primary",
                                                    href: "{item.target.route.path()}/{record_ref}",
                                                    {copy(locale, "Open owner record", "เปิดระเบียนเจ้าของข้อมูล")}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if next_href.is_some() || cursor.is_some() {
                                div { class: "route-actions work-pagination",
                                    if let Some(next_href) = next_href.as_ref() {
                                        KitLink { href: "{next_href}", {copy(locale, "Next page", "หน้าถัดไป")} }
                                    }
                                    KitLink { href: "{first_href}", {copy(locale, "First page", "หน้าแรก")} }
                                }
                            }
                        }
                        section { class: "work-contributors", aria_label: copy(locale, "Contributor status", "สถานะ contributor"),
                            header {
                                h2 { {copy(locale, "Authoritative contributors", "แหล่งข้อมูลที่เชื่อถือได้")} }
                                p { {copy(locale,
                                    "Unavailable owners stay visible as degraded evidence and never become synthetic zeroes.",
                                    "บริการเจ้าของข้อมูลที่ไม่พร้อมใช้จะแสดงเป็นหลักฐานสถานะผิดปกติ และไม่ถูกแปลงเป็นค่าศูนย์ที่สร้างขึ้นเอง",
                                )} }
                            }
                            div { class: "work-contributor-grid",
                                for source in queue.contributions.iter() {
                                    article { class: "work-contributor", "data-availability": "{work_availability_label(source.availability, Locale::En).to_ascii_lowercase()}",
                                        div {
                                            strong { "{source.label}" }
                                            span { {work_availability_label(source.availability, locale)} }
                                        }
                                        p { "{source.message}" }
                                        if let Some(reason_code) = source.reason_code.as_ref() {
                                            small {
                                                {copy(locale, "Reason", "เหตุผล")}
                                                ": {reason_code}"
                                            }
                                        }
                                        if source.availability != erpx_ui_core::ReadModelAvailability::Unavailable {
                                            small {
                                                {copy(locale, "Active items in bounded owner window", "งานที่กำลังดำเนินการในหน้าข้อมูลเจ้าของที่มีขอบเขต")}
                                                ": {source.item_count}"
                                            }
                                        }
                                    }
                                }
                            }
                            if !page.projection_cursors.is_empty() {
                                Collapsible { class: "work-projection-evidence",
                                    CollapsibleTrigger { class: "work-projection-evidence__trigger",
                                        {copy(locale, "Projection evidence", "หลักฐาน projection")}
                                    }
                                    CollapsibleContent {
                                        ul {
                                            for evidence in page.projection_cursors.iter() {
                                                li {
                                                    code { "{evidence.source}" }
                                                    " · {evidence.projection} · seq {evidence.global_sequence} · {evidence.measured_at}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        p { class: "work-redaction-note",
                            {copy(locale,
                                "Credentials, customer identity, ticket descriptions, and owner-internal payloads are redacted by contract.",
                                "ข้อมูลรับรอง ตัวตนลูกค้า รายละเอียดคำขอ และ payload ภายในของเจ้าของข้อมูลถูกปกปิดตาม contract",
                            )}
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn Work(
    _view: Option<String>,
    _domain: Option<String>,
    _severity: Option<String>,
    _cursor: Option<String>,
) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    rsx! {
        section { class: "route-denied", role: "alert", "data-reason": "native_work_transport_not_ported",
            h1 { {copy(locale, "Work unavailable on Native", "งานยังไม่พร้อมบน Native")} }
            p { {copy(locale,
                "Reason: the typed Native Tonic contributor adapter has not been certified, so this capability fails closed.",
                "เหตุผล: typed Native Tonic contributor adapter ยังไม่ผ่านการรับรอง ความสามารถนี้จึงปิดแบบ fail closed",
            )} }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn process_stage_key(stage: crate::process_delivery::ProcessStage) -> &'static str {
    match stage {
        crate::process_delivery::ProcessStage::Pending => "pending",
        crate::process_delivery::ProcessStage::Running => "running",
        crate::process_delivery::ProcessStage::RetryScheduled => "retry-scheduled",
        crate::process_delivery::ProcessStage::Failed => "failed",
        crate::process_delivery::ProcessStage::Completed => "completed",
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn process_stage_label(
    stage: crate::process_delivery::ProcessStage,
    locale: Locale,
) -> &'static str {
    match stage {
        crate::process_delivery::ProcessStage::Pending => copy(locale, "Pending", "รอดำเนินการ"),
        crate::process_delivery::ProcessStage::Running => copy(locale, "Running", "กำลังดำเนินการ"),
        crate::process_delivery::ProcessStage::RetryScheduled => {
            copy(locale, "Retry scheduled", "กำหนดลองใหม่แล้ว")
        }
        crate::process_delivery::ProcessStage::Failed => copy(locale, "Failed", "ล้มเหลว"),
        crate::process_delivery::ProcessStage::Completed => copy(locale, "Completed", "เสร็จสมบูรณ์"),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn process_kind_label(kind: crate::process_delivery::ProcessKind, locale: Locale) -> &'static str {
    match kind {
        crate::process_delivery::ProcessKind::HoldingBackup => copy(
            locale,
            "Holding backup lifecycle",
            "lifecycle การสำรองข้อมูลโฮลดิ้ง",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn process_contribution_message(
    availability: erpx_ui_core::ReadModelAvailability,
    locale: Locale,
) -> &'static str {
    match availability {
        erpx_ui_core::ReadModelAvailability::Unavailable => copy(
            locale,
            "The bounded-context owner could not provide process state.",
            "ไม่สามารถอ่านสถานะกระบวนการจากบริการเจ้าของ bounded context ได้",
        ),
        erpx_ui_core::ReadModelAvailability::Empty => copy(
            locale,
            "The owner reports no backup jobs in this bounded scope.",
            "บริการเจ้าของข้อมูลยืนยันว่าไม่มีงานสำรองข้อมูลในขอบเขตนี้",
        ),
        erpx_ui_core::ReadModelAvailability::Partial => copy(
            locale,
            "Only lifecycle evidence confirmed within the bounded owner window is shown.",
            "แสดงเฉพาะหลักฐาน lifecycle ที่ยืนยันได้ภายในหน้าต่างข้อมูลเจ้าของที่มีขอบเขต",
        ),
        erpx_ui_core::ReadModelAvailability::Available => copy(
            locale,
            "Lifecycle state was read directly from the owner service.",
            "อ่านสถานะ lifecycle จากบริการเจ้าของข้อมูลโดยตรง",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn process_freshness_label(
    freshness: erpx_ui_core::ReadModelFreshness,
    locale: Locale,
) -> &'static str {
    match freshness {
        erpx_ui_core::ReadModelFreshness::Live => copy(locale, "Live", "ข้อมูลสด"),
        erpx_ui_core::ReadModelFreshness::Current => copy(locale, "Current", "เป็นปัจจุบัน"),
        erpx_ui_core::ReadModelFreshness::Stale => copy(locale, "Stale", "ข้อมูลเก่า"),
        erpx_ui_core::ReadModelFreshness::Unknown => copy(locale, "Unknown", "ไม่ทราบ"),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn process_error_copy(
    error: &erpx_delivery_contract::DeliveryError,
    locale: Locale,
) -> (&'static str, &'static str) {
    use erpx_delivery_contract::DeliveryError;
    match (error, locale) {
        (DeliveryError::Unauthenticated(_), Locale::En) => (
            "Process session unavailable",
            "Reason: the authenticated web session is no longer valid. Sign in again.",
        ),
        (DeliveryError::Unauthenticated(_), Locale::Th) => (
            "เซสชันกระบวนการไม่พร้อมใช้งาน",
            "เหตุผล: เซสชันเว็บที่ยืนยันตัวตนแล้วไม่ถูกต้องอีกต่อไป โปรดเข้าสู่ระบบใหม่",
        ),
        (DeliveryError::Forbidden(_), Locale::En) => (
            "Process access denied",
            "Reason: the active role or authority scope cannot read these owner lifecycles.",
        ),
        (DeliveryError::Forbidden(_), Locale::Th) => (
            "ไม่มีสิทธิ์เข้าถึงกระบวนการ",
            "เหตุผล: บทบาทหรือขอบเขตสิทธิ์ที่ใช้งานอยู่ไม่สามารถอ่าน lifecycle จากบริการเจ้าของข้อมูลเหล่านี้ได้",
        ),
        (DeliveryError::Validation(_), Locale::En) => (
            "Process cursor invalid",
            "Reason: the bounded process page cursor was not accepted.",
        ),
        (DeliveryError::Validation(_), Locale::Th) => (
            "cursor ของกระบวนการไม่ถูกต้อง",
            "เหตุผล: ระบบไม่ยอมรับ cursor ของหน้ากระบวนการแบบมีขอบเขต",
        ),
        (DeliveryError::Unavailable(_), Locale::En) => (
            "Process owners unavailable",
            "Reason: an authorized owner service or the session authority service could not be reached.",
        ),
        (DeliveryError::Unavailable(_), Locale::Th) => (
            "บริการเจ้าของกระบวนการไม่พร้อมใช้งาน",
            "เหตุผล: ไม่สามารถเชื่อมต่อบริการเจ้าของข้อมูลที่ได้รับอนุญาตหรือบริการสิทธิ์ของเซสชันได้",
        ),
        (_, Locale::En) => (
            "Process request failed",
            "Reason: an owner returned invalid lifecycle evidence or could not complete the query.",
        ),
        (_, Locale::Th) => (
            "คำขอกระบวนการไม่สำเร็จ",
            "เหตุผล: บริการเจ้าของข้อมูลส่งหลักฐาน lifecycle ที่ไม่ถูกต้องหรือไม่สามารถทำคำขอให้เสร็จได้",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn Processes(cursor: Option<String>) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    if cursor
        .as_deref()
        .is_some_and(|value| !crate::process_delivery::valid_process_cursor(value))
    {
        return rsx! {
            section { class: "route-denied", role: "alert", "data-reason": "invalid_process_cursor",
                h1 { {copy(locale, "Process cursor invalid", "cursor ของกระบวนการไม่ถูกต้อง")} }
                p { {copy(locale,
                    "Reason: the page cursor is malformed or outside its bounded owner window.",
                    "เหตุผล: cursor ของหน้ามีรูปแบบไม่ถูกต้องหรืออยู่นอกขอบเขตข้อมูลของบริการเจ้าของ",
                )} }
                KitLink { href: "/processes", {copy(locale, "Return to first page", "กลับไปหน้าแรก")} }
            }
        };
    }
    let request = crate::process_delivery::AuthorizedProcessRequest {
        cursor: cursor.clone(),
        page_size: 24,
        locale: locale.language_tag().into(),
        consistency: crate::process_delivery::ProcessConsistency::Eventual,
    };
    let mut resource = use_server_future(move || {
        let request = request.clone();
        async move {
            match crate::process_delivery::list_authorized_processes(request).await {
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
        .expect("resolved authorized process future has a value");
    let title = format!("{} · ERPX", copy(locale, "Processes", "กระบวนการ"));

    rsx! {
        document::Title { "{title}" }
        article { class: "workspace-route authorized-process-page",
            nav { class: "route-breadcrumb", aria_label: copy(locale, "Breadcrumb", "เส้นทางนำทาง"),
                KitLink { href: "/modules", {copy(locale, "Modules", "โมดูล")} }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", {copy(locale, "Processes", "กระบวนการ")} }
            }
            header {
                p { class: "eyebrow", {copy(locale, "AUTHORIZED PROCESS SHELL", "ศูนย์กระบวนการตามสิทธิ์")} }
                h1 { {copy(locale, "Lifecycle and retry state", "สถานะ lifecycle และการลองใหม่")} }
                p { {copy(locale,
                    "Follow lifecycle, failure, and retry evidence directly from authorized process owners. Every item links to its canonical owner record.",
                    "ติดตามหลักฐาน lifecycle ความล้มเหลว และการลองใหม่โดยตรงจากบริการเจ้าของกระบวนการที่ได้รับอนุญาต แต่ละรายการเชื่อมกลับไปยังระเบียนหลัก",
                )} }
            }
            match page_state {
                Err(error) => {
                    let (error_title, reason) = process_error_copy(&error, locale);
                    rsx! {
                        section { class: "route-denied", role: "alert", "data-reason": "authorized_process_query_failed",
                            h2 { "{error_title}" }
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
                                KitLink { href: "/auth/accounts", {copy(locale, "Review access", "ตรวจสอบสิทธิ์")} }
                            }
                        }
                    }
                },
                Ok(page) => {
                    let unavailable = page.availability == erpx_ui_core::ReadModelAvailability::Unavailable;
                    let next_href = page.next_cursor.as_ref().map(|value| format!("/processes?cursor={value}"));
                    rsx! {
                        section { class: "process-evidence", role: "status", "data-availability": "{work_availability_label(page.availability, Locale::En).to_ascii_lowercase()}",
                            div {
                                strong { {work_availability_label(page.availability, locale)} }
                                p {
                                    {copy(locale, "Measured at", "วัดเมื่อ")}
                                    " {page.measured_at} · "
                                    {copy(locale, "Scope", "ขอบเขต")}
                                    ": {page.scope} · "
                                    {process_freshness_label(page.freshness, locale)}
                                }
                            }
                            small {
                                {copy(locale, "Reference", "รหัสอ้างอิง")}
                                ": {page.correlation_id}"
                                if let Some(version) = page.authorization_context_version {
                                    " · auth-context v{version}"
                                }
                            }
                        }
                        if unavailable {
                            section { class: "process-unavailable", role: "alert",
                                h2 { {copy(locale, "No authoritative process owner is available", "ไม่มีบริการเจ้าของกระบวนการที่เชื่อถือได้พร้อมใช้งาน")} }
                                p { {copy(locale,
                                    "The shell will not report success, completion, or a synthetic zero while every authorized owner is unavailable.",
                                    "ศูนย์กระบวนการจะไม่รายงานความสำเร็จ การเสร็จสิ้น หรือค่าศูนย์ที่สร้างขึ้นเอง ในขณะที่บริการเจ้าของที่ได้รับอนุญาตทั้งหมดไม่พร้อมใช้งาน",
                                )} }
                                if let Some(reason) = page.reason_code.as_ref() {
                                    small { {copy(locale, "Reason", "เหตุผล")} ": {reason}" }
                                }
                            }
                        } else if page.items.is_empty() {
                            section { class: "empty-state", role: "status",
                                span { class: "empty-state__icon", aria_hidden: "true", "◇" }
                                div { class: "empty-state__copy",
                                    h2 { {copy(locale, "Authoritative process list is empty", "รายการกระบวนการว่างตามข้อมูลที่เชื่อถือได้")} }
                                    p { {copy(locale,
                                        "Every responding authorized owner returned no lifecycle records in this bounded view; no sample processes were created.",
                                        "บริการเจ้าของที่ได้รับอนุญาตและตอบกลับทั้งหมดไม่พบระเบียน lifecycle ในมุมมองที่มีขอบเขตนี้ ระบบไม่ได้สร้างกระบวนการตัวอย่าง",
                                    )} }
                                }
                            }
                        } else {
                            ul { class: "process-list", aria_label: copy(locale, "Authorized process lifecycles", "lifecycle ของกระบวนการตามสิทธิ์"),
                                for item in page.items.iter() {
                                    li { class: "process-card", key: "{item.key}", "data-stage": "{process_stage_key(item.stage)}",
                                        div { class: "process-card__main",
                                            div { class: "process-card__badges",
                                                span { class: "process-kind", {copy(locale, "Holding backup", "การสำรองข้อมูลโฮลดิ้ง")} }
                                                span { class: "record-status", {process_stage_label(item.stage, locale)} }
                                                if item.retryable {
                                                    span { class: "process-retry", {copy(locale, "Retryable", "ลองใหม่ได้")} }
                                                }
                                            }
                                            h2 { {process_kind_label(item.kind, locale)} }
                                            dl { class: "process-card__facts",
                                                div {
                                                    dt { {copy(locale, "Owner", "เจ้าของข้อมูล")} }
                                                    dd { "{item.owner}" }
                                                }
                                                div {
                                                    dt { {copy(locale, "Attempts", "จำนวนครั้ง")} }
                                                    dd { "{item.attempts}/{item.max_attempts}" }
                                                }
                                                div {
                                                    dt { {copy(locale, "Updated", "อัปเดตเมื่อ")} }
                                                    dd { "{item.updated_at}" }
                                                }
                                                if let Some(next_attempt_at) = item.next_attempt_at.as_ref() {
                                                    div {
                                                        dt { {copy(locale, "Next attempt", "ลองครั้งถัดไป")} }
                                                        dd { "{next_attempt_at}" }
                                                    }
                                                }
                                                if let Some(completed_at) = item.completed_at.as_ref() {
                                                    div {
                                                        dt { {copy(locale, "Completed", "เสร็จเมื่อ")} }
                                                        dd { "{completed_at}" }
                                                    }
                                                }
                                                if let Some(failure_category) = item.failure_category.as_ref() {
                                                    div {
                                                        dt { {copy(locale, "Failure", "ความล้มเหลว")} }
                                                        dd { "{failure_category}" }
                                                    }
                                                }
                                            }
                                        }
                                        KitLink {
                                            class: "route-action route-action--primary",
                                            href: "{item.target_path}",
                                            {copy(locale, "Open owner record", "เปิดระเบียนเจ้าของข้อมูล")}
                                        }
                                    }
                                }
                            }
                        }
                        if next_href.is_some() || cursor.is_some() {
                            div { class: "route-actions process-pagination",
                                if let Some(next_href) = next_href.as_ref() {
                                    KitLink { href: "{next_href}", {copy(locale, "Next page", "หน้าถัดไป")} }
                                }
                                KitLink { href: "/processes", {copy(locale, "First page", "หน้าแรก")} }
                            }
                        }
                        section { class: "process-contributors", aria_label: copy(locale, "Process owner status", "สถานะบริการเจ้าของกระบวนการ"),
                            header {
                                h2 { {copy(locale, "Authoritative process owners", "บริการเจ้าของกระบวนการที่เชื่อถือได้")} }
                                p { {copy(locale,
                                    "Unavailable owners remain visible as degraded evidence and are never converted into success or zero.",
                                    "บริการเจ้าของที่ไม่พร้อมใช้จะแสดงเป็นหลักฐานสถานะผิดปกติ และไม่ถูกแปลงเป็นความสำเร็จหรือค่าศูนย์",
                                )} }
                            }
                            div { class: "process-contributor-grid",
                                for source in page.contributions.iter() {
                                    article { class: "process-contributor", "data-availability": "{work_availability_label(source.availability, Locale::En).to_ascii_lowercase()}",
                                        div {
                                            strong { {copy(locale, "Holding backup jobs", "งานสำรองข้อมูลโฮลดิ้ง")} }
                                            span { {work_availability_label(source.availability, locale)} }
                                        }
                                        p { {process_contribution_message(source.availability, locale)} }
                                        small {
                                            {process_freshness_label(source.freshness, locale)}
                                            if source.availability != erpx_ui_core::ReadModelAvailability::Unavailable {
                                                " · "
                                                {copy(locale, "Records", "ระเบียน")}
                                                ": {source.item_count}"
                                            }
                                        }
                                        if let Some(reason_code) = source.reason_code.as_ref() {
                                            small { {copy(locale, "Reason", "เหตุผล")} ": {reason_code}" }
                                        }
                                    }
                                }
                            }
                        }
                        p { class: "process-redaction-note",
                            {copy(locale,
                                "Access tokens, backup references, restore targets, database profiles, and owner error details are redacted by contract.",
                                "access token ข้อมูลอ้างอิงการสำรอง เป้าหมายการกู้คืน โปรไฟล์ฐานข้อมูล และรายละเอียดข้อผิดพลาดภายในถูกปกปิดตาม contract",
                            )}
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn Processes(_cursor: Option<String>) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    rsx! {
        section { class: "route-denied", role: "alert", "data-reason": "native_process_transport_not_ported",
            h1 { {copy(locale, "Processes unavailable on Native", "กระบวนการยังไม่พร้อมบน Native")} }
            p { {copy(locale,
                "Reason: the typed Native Tonic process-owner adapter has not been certified, so this capability fails closed.",
                "เหตุผล: typed Native Tonic adapter สำหรับบริการเจ้าของกระบวนการยังไม่ผ่านการรับรอง ความสามารถนี้จึงปิดแบบ fail closed",
            )} }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn insight_metric_label(
    kind: crate::insight_delivery::InsightMetricKind,
    locale: Locale,
) -> &'static str {
    match kind {
        crate::insight_delivery::InsightMetricKind::ActiveWork => {
            copy(locale, "Active authorized work", "งานตามสิทธิ์ที่กำลังดำเนินการ")
        }
        crate::insight_delivery::InsightMetricKind::Approvals => {
            copy(locale, "Approvals requiring action", "การอนุมัติที่ต้องดำเนินการ")
        }
        crate::insight_delivery::InsightMetricKind::Exceptions => copy(
            locale,
            "Open exceptions and incidents",
            "ข้อยกเว้นและเหตุขัดข้องที่เปิดอยู่",
        ),
        crate::insight_delivery::InsightMetricKind::Overdue => {
            copy(locale, "Items beyond SLA", "รายการที่เกิน SLA")
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn insight_metric_description(
    kind: crate::insight_delivery::InsightMetricKind,
    locale: Locale,
) -> &'static str {
    match kind {
        crate::insight_delivery::InsightMetricKind::ActiveWork => copy(
            locale,
            "Confirmed active records from responding owner contributors.",
            "ระเบียนที่กำลังดำเนินการและยืนยันแล้วจากบริการเจ้าของข้อมูลที่ตอบกลับ",
        ),
        crate::insight_delivery::InsightMetricKind::Approvals => copy(
            locale,
            "Confirmed approval records that still require action.",
            "ระเบียนการอนุมัติที่ยืนยันแล้วและยังต้องดำเนินการ",
        ),
        crate::insight_delivery::InsightMetricKind::Exceptions => copy(
            locale,
            "Confirmed exception and incident records that remain active.",
            "ระเบียนข้อยกเว้นและเหตุขัดข้องที่ยืนยันแล้วและยังดำเนินอยู่",
        ),
        crate::insight_delivery::InsightMetricKind::Overdue => copy(
            locale,
            "Confirmed records whose owner-provided due time is beyond SLA.",
            "ระเบียนที่ยืนยันแล้วซึ่งเวลาครบกำหนดจากเจ้าของข้อมูลเกิน SLA",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn insight_source_label(domain: erpx_ui_core::WorkDomain, locale: Locale) -> &'static str {
    match domain {
        erpx_ui_core::WorkDomain::Operations => {
            copy(locale, "Operations exceptions", "ข้อยกเว้นฝ่ายปฏิบัติการ")
        }
        erpx_ui_core::WorkDomain::Support => copy(locale, "Platform support", "ฝ่ายสนับสนุนแพลตฟอร์ม"),
        erpx_ui_core::WorkDomain::Finance => {
            copy(locale, "Finance close reviews", "การตรวจสอบปิดบัญชี")
        }
        erpx_ui_core::WorkDomain::People => copy(locale, "People approvals", "การอนุมัติด้านบุคลากร"),
        erpx_ui_core::WorkDomain::Maintenance => {
            copy(locale, "Maintenance work orders", "ใบงานบำรุงรักษา")
        }
        erpx_ui_core::WorkDomain::Procurement => {
            copy(locale, "Procurement approvals", "การอนุมัติจัดซื้อ")
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn insight_source_message(
    availability: erpx_ui_core::ReadModelAvailability,
    locale: Locale,
) -> &'static str {
    match availability {
        erpx_ui_core::ReadModelAvailability::Available => copy(
            locale,
            "This owner contributed authoritative records to the metrics.",
            "บริการเจ้าของนี้ส่งระเบียนที่เชื่อถือได้เข้าสู่การคำนวณ metric",
        ),
        erpx_ui_core::ReadModelAvailability::Empty => copy(
            locale,
            "This owner authoritatively confirmed an empty bounded window.",
            "บริการเจ้าของนี้ยืนยันว่าหน้าต่างข้อมูลที่มีขอบเขตว่างอยู่",
        ),
        erpx_ui_core::ReadModelAvailability::Partial => copy(
            locale,
            "Only the owner-confirmed bounded window contributes to the metrics.",
            "เฉพาะหน้าต่างข้อมูลที่เจ้าของยืนยันและมีขอบเขตเท่านั้นที่ใช้คำนวณ metric",
        ),
        erpx_ui_core::ReadModelAvailability::Unavailable => copy(
            locale,
            "This owner contributed no value while its projection was unavailable.",
            "บริการเจ้าของนี้ไม่ส่งค่าเข้าสู่ metric ระหว่างที่ projection ไม่พร้อมใช้งาน",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn insight_error_copy(
    error: &erpx_delivery_contract::DeliveryError,
    locale: Locale,
) -> (&'static str, &'static str) {
    use erpx_delivery_contract::DeliveryError;
    match (error, locale) {
        (DeliveryError::Unauthenticated(_), Locale::En) => (
            "Insight session unavailable",
            "Reason: the authenticated web session is no longer valid. Sign in again.",
        ),
        (DeliveryError::Unauthenticated(_), Locale::Th) => (
            "เซสชันข้อมูลเชิงลึกไม่พร้อมใช้งาน",
            "เหตุผล: เซสชันเว็บที่ยืนยันตัวตนแล้วไม่ถูกต้องอีกต่อไป โปรดเข้าสู่ระบบใหม่",
        ),
        (DeliveryError::Forbidden(_), Locale::En) => (
            "Insight access denied",
            "Reason: the active role or authority scope cannot read these owner metrics.",
        ),
        (DeliveryError::Forbidden(_), Locale::Th) => (
            "ไม่มีสิทธิ์เข้าถึงข้อมูลเชิงลึก",
            "เหตุผล: บทบาทหรือขอบเขตสิทธิ์ที่ใช้งานอยู่ไม่สามารถอ่าน metric จากบริการเจ้าของข้อมูลเหล่านี้ได้",
        ),
        (DeliveryError::Validation(_), Locale::En) => (
            "Insight request invalid",
            "Reason: the bounded insight request was not accepted.",
        ),
        (DeliveryError::Validation(_), Locale::Th) => (
            "คำขอข้อมูลเชิงลึกไม่ถูกต้อง",
            "เหตุผล: ระบบไม่ยอมรับคำขอข้อมูลเชิงลึกแบบมีขอบเขต",
        ),
        (DeliveryError::Unavailable(_), Locale::En) => (
            "Insight owners unavailable",
            "Reason: an authorized owner or the session authority service could not be reached.",
        ),
        (DeliveryError::Unavailable(_), Locale::Th) => (
            "บริการเจ้าของข้อมูลเชิงลึกไม่พร้อมใช้งาน",
            "เหตุผล: ไม่สามารถเชื่อมต่อบริการเจ้าของข้อมูลที่ได้รับอนุญาตหรือบริการสิทธิ์ของเซสชันได้",
        ),
        (_, Locale::En) => (
            "Insight request failed",
            "Reason: an owner returned invalid evidence or could not complete the query.",
        ),
        (_, Locale::Th) => (
            "คำขอข้อมูลเชิงลึกไม่สำเร็จ",
            "เหตุผล: บริการเจ้าของข้อมูลส่งหลักฐานที่ไม่ถูกต้องหรือไม่สามารถทำคำขอให้เสร็จได้",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn Insights() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = crate::insight_delivery::AuthorizedInsightRequest {
        cursor: None,
        page_size: 4,
        locale: locale.language_tag().into(),
        consistency: crate::insight_delivery::InsightConsistency::Eventual,
    };
    let mut resource = use_server_future(move || {
        let request = request.clone();
        async move {
            match crate::insight_delivery::list_authorized_insights(request).await {
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
        .expect("resolved authorized insight future has a value");
    let title = format!("{} · ERPX", copy(locale, "Insights", "ข้อมูลเชิงลึก"));

    rsx! {
        document::Title { "{title}" }
        article { class: "workspace-route authorized-insight-page",
            nav { class: "route-breadcrumb", aria_label: copy(locale, "Breadcrumb", "เส้นทางนำทาง"),
                KitLink { href: "/modules", {copy(locale, "Modules", "โมดูล")} }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", {copy(locale, "Insights", "ข้อมูลเชิงลึก")} }
            }
            header {
                p { class: "eyebrow", {copy(locale, "AUTHORIZED INSIGHT SHELL", "ศูนย์ข้อมูลเชิงลึกตามสิทธิ์")} }
                h1 { {copy(locale, "Trusted operational signals", "สัญญาณการดำเนินงานที่เชื่อถือได้")} }
                p { {copy(locale,
                    "Review metrics derived from authorized owner records. Source, measurement time, freshness, and degraded contributors remain visible with every value.",
                    "ตรวจสอบ metric ที่คำนวณจากระเบียนของบริการเจ้าของข้อมูลตามสิทธิ์ โดยแสดงแหล่งข้อมูล เวลาที่วัด ความสด และ contributor ที่ผิดปกติไว้กับทุกค่า",
                )} }
            }
            match page_state {
                Err(error) => {
                    let (error_title, reason) = insight_error_copy(&error, locale);
                    rsx! {
                        section { class: "route-denied", role: "alert", "data-reason": "authorized_insight_query_failed",
                            h2 { "{error_title}" }
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
                                KitLink { href: "/auth/accounts", {copy(locale, "Review access", "ตรวจสอบสิทธิ์")} }
                            }
                        }
                    }
                },
                Ok(page) => {
                    let unavailable = page.availability == erpx_ui_core::ReadModelAvailability::Unavailable;
                    let partial = page.availability == erpx_ui_core::ReadModelAvailability::Partial;
                    rsx! {
                        section { class: "insight-evidence", role: "status", "data-availability": "{work_availability_label(page.availability, Locale::En).to_ascii_lowercase()}",
                            div {
                                strong { {work_availability_label(page.availability, locale)} }
                                p {
                                    {copy(locale, "Measured at", "วัดเมื่อ")}
                                    " {page.measured_at} · "
                                    {copy(locale, "Scope", "ขอบเขต")}
                                    ": {page.scope} · "
                                    {process_freshness_label(page.freshness, locale)}
                                }
                            }
                            small {
                                {copy(locale, "Reference", "รหัสอ้างอิง")}
                                ": {page.correlation_id}"
                                if let Some(version) = page.authorization_context_version {
                                    " · auth-context v{version}"
                                }
                            }
                        }
                        if unavailable {
                            section { class: "insight-unavailable", role: "alert",
                                h2 { {copy(locale, "No authoritative insight source is available", "ไม่มีแหล่งข้อมูลเชิงลึกที่เชื่อถือได้พร้อมใช้งาน")} }
                                p { {copy(locale,
                                    "No metric or synthetic zero is shown while every authorized owner is unavailable.",
                                    "ระบบจะไม่แสดง metric หรือค่าศูนย์ที่สร้างขึ้นเอง ในขณะที่บริการเจ้าของที่ได้รับอนุญาตทั้งหมดไม่พร้อมใช้งาน",
                                )} }
                                if let Some(reason) = page.reason_code.as_ref() {
                                    small { {copy(locale, "Reason", "เหตุผล")} ": {reason}" }
                                }
                            }
                        } else {
                            if partial {
                                section { class: "insight-partial", role: "alert",
                                    strong { {copy(locale, "Partial owner coverage", "ข้อมูลจากบริการเจ้าของไม่ครบถ้วน")} }
                                    p { {copy(locale,
                                        "Values include only records confirmed by responding owners; unavailable contributors are excluded rather than counted as zero.",
                                        "ค่ารวมเฉพาะระเบียนที่บริการเจ้าของซึ่งตอบกลับยืนยันแล้ว contributor ที่ไม่พร้อมใช้จะถูกตัดออกแทนการนับเป็นศูนย์",
                                    )} }
                                    if let Some(reason) = page.reason_code.as_ref() {
                                        small { {copy(locale, "Reason", "เหตุผล")} ": {reason}" }
                                    }
                                }
                            }
                            section { class: "insight-metric-grid", aria_label: copy(locale, "Authorized operational metrics", "metric การดำเนินงานตามสิทธิ์"),
                                for metric in page.metrics.iter() {
                                    article { class: "insight-metric", key: "{metric.key}",
                                        div { class: "insight-metric__heading",
                                            p { {insight_metric_label(metric.kind, locale)} }
                                            span { {process_freshness_label(metric.freshness, locale)} }
                                        }
                                        strong { class: "insight-metric__value", "{metric.value}" }
                                        p { class: "insight-metric__description", {insight_metric_description(metric.kind, locale)} }
                                        dl { class: "insight-metric__evidence",
                                            div {
                                                dt { {copy(locale, "Source", "แหล่งข้อมูล")} }
                                                dd { {copy(locale, "Authorized work owners", "บริการเจ้าของงานตามสิทธิ์")} }
                                            }
                                            div {
                                                dt { {copy(locale, "Measured", "วัดเมื่อ")} }
                                                dd { "{metric.measured_at}" }
                                            }
                                            div {
                                                dt { {copy(locale, "Responding owners", "บริการเจ้าของที่ตอบกลับ")} }
                                                dd { "{metric.source_count}" }
                                            }
                                        }
                                        KitLink { href: "{metric.target_path}", {copy(locale, "Open source records", "เปิดระเบียนต้นทาง")} }
                                    }
                                }
                            }
                        }
                        section { class: "insight-sources", aria_label: copy(locale, "Insight source status", "สถานะแหล่งข้อมูลเชิงลึก"),
                            header {
                                h2 { {copy(locale, "Authoritative contributors", "contributor ที่เชื่อถือได้")} }
                                p { {copy(locale,
                                    "Each contributor retains its own availability, freshness, measurement time, and projection evidence.",
                                    "contributor แต่ละรายคงสถานะพร้อมใช้ ความสด เวลาที่วัด และหลักฐาน projection ของตนเอง",
                                )} }
                            }
                            if page.sources.is_empty() {
                                p { class: "insight-source-empty", {copy(locale,
                                    "No insight contributor is authorized for the active navigation scope.",
                                    "ไม่มี contributor ข้อมูลเชิงลึกที่ได้รับอนุญาตในขอบเขต navigation ปัจจุบัน",
                                )} }
                            } else {
                                div { class: "insight-source-grid",
                                    for source in page.sources.iter() {
                                        article { class: "insight-source", "data-availability": "{work_availability_label(source.availability, Locale::En).to_ascii_lowercase()}",
                                            div {
                                                strong { {insight_source_label(source.domain, locale)} }
                                                span { {work_availability_label(source.availability, locale)} }
                                            }
                                            p { {insight_source_message(source.availability, locale)} }
                                            small {
                                                {process_freshness_label(source.freshness, locale)}
                                                " · "
                                                {copy(locale, "Measured", "วัดเมื่อ")}
                                                ": {source.measured_at}"
                                            }
                                            small {
                                                {copy(locale, "Projection evidence", "หลักฐาน projection")}
                                                ": "
                                                if source.has_projection_evidence {
                                                    {copy(locale, "verified", "ตรวจสอบแล้ว")}
                                                } else {
                                                    {copy(locale, "not available", "ไม่พร้อมใช้")}
                                                }
                                            }
                                            if source.availability != erpx_ui_core::ReadModelAvailability::Unavailable {
                                                small { {copy(locale, "Confirmed records", "ระเบียนที่ยืนยันแล้ว")} ": {source.item_count}" }
                                            }
                                            if let Some(reason) = source.reason_code.as_ref() {
                                                small { {copy(locale, "Reason", "เหตุผล")} ": {reason}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        p { class: "insight-redaction-note",
                            {copy(locale,
                                "Access tokens, customer identities, owner payloads, and underlying record details are redacted by contract.",
                                "access token ตัวตนลูกค้า payload ภายในของบริการเจ้าของ และรายละเอียดระเบียนต้นทางถูกปกปิดตาม contract",
                            )}
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn Insights() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    rsx! {
        section { class: "route-denied", role: "alert", "data-reason": "native_insight_transport_not_ported",
            h1 { {copy(locale, "Insights unavailable on Native", "ข้อมูลเชิงลึกยังไม่พร้อมบน Native")} }
            p { {copy(locale,
                "Reason: the typed Native Tonic insight-owner adapter has not been certified, so this capability fails closed.",
                "เหตุผล: typed Native Tonic adapter สำหรับบริการเจ้าของข้อมูลเชิงลึกยังไม่ผ่านการรับรอง ความสามารถนี้จึงปิดแบบ fail closed",
            )} }
        }
    }
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
fn PlatformHoldingCanonical(holding_id: RouteUuid, history_cursor: Option<String>) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = feature_detail_request(holding_id, locale);
    let history_request = crate::feature_delivery::HoldingHistoryRequest {
        holding_id: holding_id.get(),
        cursor: history_cursor,
        page_size: 25,
        locale: locale.language_tag().to_owned(),
        consistency: crate::feature_delivery::FeatureConsistency::Eventual,
    };
    let resource = use_server_future(move || {
        let request = request.clone();
        let history_request = history_request.clone();
        async move {
            let (holding, evidence) = crate::feature_delivery::get_platform_holding(request)
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            let history =
                match crate::feature_delivery::list_platform_holding_history(history_request).await
                {
                    Ok(result) => result,
                    Err(_) => Err(erpx_delivery_contract::DeliveryError::unavailable(Some(
                        500,
                    ))),
                };
            Ok::<_, String>((holding, evidence, history))
        }
    })?;
    let (holding, evidence, history_result) = resource
        .read()
        .clone()
        .expect("resolved platform holding future has a value")
        .map_err(feature_error)?;
    let history = match history_result {
        Ok(page) => HoldingControlHistoryState::Ready { page },
        Err(erpx_delivery_contract::DeliveryError::Forbidden(meta))
        | Err(erpx_delivery_contract::DeliveryError::Unauthenticated(meta)) => {
            HoldingControlHistoryState::Forbidden {
                reason: copy(
                    locale,
                    "The owner service did not authorize this audit-history read.",
                    "บริการเจ้าของข้อมูลไม่อนุญาตให้อ่านประวัติการตรวจสอบนี้",
                )
                .into(),
                correlation_id: meta.correlation_id.to_string(),
            }
        }
        Err(error) => HoldingControlHistoryState::Unavailable {
            reason: copy(
                locale,
                "The authoritative holding control history is temporarily unavailable. No events were synthesized.",
                "ประวัติการควบคุมโฮลดิ้งที่เชื่อถือได้ไม่พร้อมใช้งานชั่วคราว และระบบไม่ได้สร้างเหตุการณ์จำลอง",
            )
            .into(),
            correlation_id: error.meta().correlation_id.to_string(),
        },
    };
    let record = CatalogRecord::try_from_segments("platform", "holding", &holding_id.to_string())
        .map_err(|error| RenderError::from(std::io::Error::other(error.to_string())))?;
    rsx! {
        RecordDetailHost {
            record,
            owner_content: rsx! {
                PlatformHoldingDetailSurface { holding, evidence, history }
            },
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn PlatformHoldingCanonical(holding_id: RouteUuid, _history_cursor: Option<String>) -> Element {
    let record = CatalogRecord::try_from_segments("platform", "holding", &holding_id.to_string())
        .map_err(|error| RenderError::from(std::io::Error::other(error.to_string())))?;
    rsx! {
        RecordDetailHost {
            record,
            owner_content: rsx! { FeatureClosed { name: "Platform holding detail" } },
        }
    }
}

/// Compatibility-only route for the former plural Platform Holding URL. The
/// replacement prevents the alias from becoming a second record identity.
#[component]
fn PlatformHolding(holding_id: RouteUuid, history_cursor: Option<String>) -> Element {
    let navigator = use_navigator();
    let target = AppRoute::PlatformHoldingCanonical {
        holding_id,
        history_cursor,
    };
    let href = route_path(&target);
    use_effect(move || {
        navigator.replace(target.clone());
    });
    rsx! {
        section { class: "route-loading", role: "status",
            p { "Redirecting to the canonical Platform Holding route…" }
            KitLink { href, "Continue" }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn PlatformSupport() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let state = SupportTicketListState::Offline {
        data: None,
        metadata: None,
        reason: copy(
            locale,
            "The typed platform-support route is available, but its dedicated list delivery is not connected yet (support_delivery_not_connected).",
            "เส้นทาง Platform Support แบบ typed พร้อมใช้งานแล้ว แต่ delivery สำหรับรายการยังไม่ได้เชื่อมต่อ (support_delivery_not_connected)",
        )
        .into(),
    };
    rsx! {
        document::Title { "{SupportRoute::PlatformSupport.label(locale)} · ERPX" }
        SupportTicketListSurface { state }
    }
}

#[cfg(feature = "native")]
#[component]
fn PlatformSupport() -> Element {
    rsx! { FeatureClosed { name: "Platform support (native delivery not connected)" } }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn PlatformSupportTicket(id: RouteUuid) -> Element {
    let _ = id;
    let locale = erpx_ui_core::use_locale().current();
    let state = SupportState::Offline {
        data: None,
        metadata: None,
        reason: copy(
            locale,
            "The ticket owner route is canonical, but its dedicated detail delivery is not connected yet (support_detail_delivery_not_connected).",
            "เส้นทางเจ้าของทิกเก็ตเป็น canonical แล้ว แต่ delivery สำหรับรายละเอียดเฉพาะยังไม่ได้เชื่อมต่อ (support_detail_delivery_not_connected)",
        )
        .into(),
    };
    rsx! {
        document::Title { "{SupportRoute::PlatformSupportDetail.label(locale)} · ERPX" }
        SupportSurface { route: SupportRoute::PlatformSupportDetail, state }
    }
}

#[cfg(feature = "native")]
#[component]
fn PlatformSupportTicket(id: RouteUuid) -> Element {
    let _ = id;
    rsx! { FeatureClosed { name: "Platform support detail (native delivery not connected)" } }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn PlatformIncidents() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let state = IncidentListState::Offline {
        data: None,
        metadata: None,
        reason: copy(
            locale,
            "The typed incident route is available, but its dedicated list delivery is not connected yet (incident_delivery_not_connected).",
            "เส้นทางเหตุขัดข้องแบบ typed พร้อมใช้งานแล้ว แต่ delivery สำหรับรายการยังไม่ได้เชื่อมต่อ (incident_delivery_not_connected)",
        )
        .into(),
    };
    rsx! {
        document::Title { "{SupportRoute::PlatformIncidents.label(locale)} · ERPX" }
        IncidentListSurface { state }
    }
}

#[cfg(feature = "native")]
#[component]
fn PlatformIncidents() -> Element {
    rsx! { FeatureClosed { name: "Platform incidents (native delivery not connected)" } }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn PlatformIncident(id: RouteUuid) -> Element {
    let _ = id;
    let locale = erpx_ui_core::use_locale().current();
    let state = IncidentDetailState::Offline {
        data: None,
        metadata: None,
        reason: copy(
            locale,
            "The incident owner route is canonical, but its dedicated detail delivery is not connected yet (incident_detail_delivery_not_connected).",
            "เส้นทางเจ้าของเหตุขัดข้องเป็น canonical แล้ว แต่ delivery สำหรับรายละเอียดเฉพาะยังไม่ได้เชื่อมต่อ (incident_detail_delivery_not_connected)",
        )
        .into(),
    };
    rsx! {
        document::Title { "{SupportRoute::PlatformIncidentDetail.label(locale)} · ERPX" }
        IncidentDetailSurface { state }
    }
}

#[cfg(feature = "native")]
#[component]
fn PlatformIncident(id: RouteUuid) -> Element {
    let _ = id;
    rsx! { FeatureClosed { name: "Platform incident detail (native delivery not connected)" } }
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
    let record = CatalogRecord::try_from_segments("identity", kind.slug(), &id.to_string())
        .map_err(|error| RenderError::from(std::io::Error::other(error.to_string())))?;
    rsx! {
        document::Title { "{page_title} · ERPX" }
        RecordDetailHost {
            record,
            owner_content: rsx! {
                IdentityAccessDetailSurface {
                    detail,
                    on_principal_command,
                    command_feedback: command_feedback(),
                    command_busy: command_busy(),
                }
            },
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn IdentityDetail(kind: IdentityRecordKind, id: RouteUuid) -> Element {
    let record = CatalogRecord::try_from_segments("identity", kind.slug(), &id.to_string())
        .map_err(|error| RenderError::from(std::io::Error::other(error.to_string())))?;
    rsx! {
        RecordDetailHost {
            record,
            owner_content: rsx! { FeatureClosed { name: "Identity & access detail" } },
        }
    }
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
    let catalog_record =
        CatalogRecord::try_from_segments(workspace.slug(), resource.as_str(), &id.to_string());
    match catalog_record {
        Ok(record) => {
            let owner_content = record_owner_content(&record, workspace, resource, id);
            rsx! { RecordDetailHost { record, owner_content } }
        }
        Err(_) => rsx! {
            CatalogRouteUnavailable {
                title: format!("{workspace} · {resource}"),
                reason: "The route is not a current or future catalog record family.".to_owned(),
            }
        },
    }
}

fn record_owner_content(
    record: &CatalogRecord,
    workspace: Workspace,
    resource: ResourceName,
    id: RouteUuid,
) -> Element {
    match record.record().record_type().as_str() {
        "procurement.requisition" | "procurement.purchase-order" | "procurement.receipt" => rsx! {
            Phase5ProcurementOwnerDetail { record: record.clone() }
        },
        "finance.invoice" => rsx! {
            Phase5CustomerInvoiceOwnerDetail { workspace, resource, id }
        },
        "finance.vendor-invoice" | "finance.payment" => rsx! {
            Phase5FinanceOwnerDetail { record: record.clone() }
        },
        _ => rsx! {
            ProtectedRouteSurface { query: RouteQuery::Record { workspace, resource, id } }
        },
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn procurement_owner_error_state(
    error: erpx_console_feature_procurement::ProcurementOwnerGatewayError,
) -> erpx_console_feature_procurement::ProcurementOwnerDetailState {
    use erpx_console_feature_procurement::{
        ProcurementOwnerDetailState as State, ProcurementOwnerGatewayError as Error,
    };
    match error {
        Error::Denied => State::Denied,
        Error::Offline => State::Offline,
        Error::Conflict | Error::Stale | Error::Unavailable | Error::Invalid => {
            State::Unavailable { retryable: true }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn Phase5ProcurementOwnerDetail(record: CatalogRecord) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let requested_record = record.record().clone();
    let expected_record = requested_record.clone();
    let mut resource = use_server_future(move || {
        let requested_record = requested_record.clone();
        async move {
            crate::owner_detail_delivery::load_procurement_owner(requested_record)
                .await
                .unwrap_or(Err(
                    erpx_console_feature_procurement::ProcurementOwnerGatewayError::Offline,
                ))
        }
    })?;
    let state = resource
        .read()
        .clone()
        .expect("resolved procurement owner future has a value")
        .unwrap_or_else(procurement_owner_error_state);
    let mut mutation_busy = use_signal(|| false);
    let mut mutation_resource = resource;
    let on_mutate = EventHandler::new(move |mutation| {
        if *mutation_busy.peek() {
            return;
        }
        mutation_busy.set(true);
        spawn(async move {
            let _ = crate::owner_detail_delivery::mutate_procurement_owner(mutation).await;
            mutation_busy.set(false);
            mutation_resource.restart();
        });
    });
    let on_retry = EventHandler::new(move |_| resource.restart());
    rsx! {
        erpx_console_feature_procurement::ProcurementOwnerDetailSurface {
            expected_record,
            state,
            locale,
            on_mutate,
            on_retry,
            mutation_busy: mutation_busy(),
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn Phase5ProcurementOwnerDetail(record: CatalogRecord) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    rsx! {
        erpx_console_feature_procurement::ProcurementOwnerDetailSurface {
            expected_record: record.record().clone(),
            state: erpx_console_feature_procurement::ProcurementOwnerDetailState::Unavailable {
                retryable: true,
            },
            locale,
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn finance_owner_error_state(
    error: erpx_console_feature_finance::FinanceOwnerGatewayError,
) -> erpx_console_feature_finance::FinanceOwnerDetailState {
    use erpx_console_feature_finance::{
        FinanceOwnerDetailState as State, FinanceOwnerGatewayError as Error,
    };
    match error {
        Error::Denied => State::Denied,
        Error::Offline => State::Offline,
        Error::Conflict | Error::Stale | Error::Unavailable | Error::Invalid => {
            State::Unavailable { retryable: true }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn Phase5FinanceOwnerDetail(record: CatalogRecord) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let requested_record = record.record().clone();
    let expected_record = requested_record.clone();
    let mut resource = use_server_future(move || {
        let requested_record = requested_record.clone();
        async move {
            crate::owner_detail_delivery::load_finance_owner(requested_record)
                .await
                .unwrap_or(Err(
                    erpx_console_feature_finance::FinanceOwnerGatewayError::Offline,
                ))
        }
    })?;
    let state = resource
        .read()
        .clone()
        .expect("resolved finance owner future has a value")
        .unwrap_or_else(finance_owner_error_state);
    let mut mutation_busy = use_signal(|| false);
    let mut mutation_resource = resource;
    let on_mutate = EventHandler::new(move |mutation| {
        if *mutation_busy.peek() {
            return;
        }
        mutation_busy.set(true);
        spawn(async move {
            let _ = crate::owner_detail_delivery::mutate_finance_owner(mutation).await;
            mutation_busy.set(false);
            mutation_resource.restart();
        });
    });
    let on_retry = EventHandler::new(move |_| resource.restart());
    rsx! {
        erpx_console_feature_finance::FinanceOwnerDetailSurface {
            expected_record,
            state,
            locale,
            on_mutate,
            on_retry,
            mutation_busy: mutation_busy(),
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn Phase5FinanceOwnerDetail(record: CatalogRecord) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    rsx! {
        erpx_console_feature_finance::FinanceOwnerDetailSurface {
            expected_record: record.record().clone(),
            state: erpx_console_feature_finance::FinanceOwnerDetailState::Unavailable {
                retryable: true,
            },
            locale,
        }
    }
}

#[component]
fn Phase5CustomerInvoiceOwnerDetail(
    workspace: Workspace,
    resource: ResourceName,
    id: RouteUuid,
) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    rsx! {
        erpx_console_feature_finance::CustomerInvoiceArOwnerSlot {
            locale,
            owner_content: rsx! {
                ProtectedRouteSurface {
                    query: RouteQuery::Record { workspace, resource, id }
                }
            },
        }
    }
}

#[component]
fn CatalogRecordDetail(module: ResourceName, record: ResourceName, id: RouteUuid) -> Element {
    match CatalogRecord::try_from_segments(module.as_str(), record.as_str(), &id.to_string()) {
        Ok(catalog_record) => {
            let owner = format!("{}.{}", module.as_str(), record.as_str());
            rsx! {
                RecordDetailHost {
                    record: catalog_record,
                    owner_content: rsx! { OwnerDetailUnavailable { owner } },
                }
            }
        }
        Err(_) => rsx! {
            CatalogRouteUnavailable {
                title: format!("{} · {}", module.as_str(), record.as_str()),
                reason: "The route is not a current or future catalog record family.".to_owned(),
            }
        },
    }
}

#[component]
pub fn RecordDetailHost(record: CatalogRecord, owner_content: Element) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let descriptor = record.descriptor();
    let record_type = descriptor.id;
    let canonical_route = record.canonical_path();
    if !record.collaboration_available() {
        return rsx! {
            CatalogRouteUnavailable {
                title: humanize_resource(descriptor.record),
                reason: copy(
                    locale,
                    "This future catalog record is unavailable. Collaboration is not mounted until its owner context is current.",
                    "รายการในแค็ตตาล็อกอนาคตนี้ยังไม่พร้อม และจะไม่เปิดการทำงานร่วมกันจนกว่าบริบทเจ้าของจะเป็นปัจจุบัน",
                ).to_owned(),
            }
        };
    }

    // Catalog data selects the shell only. Protected collaboration identity
    // comes back from a fresh authenticated Item Directory resolution.
    let resolution =
        use_record_resolution(record.record().id().as_uuid(), canonical_route.clone())?;
    let references_state = use_record_references(resolution.clone(), canonical_route.clone())?;
    let activity_refresh = use_signal(|| 0_u64);
    let activity_state = use_record_activity(
        resolution.clone(),
        canonical_route.clone(),
        activity_refresh,
    )?;
    #[cfg(any(feature = "web", feature = "server"))]
    let on_review_suggestion = {
        let mut activity_refresh = activity_refresh;
        EventHandler::new(move |request: FieldSuggestionReviewRequest| {
            spawn(async move {
                if matches!(
                    crate::record_collaboration_delivery::mutate_record_collaboration(
                        CollaborationMutation::ReviewFieldSuggestion(request),
                    )
                    .await,
                    Ok(Ok(_))
                ) {
                    let generation = (*activity_refresh.peek()).saturating_add(1);
                    activity_refresh.set(generation);
                }
            });
        })
    };
    #[cfg(feature = "native")]
    let on_review_suggestion = {
        let gateway = use_context::<crate::native_delivery::NativeGrpcGateway>();
        let mut activity_refresh = activity_refresh;
        EventHandler::new(move |request: FieldSuggestionReviewRequest| {
            let gateway = gateway.clone();
            spawn(async move {
                if gateway
                    .mutate(CollaborationMutation::ReviewFieldSuggestion(request))
                    .await
                    .is_ok()
                {
                    let generation = (*activity_refresh.peek()).saturating_add(1);
                    activity_refresh.set(generation);
                }
            });
        })
    };
    let gate = resolution.activity_gate();
    let host_route = canonical_route.clone();
    let references_route = canonical_route.clone();
    let activity_route = canonical_route;
    rsx! {
        document::Title { "{record_type} · ERPX" }
        article {
            class: "record-detail-host",
            "data-record-family": record_type,
            "data-canonical-route": host_route,
            section { class: "record-detail-host__owner", aria_label: copy(locale, "Owner record detail", "รายละเอียดรายการจากเจ้าของ"),
                {owner_content}
            }
            section { class: "record-detail-host__collaboration", aria_label: copy(locale, "Record collaboration", "การทำงานร่วมกันของรายการ"),
                RecordReferences {
                    resolution: resolution.clone(),
                    current_route: references_route,
                    state: references_state,
                    locale,
                }
                AuthorizedRecordActivityRail {
                    gate,
                    current_route: activity_route,
                    state: activity_state,
                    locale,
                    on_review_suggestion: Some(on_review_suggestion),
                }
            }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn use_record_resolution(
    record_id: uuid::Uuid,
    current_route: String,
) -> Result<RecordResolutionState, RenderError> {
    let request = ResolveRecordRequest {
        record_id,
        current_route,
    };
    let resource = use_server_future(move || {
        let request = request.clone();
        async move {
            Ok::<RecordResolutionState, erpx_console_feature_ai_collaboration::GatewayError>(
                crate::record_collaboration_delivery::resolve_record_reference(request)
                    .await
                    .unwrap_or(RecordResolutionState::Degraded),
            )
        }
    })?;
    resource
        .read()
        .clone()
        .expect("resolved Item Directory future has a value")
        .map_err(|error| RenderError::from(std::io::Error::other(format!("{error:?}"))))
}

#[cfg(feature = "native")]
fn use_record_resolution(
    record_id: uuid::Uuid,
    current_route: String,
) -> Result<RecordResolutionState, RenderError> {
    let gateway = use_context::<crate::native_delivery::NativeGrpcGateway>();
    let resource = use_resource(move || {
        let gateway = gateway.clone();
        let current_route = current_route.clone();
        async move {
            gateway
                .resolve_record(ResolveRecordRequest {
                    record_id,
                    current_route,
                })
                .await
        }
    });
    Ok(resource
        .suspend()?
        .read()
        .clone()
        .unwrap_or(RecordResolutionState::Degraded))
}

fn references_gate_state(
    resolution: &RecordResolutionState,
    current_route: &str,
) -> RecordReferencesState {
    if resolution.authorized_record(current_route).is_some() {
        return RecordReferencesState::Loading;
    }
    match resolution {
        RecordResolutionState::Resolving => RecordReferencesState::Loading,
        RecordResolutionState::Neutral => RecordReferencesState::Denied,
        RecordResolutionState::Stale => RecordReferencesState::Stale,
        RecordResolutionState::Degraded => RecordReferencesState::Degraded { retryable: true },
        RecordResolutionState::Offline => RecordReferencesState::Offline,
        RecordResolutionState::Authorized { .. } => {
            RecordReferencesState::Degraded { retryable: false }
        }
    }
}

fn activity_gate_state(
    resolution: &RecordResolutionState,
    current_route: &str,
) -> ActivityRailState {
    if resolution.authorized_record(current_route).is_some() {
        return ActivityRailState::Loading;
    }
    match resolution {
        RecordResolutionState::Resolving => ActivityRailState::Loading,
        RecordResolutionState::Neutral => ActivityRailState::Denied,
        RecordResolutionState::Stale => ActivityRailState::Stale {
            threads: Vec::new(),
            reason: "The authorized record descriptor is stale".into(),
        },
        RecordResolutionState::Degraded => ActivityRailState::Degraded {
            threads: Vec::new(),
            reason: "Record activity authorization is unavailable".into(),
            retryable: true,
        },
        RecordResolutionState::Offline => ActivityRailState::Offline {
            cached_threads: Vec::new(),
        },
        RecordResolutionState::Authorized { .. } => ActivityRailState::Degraded {
            threads: Vec::new(),
            reason: "The canonical record route did not match authorization".into(),
            retryable: false,
        },
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn use_record_references(
    resolution: RecordResolutionState,
    current_route: String,
) -> Result<RecordReferencesState, RenderError> {
    let resource = use_server_future(move || {
        let resolution = resolution.clone();
        let current_route = current_route.clone();
        async move {
            let Some(anchor) = resolution.authorized_record(&current_route).cloned() else {
                return Ok::<
                    RecordReferencesState,
                    erpx_console_feature_ai_collaboration::GatewayError,
                >(references_gate_state(&resolution, &current_route));
            };
            Ok(
                crate::record_collaboration_delivery::load_record_references(ReferencesRequest {
                    anchor,
                })
                .await
                .unwrap_or(RecordReferencesState::Degraded { retryable: true }),
            )
        }
    })?;
    resource
        .read()
        .clone()
        .expect("resolved reference future has a value")
        .map_err(|error| RenderError::from(std::io::Error::other(format!("{error:?}"))))
}

#[cfg(feature = "native")]
fn use_record_references(
    resolution: RecordResolutionState,
    current_route: String,
) -> Result<RecordReferencesState, RenderError> {
    let gateway = use_context::<crate::native_delivery::NativeGrpcGateway>();
    let resource = use_resource(move || {
        let gateway = gateway.clone();
        let resolution = resolution.clone();
        let current_route = current_route.clone();
        async move {
            let Some(anchor) = resolution.authorized_record(&current_route).cloned() else {
                return Ok(references_gate_state(&resolution, &current_route));
            };
            gateway.load_references(ReferencesRequest { anchor }).await
        }
    });
    Ok(resource
        .suspend()?
        .read()
        .clone()
        .unwrap_or(RecordReferencesState::Degraded { retryable: true }))
}

#[cfg(any(feature = "web", feature = "server"))]
fn use_record_activity(
    resolution: RecordResolutionState,
    current_route: String,
    activity_refresh: Signal<u64>,
) -> Result<ActivityRailState, RenderError> {
    let resource = use_server_future(move || {
        let _generation = *activity_refresh.read();
        let resolution = resolution.clone();
        let current_route = current_route.clone();
        async move {
            let Some(anchor) = resolution.authorized_record(&current_route).cloned() else {
                return Ok::<ActivityRailState, erpx_console_feature_ai_collaboration::GatewayError>(
                    activity_gate_state(&resolution, &current_route),
                );
            };
            Ok(
                crate::record_collaboration_delivery::load_record_activity(ActivityRequest {
                    anchor,
                    cursor: None,
                })
                .await
                .unwrap_or_else(|_| {
                    erpx_console_feature_ai_collaboration::activity_error(
                        erpx_console_feature_ai_collaboration::GatewayError::Unavailable,
                    )
                }),
            )
        }
    })?;
    resource
        .read()
        .clone()
        .expect("resolved activity future has a value")
        .map_err(|error| RenderError::from(std::io::Error::other(format!("{error:?}"))))
}

#[cfg(feature = "native")]
fn use_record_activity(
    resolution: RecordResolutionState,
    current_route: String,
    activity_refresh: Signal<u64>,
) -> Result<ActivityRailState, RenderError> {
    let gateway = use_context::<crate::native_delivery::NativeGrpcGateway>();
    let resource = use_resource(move || {
        let _generation = *activity_refresh.read();
        let gateway = gateway.clone();
        let resolution = resolution.clone();
        let current_route = current_route.clone();
        async move {
            let Some(anchor) = resolution.authorized_record(&current_route).cloned() else {
                return Ok(activity_gate_state(&resolution, &current_route));
            };
            gateway
                .load_activity(ActivityRequest {
                    anchor,
                    cursor: None,
                })
                .await
        }
    });
    Ok(resource.suspend()?.read().clone().unwrap_or_else(|_| {
        erpx_console_feature_ai_collaboration::activity_error(
            erpx_console_feature_ai_collaboration::GatewayError::Unavailable,
        )
    }))
}

#[component]
fn OwnerDetailUnavailable(owner: String) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    rsx! {
        section { class: "record-owner-slot record-owner-slot--unavailable", role: "status",
            p { class: "eyebrow", {copy(locale, "OWNER DETAIL", "รายละเอียดจากเจ้าของ")} }
            h1 { {humanize_resource(&owner.replace('.', "-"))} }
            p { {copy(
                locale,
                "The canonical record shell is ready, but this owner's dedicated detail content is not connected yet.",
                "เชลล์รายการมาตรฐานพร้อมแล้ว แต่เนื้อหารายละเอียดเฉพาะของเจ้าของรายนี้ยังไม่ได้เชื่อมต่อ",
            )} }
        }
    }
}

#[component]
fn CatalogRouteUnavailable(title: String, reason: String) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let unavailable = copy(locale, "Unavailable", "ไม่พร้อมใช้งาน");
    rsx! {
        section { class: "route-denied", role: "alert", "data-state": "unavailable",
            h1 { "{title} · {unavailable}" }
            p { "{reason}" }
        }
    }
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
    let locale = erpx_ui_core::use_locale().current();
    let current_route = format!("/reference/item/{id}");
    let resolution = use_record_resolution(id.get(), current_route)?;
    let target = match &resolution {
        RecordResolutionState::Authorized {
            canonical_route, ..
        } => Some(canonical_route.clone()),
        _ => None,
    };
    let redirect_target = target.clone();
    let navigator = use_navigator();
    use_effect(move || {
        if let Some(target) = redirect_target.clone() {
            navigator.replace(target);
        }
    });

    let (state, title, message) = match resolution {
        RecordResolutionState::Resolving => (
            "resolving",
            copy(locale, "Resolving reference", "กำลังค้นหาการอ้างอิง"),
            copy(
                locale,
                "Checking current access and the live Item Directory…",
                "กำลังตรวจสอบสิทธิ์ปัจจุบันและไดเรกทอรีรายการแบบสด…",
            ),
        ),
        RecordResolutionState::Authorized { .. } => (
            "redirecting",
            copy(locale, "Opening record", "กำลังเปิดรายการ"),
            copy(
                locale,
                "The reference was authorized. Redirecting to its canonical route…",
                "การอ้างอิงได้รับอนุญาตแล้ว กำลังไปยังเส้นทางมาตรฐาน…",
            ),
        ),
        RecordResolutionState::Neutral => (
            "neutral",
            copy(locale, "Reference unavailable", "การอ้างอิงไม่พร้อมใช้งาน"),
            copy(
                locale,
                "The reference was not found or is not available to the active role and scope.",
                "ไม่พบการอ้างอิง หรือการอ้างอิงไม่พร้อมสำหรับบทบาทและขอบเขตปัจจุบัน",
            ),
        ),
        RecordResolutionState::Stale => (
            "stale",
            copy(locale, "Reference is updating", "การอ้างอิงกำลังอัปเดต"),
            copy(
                locale,
                "The directory projection is stale. No cached route or record payload was used.",
                "ไดเรกทอรีโพรเจกชันล้าสมัย ระบบไม่ได้ใช้เส้นทางหรือข้อมูลรายการจากแคช",
            ),
        ),
        RecordResolutionState::Degraded => (
            "degraded",
            copy(locale, "Reference unavailable", "การอ้างอิงไม่พร้อมใช้งาน"),
            copy(
                locale,
                "The live resolver is unavailable. No record payload was disclosed.",
                "ตัวค้นหาแบบสดไม่พร้อมใช้งาน และไม่มีการเปิดเผยข้อมูลรายการ",
            ),
        ),
        RecordResolutionState::Offline => (
            "offline",
            copy(
                locale,
                "Reference requires a connection",
                "การอ้างอิงต้องเชื่อมต่อเครือข่าย",
            ),
            copy(
                locale,
                "Reconnect to run a fresh authorization check. Cached authorization is never used.",
                "เชื่อมต่ออีกครั้งเพื่อตรวจสอบสิทธิ์ใหม่ ระบบจะไม่ใช้สิทธิ์จากแคช",
            ),
        ),
    };
    rsx! {
        document::Title { "{title} · ERPX" }
        section { class: "reference-resolution", role: "status", "data-state": state,
            p { class: "eyebrow", {copy(locale, "SHARED REFERENCE", "การอ้างอิงร่วม")} }
            h1 { "{title}" }
            p { "{message}" }
            if let Some(href) = target {
                KitLink { href, {copy(locale, "Continue to record", "ไปยังรายการ")} }
            }
        }
    }
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

#[derive(Clone, PartialEq)]
enum SecurityDialog {
    StartTotp,
    Enrollment(crate::security_delivery::TotpEnrollmentView),
    RemoveTotp,
}

#[cfg(any(feature = "web", feature = "server"))]
fn security_error_copy(
    error: &erpx_delivery_contract::DeliveryError,
    locale: Locale,
) -> (&'static str, &'static str) {
    use erpx_delivery_contract::DeliveryError;
    match (error, locale) {
        (DeliveryError::Unauthenticated(_), Locale::En) => (
            "Session unavailable",
            "Reason: the authenticated session expired or was revoked. Sign in again.",
        ),
        (DeliveryError::Unauthenticated(_), Locale::Th) => (
            "เซสชันไม่พร้อมใช้งาน",
            "เหตุผล: เซสชันหมดอายุหรือถูกเพิกถอน โปรดเข้าสู่ระบบอีกครั้ง",
        ),
        (DeliveryError::Forbidden(_), Locale::En) => (
            "Security change denied",
            "Reason: this identity is managed by another authority or lacks permission.",
        ),
        (DeliveryError::Forbidden(_), Locale::Th) => (
            "ไม่อนุญาตให้เปลี่ยนความปลอดภัย",
            "เหตุผล: ตัวตนนี้ถูกจัดการโดย authority อื่นหรือไม่มีสิทธิ์เพียงพอ",
        ),
        (DeliveryError::Conflict(_), Locale::En) => (
            "Security state changed",
            "Reason: the MFA proof or idempotency receipt was already used. Refresh and retry with a current code.",
        ),
        (DeliveryError::Conflict(_), Locale::Th) => (
            "สถานะความปลอดภัยเปลี่ยนแล้ว",
            "เหตุผล: หลักฐาน MFA หรือ idempotency receipt ถูกใช้แล้ว โปรดโหลดใหม่และใช้รหัสปัจจุบัน",
        ),
        (DeliveryError::RateLimited(_), Locale::En) => (
            "Too many attempts",
            "Reason: the identity owner temporarily limited security attempts. Wait before retrying.",
        ),
        (DeliveryError::RateLimited(_), Locale::Th) => (
            "ลองหลายครั้งเกินไป",
            "เหตุผล: บริการตัวตนจำกัดการลองชั่วคราว โปรดรอก่อนลองอีกครั้ง",
        ),
        (DeliveryError::Unavailable(_), Locale::En) => (
            "Security service unavailable",
            "Reason: the identity owner or protected key service could not be reached.",
        ),
        (DeliveryError::Unavailable(_), Locale::Th) => (
            "บริการความปลอดภัยไม่พร้อมใช้งาน",
            "เหตุผล: ไม่สามารถเชื่อมต่อบริการตัวตนหรือบริการกุญแจที่ได้รับการปกป้อง",
        ),
        (_, Locale::En) => (
            "Security request failed",
            "Reason: validation or the current MFA proof was not accepted by the identity owner.",
        ),
        (_, Locale::Th) => (
            "คำขอความปลอดภัยไม่สำเร็จ",
            "เหตุผล: บริการตัวตนไม่ยอมรับ validation หรือหลักฐาน MFA ปัจจุบัน",
        ),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn Security() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = crate::security_delivery::SecurityStateRequest {
        locale: locale.language_tag().into(),
    };
    let mut resource = use_server_future(move || {
        let request = request.clone();
        async move {
            match crate::security_delivery::get_security_factor_state(request).await {
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
        .expect("resolved security future has a value");
    let mut dialog = use_signal(|| None::<SecurityDialog>);
    let mut verification_code = use_signal(String::new);
    let mut removal_idempotency_key = use_signal(|| uuid::Uuid::new_v4().to_string());
    let mut busy = use_signal(|| false);
    let mut feedback = use_signal(|| None::<DeviceFeedback>);
    let page_title = format!("{} · ERPX", copy(locale, "Security", "ความปลอดภัย"));

    let start_totp = move |_| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        feedback.set(None);
        spawn(async move {
            let result = crate::security_delivery::start_totp_enrollment(
                crate::security_delivery::BeginTotpRequest {
                    locale: locale.language_tag().into(),
                    purpose: "enroll account TOTP factor".into(),
                    confirmed: true,
                },
            )
            .await;
            busy.set(false);
            match result {
                Ok(Ok(enrollment)) => {
                    verification_code.set(String::new());
                    dialog.set(Some(SecurityDialog::Enrollment(enrollment)));
                }
                Ok(Err(error)) => {
                    let (title, reason) = security_error_copy(&error, locale);
                    feedback.set(Some(DeviceFeedback {
                        state: "error",
                        message: format!(
                            "{title}. {reason} {}: {}",
                            copy(locale, "Reference", "รหัสอ้างอิง"),
                            error.meta().correlation_id
                        ),
                        audit_reference: None,
                    }));
                }
                Err(_) => feedback.set(Some(DeviceFeedback {
                    state: "error",
                    message: copy(
                        locale,
                        "The same-origin TOTP enrollment transport is unavailable.",
                        "ช่องทางลงทะเบียน TOTP แบบ same-origin ไม่พร้อมใช้งาน",
                    )
                    .into(),
                    audit_reference: None,
                })),
            }
        });
    };

    let verify_totp = move |_| {
        let Some(SecurityDialog::Enrollment(enrollment)) = dialog.read().clone() else {
            return;
        };
        if *busy.read() {
            return;
        }
        busy.set(true);
        let code = verification_code.read().clone();
        spawn(async move {
            let result = crate::security_delivery::confirm_totp_enrollment(
                crate::security_delivery::VerifyTotpRequest {
                    enrollment_id: enrollment.enrollment_id,
                    code,
                    locale: locale.language_tag().into(),
                },
            )
            .await;
            busy.set(false);
            match result {
                Ok(Ok(outcome)) if outcome.verified => {
                    verification_code.set(String::new());
                    dialog.set(None);
                    feedback.set(Some(DeviceFeedback {
                        state: "success",
                        message: copy(
                            locale,
                            "TOTP was verified by the identity owner. Create recovery codes next.",
                            "บริการตัวตนยืนยัน TOTP แล้ว โปรดสร้างรหัสกู้คืนเป็นขั้นตอนถัดไป",
                        )
                        .into(),
                        audit_reference: None,
                    }));
                    resource.restart();
                }
                Ok(Ok(_)) => feedback.set(Some(DeviceFeedback {
                    state: "pending",
                    message: copy(
                        locale,
                        "Verification was accepted but authoritative confirmation is pending.",
                        "ระบบรับการยืนยันแล้ว แต่ยังรอสถานะที่เชื่อถือได้",
                    )
                    .into(),
                    audit_reference: None,
                })),
                Ok(Err(error)) => {
                    let (title, reason) = security_error_copy(&error, locale);
                    feedback.set(Some(DeviceFeedback {
                        state: "error",
                        message: format!(
                            "{title}. {reason} {}: {}",
                            copy(locale, "Reference", "รหัสอ้างอิง"),
                            error.meta().correlation_id
                        ),
                        audit_reference: None,
                    }));
                }
                Err(_) => feedback.set(Some(DeviceFeedback {
                    state: "error",
                    message: copy(
                        locale,
                        "The same-origin TOTP verification transport is unavailable.",
                        "ช่องทางยืนยัน TOTP แบบ same-origin ไม่พร้อมใช้งาน",
                    )
                    .into(),
                    audit_reference: None,
                })),
            }
        });
    };

    let remove_totp = move |_| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        let code = verification_code.read().clone();
        let idempotency_key = removal_idempotency_key.read().clone();
        spawn(async move {
            let result = crate::security_delivery::remove_totp_factor(
                crate::security_delivery::HighRiskSecurityRequest {
                    mfa_code: code,
                    idempotency_key,
                    locale: locale.language_tag().into(),
                    purpose: "remove active account TOTP factor".into(),
                    confirmed: true,
                },
            )
            .await;
            busy.set(false);
            match result {
                Ok(Ok(outcome)) => {
                    verification_code.set(String::new());
                    dialog.set(None);
                    feedback.set(Some(DeviceFeedback {
                        state: if outcome.authoritative_state == "confirmed" {
                            "success"
                        } else {
                            "pending"
                        },
                        message: copy(
                            locale,
                            "TOTP removal was committed and the authoritative factor state was refreshed.",
                            "บันทึกการนำ TOTP ออกแล้ว และโหลดสถานะ factor ที่เชื่อถือได้ใหม่แล้ว",
                        )
                        .into(),
                        audit_reference: Some(outcome.receipt.audit_reference.to_string()),
                    }));
                    resource.restart();
                }
                Ok(Err(error)) => {
                    let (title, reason) = security_error_copy(&error, locale);
                    feedback.set(Some(DeviceFeedback {
                        state: "error",
                        message: format!(
                            "{title}. {reason} {}: {}",
                            copy(locale, "Reference", "รหัสอ้างอิง"),
                            error.meta().correlation_id
                        ),
                        audit_reference: None,
                    }));
                }
                Err(_) => feedback.set(Some(DeviceFeedback {
                    state: "error",
                    message: copy(
                        locale,
                        "The same-origin TOTP removal transport is unavailable.",
                        "ช่องทางนำ TOTP ออกแบบ same-origin ไม่พร้อมใช้งาน",
                    )
                    .into(),
                    audit_reference: None,
                })),
            }
        });
    };

    rsx! {
        document::Title { "{page_title}" }
        article { class: "auth-page-form auth-utility-page security-factor-page",
            p { class: "eyebrow", {copy(locale, "SECURITY CENTER", "ศูนย์ความปลอดภัย")} }
            div { class: "auth-utility-heading",
                span { aria_hidden: "true", "⌾" }
                div {
                    h2 { {copy(locale, "Security factors", "ปัจจัยความปลอดภัย")} }
                    p { class: "auth-page-form__intro", {copy(locale,
                        "Manage factors against the authoritative identity owner. One-time material is never stored in shared caches.",
                        "จัดการ factor กับบริการเจ้าของข้อมูลตัวตนโดยตรง ข้อมูลใช้ครั้งเดียวจะไม่ถูกเก็บในแคชร่วม",
                    )} }
                }
            }
            match page_state {
                Err(error) => {
                    let (title, reason) = security_error_copy(&error, locale);
                    rsx! {
                        section { class: "route-denied", role: "alert", "data-reason": "identity_security_query_failed",
                            h3 { "{title}" }
                            p { "{reason}" }
                            small { {copy(locale, "Reference", "รหัสอ้างอิง")} ": {error.meta().correlation_id}" }
                            KitButton { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| resource.restart(), {copy(locale, "Try again", "ลองอีกครั้ง")} }
                        }
                    }
                },
                Ok(page) => rsx! {
                    section { class: "auth-utility-status", role: "status",
                        span { class: "auth-utility-status__dot", aria_hidden: "true" }
                        div {
                            strong { {copy(locale, "Authoritative factor state", "สถานะ factor ที่เชื่อถือได้")} }
                            p { {copy(locale, "Measured at", "วัดเมื่อ")} " {page.measured_at} · {page.freshness}" }
                            small { {copy(locale, "Reference", "รหัสอ้างอิง")} ": {page.correlation_id}" }
                        }
                    }
                    if page.directory_managed {
                        section { class: "device-authoritative-empty", role: "status",
                            h3 { {copy(locale, "Directory-managed identity", "ตัวตนที่จัดการโดย Directory")} }
                            p { {copy(locale,
                                "Local factor changes are unavailable because the directory authority owns this identity.",
                                "ไม่สามารถเปลี่ยน factor ภายในระบบได้ เนื่องจาก directory authority เป็นเจ้าของตัวตนนี้",
                            )} }
                        }
                    }
                    section { class: "security-factor-grid", aria_label: copy(locale, "Security factors", "ปัจจัยความปลอดภัย"),
                        article { class: "security-factor-card", "data-enabled": page.totp_enabled,
                            div {
                                h3 { "TOTP" }
                                p { if page.totp_enabled { {copy(locale, "Enabled", "เปิดใช้งานแล้ว")} } else { {copy(locale, "Not enrolled", "ยังไม่ได้ลงทะเบียน")} } }
                                if let Some(enabled_at) = page.totp_enabled_at.as_ref() { small { {copy(locale, "Enabled at", "เปิดใช้เมื่อ")} ": {enabled_at}" } }
                            }
                            if !page.directory_managed {
                                if page.totp_enabled {
                                    KitButton { variant: ButtonVariant::Secondary, r#type: "button", disabled: *busy.read(), "data-testid": "totp-remove", onclick: move |_| { verification_code.set(String::new()); removal_idempotency_key.set(uuid::Uuid::new_v4().to_string()); dialog.set(Some(SecurityDialog::RemoveTotp)); }, {copy(locale, "Remove TOTP", "นำ TOTP ออก")} }
                                } else {
                                    KitButton { variant: ButtonVariant::Primary, r#type: "button", disabled: *busy.read(), "data-testid": "totp-enroll", onclick: move |_| dialog.set(Some(SecurityDialog::StartTotp)), {copy(locale, "Set up TOTP", "ตั้งค่า TOTP")} }
                                }
                            }
                        }
                        article { class: "security-factor-card",
                            div {
                                h3 { {copy(locale, "Passkeys", "Passkey")} }
                                p {
                                    "{page.passkey_count} "
                                    {copy(locale, "active", "รายการที่ใช้งาน")}
                                }
                            }
                            small { {copy(locale, "Passkey registration will remain unavailable unless the live provider is configured.", "การลงทะเบียน Passkey จะยังไม่พร้อมจนกว่าจะตั้งค่า live provider")} }
                        }
                        article { class: "security-factor-card",
                            div {
                                h3 { {copy(locale, "Recovery codes", "รหัสกู้คืน")} }
                                p {
                                    "{page.recovery_codes_remaining}/{page.recovery_codes_total} "
                                    {copy(locale, "remaining", "คงเหลือ")}
                                }
                            }
                            KitLink { href: "/auth/recovery", {copy(locale, "Manage recovery codes", "จัดการรหัสกู้คืน")} }
                        }
                    }
                }
            }
            if let Some(active_dialog) = dialog.read().as_ref() {
                section { class: "device-revoke-confirmation security-factor-dialog", role: "alertdialog", aria_modal: "true", aria_labelledby: "security-dialog-title", aria_describedby: "security-dialog-description",
                    match active_dialog {
                        SecurityDialog::StartTotp => rsx! {
                            strong { id: "security-dialog-title", {copy(locale, "Set up TOTP?", "ตั้งค่า TOTP หรือไม่")} }
                            p { id: "security-dialog-description", {copy(locale,
                                "A new one-time secret will be shown once. Store it only in your authenticator app.",
                                "ระบบจะแสดง secret ใช้ครั้งเดียวหนึ่งครั้ง โปรดเก็บไว้เฉพาะในแอปยืนยันตัวตน",
                            )} }
                            div { class: "route-actions",
                                KitButton { variant: ButtonVariant::Secondary, r#type: "button", disabled: *busy.read(), onclick: move |_| dialog.set(None), {copy(locale, "Cancel", "ยกเลิก")} }
                                KitButton { variant: ButtonVariant::Primary, r#type: "button", disabled: *busy.read(), "data-testid": "totp-generate", onclick: start_totp, {copy(locale, "Generate secret", "สร้าง secret")} }
                            }
                        },
                        SecurityDialog::Enrollment(enrollment) => rsx! {
                            strong { id: "security-dialog-title", {copy(locale, "Add the manual key", "เพิ่มกุญแจด้วยตนเอง")} }
                            p { id: "security-dialog-description", {copy(locale,
                                "Enter this key in your authenticator. It will disappear after verification or cancellation.",
                                "กรอกกุญแจนี้ในแอปยืนยันตัวตน กุญแจจะหายไปหลังยืนยันหรือยกเลิก",
                            )} }
                            code { class: "security-one-time-secret", "data-testid": "totp-secret", "{enrollment.secret}" }
                            label { class: "auth-page-form__field",
                                span { {copy(locale, "Current 6-digit code", "รหัสปัจจุบัน 6 หลัก")} }
                                KitInput { r#type: "text", inputmode: "numeric", autocomplete: "one-time-code", maxlength: "6", value: "{verification_code}", disabled: *busy.read(), "data-testid": "totp-code", oninput: move |event: FormEvent| verification_code.set(event.value()) }
                            }
                            div { class: "route-actions",
                                KitButton { variant: ButtonVariant::Secondary, r#type: "button", disabled: *busy.read(), onclick: move |_| { verification_code.set(String::new()); dialog.set(None); }, {copy(locale, "Cancel", "ยกเลิก")} }
                                KitButton { variant: ButtonVariant::Primary, r#type: "button", disabled: *busy.read() || !valid_six_digit_code(&verification_code.read()), "data-testid": "totp-verify", onclick: verify_totp, {copy(locale, "Verify TOTP", "ยืนยัน TOTP")} }
                            }
                        },
                        SecurityDialog::RemoveTotp => rsx! {
                            strong { id: "security-dialog-title", {copy(locale, "Remove TOTP?", "นำ TOTP ออกหรือไม่")} }
                            p { id: "security-dialog-description", {copy(locale,
                                "Confirm with a current TOTP code. Existing recovery codes remain available.",
                                "ยืนยันด้วยรหัส TOTP ปัจจุบัน รหัสกู้คืนเดิมจะยังใช้งานได้",
                            )} }
                            label { class: "auth-page-form__field",
                                span { {copy(locale, "Current 6-digit code", "รหัสปัจจุบัน 6 หลัก")} }
                                KitInput { r#type: "text", inputmode: "numeric", autocomplete: "one-time-code", maxlength: "6", value: "{verification_code}", disabled: *busy.read(), "data-testid": "totp-remove-code", oninput: move |event: FormEvent| verification_code.set(event.value()) }
                            }
                            div { class: "route-actions",
                                KitButton { variant: ButtonVariant::Secondary, r#type: "button", disabled: *busy.read(), onclick: move |_| { verification_code.set(String::new()); dialog.set(None); }, {copy(locale, "Cancel", "ยกเลิก")} }
                                KitButton { variant: ButtonVariant::Secondary, class: "account-remove-button", r#type: "button", disabled: *busy.read() || !valid_six_digit_code(&verification_code.read()), "data-testid": "totp-remove-confirm", onclick: remove_totp, {copy(locale, "Confirm removal", "ยืนยันการนำออก")} }
                            }
                        },
                    }
                }
            }
            if let Some(status) = feedback.read().as_ref() {
                section { class: "identity-command-feedback", "data-state": status.state, role: "status",
                    p { "{status.message}" }
                    if let Some(reference) = status.audit_reference.as_ref() { small { {copy(locale, "Audit reference", "รหัสอ้างอิงการตรวจสอบ")} ": {reference}" } }
                }
            }
            div { class: "route-actions auth-utility-actions",
                KitLink { href: "/auth/devices", {copy(locale, "Devices and sessions", "อุปกรณ์และเซสชัน")} }
                KitLink { href: "/work", {copy(locale, "Return to work", "กลับไปยังงาน")} }
            }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn Security() -> Element {
    rsx! { FeatureClosed { name: "Security factors" } }
}

fn valid_six_digit_code(value: &str) -> bool {
    value.len() == 6 && value.as_bytes().iter().all(u8::is_ascii_digit)
}

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

#[cfg(any(feature = "web", feature = "server"))]
#[component]
fn Recovery() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let request = crate::security_delivery::SecurityStateRequest {
        locale: locale.language_tag().into(),
    };
    let mut resource = use_server_future(move || {
        let request = request.clone();
        async move {
            match crate::security_delivery::get_security_factor_state(request).await {
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
        .expect("resolved recovery future has a value");
    let mut confirmation_open = use_signal(|| false);
    let mut mfa_code = use_signal(String::new);
    let mut idempotency_key = use_signal(|| uuid::Uuid::new_v4().to_string());
    let mut busy = use_signal(|| false);
    let mut feedback = use_signal(|| None::<DeviceFeedback>);
    let mut issued_codes =
        use_signal(|| None::<crate::security_delivery::RecoveryCodeRotationView>);
    let page_title = format!("{} · ERPX", copy(locale, "Recovery", "การกู้คืน"));

    let rotate_codes = move |_| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        let code = mfa_code.read().clone();
        let request_key = idempotency_key.read().clone();
        spawn(async move {
            let result = crate::security_delivery::rotate_recovery_codes(
                crate::security_delivery::HighRiskSecurityRequest {
                    mfa_code: code,
                    idempotency_key: request_key,
                    locale: locale.language_tag().into(),
                    purpose: "rotate account recovery codes".into(),
                    confirmed: true,
                },
            )
            .await;
            busy.set(false);
            match result {
                Ok(Ok(outcome)) => {
                    confirmation_open.set(false);
                    mfa_code.set(String::new());
                    idempotency_key.set(uuid::Uuid::new_v4().to_string());
                    feedback.set(Some(DeviceFeedback {
                        state: if outcome.authoritative_state == "confirmed" {
                            "success"
                        } else {
                            "pending"
                        },
                        message: if outcome.receipt.replayed && outcome.codes.is_empty() {
                            copy(
                                locale,
                                "The earlier rotation is already committed. One-time codes cannot be shown again; rotate once more with a new current MFA code if they were not saved.",
                                "การหมุนเวียนครั้งก่อนถูกบันทึกแล้ว ไม่สามารถแสดงรหัสใช้ครั้งเดียวซ้ำได้ หากยังไม่ได้บันทึกโปรดหมุนเวียนอีกครั้งด้วยรหัส MFA ปัจจุบันใหม่",
                            )
                        } else {
                            copy(
                                locale,
                                "Recovery codes were rotated and confirmed by the identity owner. Save them now.",
                                "หมุนเวียนรหัสกู้คืนและยืนยันจากบริการตัวตนแล้ว โปรดบันทึกรหัสตอนนี้",
                            )
                        }
                        .into(),
                        audit_reference: Some(outcome.receipt.audit_reference.to_string()),
                    }));
                    issued_codes.set(Some(outcome));
                    resource.restart();
                }
                Ok(Err(error)) => {
                    let (title, reason) = security_error_copy(&error, locale);
                    feedback.set(Some(DeviceFeedback {
                        state: "error",
                        message: format!(
                            "{title}. {reason} {}: {}",
                            copy(locale, "Reference", "รหัสอ้างอิง"),
                            error.meta().correlation_id
                        ),
                        audit_reference: None,
                    }));
                }
                Err(_) => feedback.set(Some(DeviceFeedback {
                    state: "error",
                    message: copy(
                        locale,
                        "The same-origin recovery rotation transport is unavailable. Retry safely with the same request.",
                        "ช่องทางหมุนเวียนรหัสกู้คืนแบบ same-origin ไม่พร้อมใช้งาน สามารถลองคำขอเดิมซ้ำได้อย่างปลอดภัย",
                    )
                    .into(),
                    audit_reference: None,
                })),
            }
        });
    };

    rsx! {
        document::Title { "{page_title}" }
        article { class: "auth-page-form auth-utility-page security-factor-page",
            p { class: "eyebrow", {copy(locale, "SECURITY CENTER", "ศูนย์ความปลอดภัย")} }
            div { class: "auth-utility-heading",
                span { aria_hidden: "true", "↻" }
                div {
                    h2 { {copy(locale, "Recovery codes", "รหัสกู้คืน")} }
                    p { class: "auth-page-form__intro", {copy(locale,
                        "Rotate recovery codes with a fresh TOTP proof. Previous code hashes are replaced atomically.",
                        "หมุนเวียนรหัสกู้คืนด้วยหลักฐาน TOTP ล่าสุด โดย hash ของรหัสเดิมจะถูกแทนที่แบบ atomic",
                    )} }
                }
            }
            match page_state {
                Err(error) => {
                    let (title, reason) = security_error_copy(&error, locale);
                    rsx! {
                        section { class: "route-denied", role: "alert", "data-reason": "identity_recovery_query_failed",
                            h3 { "{title}" }
                            p { "{reason}" }
                            small { {copy(locale, "Reference", "รหัสอ้างอิง")} ": {error.meta().correlation_id}" }
                            KitButton { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| resource.restart(), {copy(locale, "Try again", "ลองอีกครั้ง")} }
                        }
                    }
                },
                Ok(page) => rsx! {
                    section { class: "auth-utility-status", role: "status",
                        span { class: "auth-utility-status__dot", aria_hidden: "true" }
                        div {
                            strong { {copy(locale, "Authoritative recovery state", "สถานะการกู้คืนที่เชื่อถือได้")} }
                            p {
                                "{page.recovery_codes_remaining}/{page.recovery_codes_total} "
                                {copy(locale, "codes remaining", "รหัสคงเหลือ")}
                                " · "
                                {copy(locale, "measured", "วัดเมื่อ")}
                                " {page.measured_at}"
                            }
                            small { {copy(locale, "Reference", "รหัสอ้างอิง")} ": {page.correlation_id}" }
                        }
                    }
                    if page.directory_managed {
                        section { class: "device-authoritative-empty", role: "status",
                            h3 { {copy(locale, "Directory-managed recovery", "การกู้คืนที่จัดการโดย Directory")} }
                            p { {copy(locale,
                                "Local recovery-code rotation is unavailable for this directory-managed identity.",
                                "ไม่สามารถหมุนเวียนรหัสกู้คืนภายในระบบสำหรับตัวตนที่จัดการโดย Directory",
                            )} }
                        }
                    } else if !page.totp_enabled {
                        section { class: "device-authoritative-empty", role: "status",
                            h3 { {copy(locale, "TOTP required", "ต้องใช้ TOTP")} }
                            p { {copy(locale,
                                "Set up TOTP before creating or rotating recovery codes.",
                                "ตั้งค่า TOTP ก่อนสร้างหรือหมุนเวียนรหัสกู้คืน",
                            )} }
                            KitLink { href: "/auth/security", {copy(locale, "Set up TOTP", "ตั้งค่า TOTP")} }
                        }
                    } else {
                        section { class: "security-recovery-actions",
                            p { {copy(locale,
                                "Generating a new batch invalidates every previous recovery code, including unused codes.",
                                "การสร้างชุดใหม่จะทำให้รหัสกู้คืนเดิมทั้งหมดใช้ไม่ได้ รวมถึงรหัสที่ยังไม่เคยใช้",
                            )} }
                            KitButton {
                                variant: ButtonVariant::Primary,
                                r#type: "button",
                                disabled: *busy.read(),
                                "data-testid": "recovery-rotate",
                                onclick: move |_| {
                                    issued_codes.set(None);
                                    mfa_code.set(String::new());
                                    idempotency_key.set(uuid::Uuid::new_v4().to_string());
                                    confirmation_open.set(true);
                                },
                                {copy(locale, "Rotate 8 recovery codes", "หมุนเวียนรหัสกู้คืน 8 รหัส")}
                            }
                        }
                    }
                }
            }
            if *confirmation_open.read() {
                section { class: "device-revoke-confirmation security-factor-dialog", role: "alertdialog", aria_modal: "true", aria_labelledby: "recovery-dialog-title", aria_describedby: "recovery-dialog-description",
                    strong { id: "recovery-dialog-title", {copy(locale, "Rotate all recovery codes?", "หมุนเวียนรหัสกู้คืนทั้งหมดหรือไม่")} }
                    p { id: "recovery-dialog-description", {copy(locale,
                        "Confirm with a current 6-digit TOTP code. The new codes are displayed once.",
                        "ยืนยันด้วยรหัส TOTP ปัจจุบัน 6 หลัก ระบบจะแสดงรหัสใหม่เพียงครั้งเดียว",
                    )} }
                    label { class: "auth-page-form__field",
                        span { {copy(locale, "Current 6-digit code", "รหัสปัจจุบัน 6 หลัก")} }
                        KitInput { r#type: "text", inputmode: "numeric", autocomplete: "one-time-code", maxlength: "6", value: "{mfa_code}", disabled: *busy.read(), "data-testid": "recovery-mfa-code", oninput: move |event: FormEvent| mfa_code.set(event.value()) }
                    }
                    div { class: "route-actions",
                        KitButton { variant: ButtonVariant::Secondary, r#type: "button", disabled: *busy.read(), "data-testid": "recovery-rotate-cancel", onclick: move |_| { mfa_code.set(String::new()); confirmation_open.set(false); }, {copy(locale, "Cancel", "ยกเลิก")} }
                        KitButton { variant: ButtonVariant::Primary, r#type: "button", disabled: *busy.read() || !valid_six_digit_code(&mfa_code.read()), "data-testid": "recovery-rotate-confirm", onclick: rotate_codes, {copy(locale, "Confirm rotation", "ยืนยันการหมุนเวียน")} }
                    }
                }
            }
            if let Some(outcome) = issued_codes.read().as_ref() {
                section { class: "security-one-time-codes", role: "status", "data-testid": "recovery-codes",
                    h3 { {copy(locale, "Save these codes now", "บันทึกรหัสเหล่านี้ตอนนี้")} }
                    if outcome.codes.is_empty() {
                        p { {copy(locale,
                            "This was an idempotent replay, so one-time codes are not disclosed again.",
                            "คำขอนี้เป็น idempotent replay จึงไม่เปิดเผยรหัสใช้ครั้งเดียวซ้ำ",
                        )} }
                    } else {
                        p { {copy(locale,
                            "Each code works once. Do not paste them into chat, tickets, or shared documents.",
                            "แต่ละรหัสใช้ได้ครั้งเดียว ห้ามวางรหัสในแชต ticket หรือเอกสารร่วม",
                        )} }
                        ol {
                            for code in outcome.codes.iter() { li { code { "{code}" } } }
                        }
                    }
                    KitButton { variant: ButtonVariant::Secondary, r#type: "button", onclick: move |_| issued_codes.set(None), {copy(locale, "I saved them", "ฉันบันทึกแล้ว")} }
                }
            }
            if let Some(status) = feedback.read().as_ref() {
                section { class: "identity-command-feedback", "data-state": status.state, role: "status",
                    p { "{status.message}" }
                    if let Some(reference) = status.audit_reference.as_ref() { small { {copy(locale, "Audit reference", "รหัสอ้างอิงการตรวจสอบ")} ": {reference}" } }
                }
            }
            div { class: "route-actions auth-utility-actions",
                KitLink { href: "/auth/security", {copy(locale, "Security factors", "ปัจจัยความปลอดภัย")} }
                KitLink { href: "/auth/devices", {copy(locale, "Devices and sessions", "อุปกรณ์และเซสชัน")} }
            }
            p { class: "auth-page-form__security", {copy(locale,
                "Recovery codes are returned only to this protected page and are never persisted in plaintext by ERPX.",
                "รหัสกู้คืนจะส่งกลับมายังหน้าที่ได้รับการปกป้องนี้เท่านั้น และ ERPX จะไม่จัดเก็บรหัสแบบ plaintext",
            )} }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn Recovery() -> Element {
    rsx! { FeatureClosed { name: "Recovery codes" } }
}

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
        Workspace::Operations => "exception",
        Workspace::Finance => "journal",
        Workspace::Sales => "order",
        Workspace::Procurement => "purchase-order",
        Workspace::Inventory => "document",
        Workspace::Asset => "asset",
        Workspace::Maintenance => "work-order",
        Workspace::Quality => "inspection",
        Workspace::Marketing => "campaign",
        Workspace::People => "employee",
        Workspace::Pos => "sale",
        Workspace::Legal => "contract",
        Workspace::Manufacturing => "order",
        Workspace::Planning => "demand-plan",
        Workspace::Project => "project",
        Workspace::Commerce => "catalog-publication",
        Workspace::Talent => "candidate",
        Workspace::Learning => "enrollment",
        Workspace::Benefits => "plan",
        Workspace::FieldService => "visit",
        Workspace::Ai => "thread",
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
        ("exception" | "exceptions", Locale::Th) => "ข้อยกเว้น",
        ("journal" | "journal-entries", Locale::Th) => "รายการสมุดรายวัน",
        ("order" | "orders", Locale::Th) => "คำสั่งซื้อ",
        ("purchase-order" | "purchase-orders", Locale::Th) => "ใบสั่งซื้อ",
        ("document" | "stock-items", Locale::Th) => "เอกสารสินค้าคงคลัง",
        ("asset" | "assets", Locale::Th) => "สินทรัพย์",
        ("work-order" | "work-orders", Locale::Th) => "ใบงาน",
        ("inspection" | "inspections", Locale::Th) => "การตรวจสอบ",
        ("campaign" | "campaigns", Locale::Th) => "แคมเปญ",
        ("employee" | "employees", Locale::Th) => "พนักงาน",
        ("sale" | "sales", Locale::Th) => "รายการขาย",
        ("contract" | "contracts", Locale::Th) => "สัญญา",
        ("demand-plan" | "demand-plans", Locale::Th) => "แผนอุปสงค์",
        ("project" | "projects", Locale::Th) => "โครงการ",
        ("catalog-publication" | "listings", Locale::Th) => "รายการสินค้าออนไลน์",
        ("candidate" | "candidates", Locale::Th) => "ผู้สมัคร",
        ("enrollment" | "enrollments", Locale::Th) => "การลงทะเบียน",
        ("plan" | "benefit-plans", Locale::Th) => "แผนสวัสดิการ",
        ("visit" | "service-orders", Locale::Th) => "ใบงานบริการ",
        ("thread" | "conversations", Locale::Th) => "บทสนทนา",
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

fn route_freshness_is_unavailable(freshness_tag: &str) -> bool {
    matches!(freshness_tag, "not-connected" | "unavailable")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteUnavailableCopy {
    title: String,
    reason: String,
}

fn route_unavailable_copy(
    query: &RouteQuery,
    heading: &str,
    locale: Locale,
) -> RouteUnavailableCopy {
    let title = match locale {
        Locale::En => format!("{heading} is unavailable"),
        Locale::Th => format!("{heading} ยังไม่พร้อมใช้งาน"),
    };
    let reason = match query {
        RouteQuery::Workspace {
            workspace,
            resource,
        } => {
            let owner = workspace_label(*workspace, locale);
            let resource = localized_resource_label(
                resource
                    .as_ref()
                    .map_or_else(|| default_workspace_resource(*workspace), ResourceName::as_str),
                locale,
            );
            match locale {
                Locale::En => format!(
                    "The {} backend could not provide an authoritative {} list. No rows are being shown.",
                    owner.to_ascii_lowercase(),
                    resource.to_ascii_lowercase(),
                ),
                Locale::Th => format!(
                    "บริการข้อมูล{owner}ไม่สามารถส่งรายการ{resource}จากแหล่งข้อมูลที่เชื่อถือได้ จึงไม่มีการแสดงข้อมูล"
                ),
            }
        }
        RouteQuery::Record {
            workspace,
            resource,
            ..
        } => {
            let owner = workspace_label(*workspace, locale);
            let resource = localized_resource_label(resource.as_str(), locale);
            match locale {
                Locale::En => format!(
                    "The {} backend could not provide this authoritative {} record. No record is being shown.",
                    owner.to_ascii_lowercase(),
                    resource.to_ascii_lowercase(),
                ),
                Locale::Th => format!(
                    "บริการข้อมูล{owner}ไม่สามารถส่งรายการ{resource}นี้จากแหล่งข้อมูลที่เชื่อถือได้ จึงไม่มีการแสดงรายการ"
                ),
            }
        }
        RouteQuery::Reference { .. } => match locale {
            Locale::En => "The shared-reference backend could not provide this authoritative record. No record is being shown.".to_owned(),
            Locale::Th => "บริการข้อมูลอ้างอิงร่วมไม่สามารถส่งรายการนี้จากแหล่งข้อมูลที่เชื่อถือได้ จึงไม่มีการแสดงรายการ".to_owned(),
        },
        _ => match locale {
            Locale::En => format!(
                "The backend could not provide authoritative {heading} data. No rows are being shown."
            ),
            Locale::Th => format!(
                "บริการข้อมูลไม่สามารถส่งข้อมูล {heading} จากแหล่งข้อมูลที่เชื่อถือได้ จึงไม่มีการแสดงข้อมูล"
            ),
        },
    };

    RouteUnavailableCopy { title, reason }
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
        ) => {
            let cataloged_detail = match &route {
                AppRoute::WorkspaceRecord {
                    workspace,
                    resource,
                    id,
                } => CatalogRecord::try_from_segments(
                    workspace.slug(),
                    resource.as_str(),
                    &id.to_string(),
                )
                .is_ok(),
                _ => true,
            };
            route_path(&route) == path && workspace.available_on(channel) && cataloged_detail
        }
        Ok(route @ AppRoute::CatalogRecordDetail { .. }) => {
            let AppRoute::CatalogRecordDetail { module, record, id } = &route else {
                unreachable!("matched catalog detail")
            };
            route_path(&route) == path
                && CatalogRecord::try_from_segments(
                    module.as_str(),
                    record.as_str(),
                    &id.to_string(),
                )
                .is_ok()
        }
        Ok(AppRoute::PlatformHolding { .. }) => false,
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
    fn unconnected_workspace_copy_names_the_authoritative_owner_and_resource() {
        assert!(route_freshness_is_unavailable("not-connected"));
        assert!(route_freshness_is_unavailable("unavailable"));
        assert!(!route_freshness_is_unavailable("partial"));
        assert!(!route_freshness_is_unavailable("fresh"));

        for workspace in Workspace::ALL {
            let route = RouteQuery::Workspace {
                workspace,
                resource: None,
            };
            let heading = route_heading(&route, "unused", Locale::En);
            let unavailable = route_unavailable_copy(&route, &heading, Locale::En);

            assert_eq!(unavailable.title, format!("{heading} is unavailable"));
            assert!(
                unavailable
                    .reason
                    .starts_with(&format!("The {} backend", heading.to_ascii_lowercase())),
                "{}",
                unavailable.reason
            );
            assert!(
                unavailable
                    .reason
                    .contains("could not provide an authoritative"),
                "{}",
                unavailable.reason
            );
            assert!(unavailable.reason.ends_with("No rows are being shown."));
        }

        let route = RouteQuery::Workspace {
            workspace: Workspace::Inventory,
            resource: Some("stock-items".parse().unwrap()),
        };
        let heading = route_heading(&route, "unused", Locale::En);
        let unavailable = route_unavailable_copy(&route, &heading, Locale::En);
        assert_eq!(unavailable.title, "Inventory · Stock Items is unavailable");
        assert_eq!(
            unavailable.reason,
            "The inventory backend could not provide an authoritative stock items list. No rows are being shown."
        );
    }

    #[test]
    fn unconnected_route_copy_distinguishes_lists_records_and_thai() {
        let id = RouteUuid::try_new(uuid::Uuid::from_u128(11)).unwrap();
        let record = RouteQuery::Record {
            workspace: Workspace::People,
            resource: "employees".parse().unwrap(),
            id,
        };
        let english = route_unavailable_copy(&record, "Employee · 11", Locale::En);
        assert_eq!(english.title, "Employee · 11 is unavailable");
        assert!(
            english
                .reason
                .contains("this authoritative employees record")
        );
        assert!(english.reason.ends_with("No record is being shown."));

        let thai_route = RouteQuery::Workspace {
            workspace: Workspace::Inventory,
            resource: Some("stock-items".parse().unwrap()),
        };
        let thai_heading = route_heading(&thai_route, "unused", Locale::Th);
        let thai = route_unavailable_copy(&thai_route, &thai_heading, Locale::Th);
        assert_eq!(thai.title, "สินค้าคงคลัง · เอกสารสินค้าคงคลัง ยังไม่พร้อมใช้งาน");
        assert!(thai.reason.contains("แหล่งข้อมูลที่เชื่อถือได้"));
        assert!(thai.reason.contains("ไม่มีการแสดงข้อมูล"));
    }

    #[test]
    fn platform_holding_plural_detail_is_a_compatibility_redirect() {
        let holding_id = RouteUuid::try_new(uuid::Uuid::from_u128(1)).unwrap();
        let list = AppRoute::PlatformHoldings { cursor: None };
        let compatibility = AppRoute::PlatformHolding {
            holding_id,
            history_cursor: None,
        };
        let detail = AppRoute::PlatformHoldingCanonical {
            holding_id,
            history_cursor: None,
        };
        assert_eq!(route_path(&list), "/platform/holdings");
        assert_eq!(
            route_path(&compatibility),
            "/platform/holdings/00000000-0000-0000-0000-000000000001"
        );
        assert_eq!(
            route_path(&detail),
            "/platform/holding/00000000-0000-0000-0000-000000000001"
        );
        assert!(is_canonical_console_path(
            &route_path(&list),
            ClientChannel::Web
        ));
        assert!(!is_canonical_console_path(
            &route_path(&compatibility),
            ClientChannel::Web
        ));
        assert!(is_canonical_console_path(
            &route_path(&detail),
            ClientChannel::Web
        ));
        let history = AppRoute::PlatformHoldingCanonical {
            holding_id,
            history_cursor: Some("1787270400000.00000000-0000-0000-0000-000000000002".into()),
        };
        assert!(history.to_string().contains("history_cursor="));
        assert_eq!(route_path(&history), route_path(&detail));
        let source = include_str!("routes.rs");
        let redirect = source.find("fn PlatformHolding(").unwrap();
        let canonical = source[redirect..]
            .find("AppRoute::PlatformHoldingCanonical")
            .unwrap();
        let replace = source[redirect..].find("navigator.replace").unwrap();
        assert!(canonical < replace);
        assert!(!is_canonical_console_path(
            "/platform/holdings/not-a-uuid",
            ClientChannel::Web
        ));
    }

    #[test]
    fn platform_support_owner_routes_are_canonical_uuid_routes() {
        let id = RouteUuid::try_new(uuid::Uuid::from_u128(7)).unwrap();
        for path in ["/platform/support", "/platform/incidents"] {
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
            "/platform/support/not-a-uuid",
            ClientChannel::Web
        ));
        assert!(!is_canonical_console_path(
            "/platform/incidents/not-a-uuid",
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

    #[test]
    fn all_93_current_catalog_families_route_through_the_shared_record_host() {
        use erpx_integration_contracts::catalog::{ContextLifecycle, RECORD_TYPES};

        let id = uuid::Uuid::from_u128(0x92);
        let mut owners = std::collections::BTreeSet::new();
        let current = RECORD_TYPES
            .iter()
            .filter(|descriptor| descriptor.owner.lifecycle() == ContextLifecycle::Current)
            .collect::<Vec<_>>();
        assert_eq!(current.len(), 93);

        for descriptor in current {
            owners.insert(descriptor.owner);
            let path = format!("/{}/{}/{id}", descriptor.module, descriptor.record);
            let record = CatalogRecord::try_from_path(&path).expect("catalog detail identity");
            assert!(record.collaboration_available(), "{}", descriptor.id);
            assert_eq!(record.canonical_path(), path);
            assert!(
                is_canonical_console_path(&path, ClientChannel::Web),
                "{path}"
            );
            match AppRoute::from_str(&path).expect("current detail route parses") {
                AppRoute::IdentityPrincipal { .. }
                | AppRoute::IdentityRole { .. }
                | AppRoute::IdentityGrant { .. }
                | AppRoute::IdentityAccessReview { .. }
                | AppRoute::IdentityBreakGlassSession { .. }
                | AppRoute::PlatformHoldingCanonical { .. }
                | AppRoute::WorkspaceRecord { .. }
                | AppRoute::CatalogRecordDetail { .. } => {}
                route => panic!("{} bypasses RecordDetailHost via {route:?}", descriptor.id),
            }
        }
        assert_eq!(
            owners.len(),
            21,
            "one or more current contexts are not routed"
        );

        let source = include_str!("routes.rs");
        let host = source.find("pub fn RecordDetailHost").unwrap();
        let references = source[host..].find("RecordReferences {").unwrap();
        let activity = source[host..]
            .find("AuthorizedRecordActivityRail {")
            .unwrap();
        assert!(references > 0 && activity > references);
    }

    #[test]
    fn collaboration_route_fixtures_cover_phase_two_canaries_and_all_current_contexts() {
        let id = uuid::Uuid::from_u128(0xe2e);
        for record_type in [
            "identity.principal",
            "platform.holding",
            "organization.holding",
            "commercial.subscription",
            "brand.profile",
            "customer.account",
            "customer-360.profile",
            "support.customer-case",
            "finance.invoice",
            "finance.vendor-invoice",
            "finance.payment",
            "sales.quote",
            "pos.sale",
            "procurement.requisition",
            "procurement.purchase-order",
            "procurement.receipt",
            "inventory.document",
            "asset.asset",
            "maintenance.work-order",
            "quality.inspection",
            "operations.exception",
            "people.employee",
            "legal.contract",
            "marketing.campaign",
            "crm.lead",
        ] {
            let descriptor = erpx_integration_contracts::catalog::record_type(record_type)
                .expect("fixture stays in catalog");
            let path = format!("/{}/{}/{id}", descriptor.module, descriptor.record);
            let record = CatalogRecord::try_from_path(&path).unwrap();
            assert!(record.collaboration_available(), "{record_type}");
            assert!(
                is_canonical_console_path(&path, ClientChannel::Web),
                "{path}"
            );
        }
    }

    #[test]
    fn phase_five_procure_to_pay_routes_keep_the_shared_host_and_typed_owner_slots() {
        let id = uuid::Uuid::from_u128(0xf5);
        for record_type in [
            "procurement.requisition",
            "procurement.purchase-order",
            "procurement.receipt",
            "finance.vendor-invoice",
            "finance.payment",
        ] {
            let descriptor = erpx_integration_contracts::catalog::record_type(record_type)
                .expect("Phase 5 fixture stays current");
            let path = format!("/{}/{}/{id}", descriptor.module, descriptor.record);
            let route = AppRoute::from_str(&path).expect("canonical Phase 5 route parses");
            assert!(matches!(route, AppRoute::WorkspaceRecord { .. }));
            assert!(is_canonical_console_path(&path, ClientChannel::Web));
            assert!(
                CatalogRecord::try_from_path(&path)
                    .expect("catalog record")
                    .collaboration_available()
            );
        }

        let source = include_str!("routes.rs");
        let owner_router = source.find("fn record_owner_content").unwrap();
        let procurement = source[owner_router..]
            .find("Phase5ProcurementOwnerDetail")
            .unwrap();
        let customer_invoice = source[owner_router..]
            .find("Phase5CustomerInvoiceOwnerDetail")
            .unwrap();
        let vendor_invoice = source[owner_router..]
            .find("\"finance.vendor-invoice\"")
            .unwrap();
        let finance = source[owner_router..]
            .find("Phase5FinanceOwnerDetail")
            .unwrap();
        let host = source.find("pub fn RecordDetailHost").unwrap();
        let references = source[host..].find("RecordReferences {").unwrap();
        let activity = source[host..]
            .find("AuthorizedRecordActivityRail {")
            .unwrap();
        assert!(procurement < customer_invoice);
        assert!(customer_invoice < vendor_invoice);
        assert!(vendor_invoice < finance);
        assert!(references < activity);
        assert!(!source[owner_router..host].contains("RecordReferences {"));

        let customer_invoice_path = format!("/finance/invoice/{id}");
        assert!(is_canonical_console_path(
            &customer_invoice_path,
            ClientChannel::Web
        ));
        assert!(source.contains("CustomerInvoiceArOwnerSlot"));

        let vendor_invoice_path = format!("/finance/vendor-invoice/{id}");
        assert!(matches!(
            AppRoute::from_str(&vendor_invoice_path).expect("singular vendor-invoice route parses"),
            AppRoute::WorkspaceRecord { .. }
        ));
        assert!(is_canonical_console_path(
            &vendor_invoice_path,
            ClientChannel::Web
        ));
        let vendor_invoice = CatalogRecord::try_from_path(&vendor_invoice_path)
            .expect("vendor invoice is a live catalog record");
        assert_eq!(
            vendor_invoice.canonical_path(),
            vendor_invoice_path,
            "vendor invoice canonical route stays singular"
        );
        assert!(vendor_invoice.collaboration_available());
    }

    #[test]
    fn future_catalog_details_are_canonical_but_fail_closed_before_collaboration_mount() {
        use erpx_integration_contracts::catalog::{ContextLifecycle, RECORD_TYPES};

        let id = uuid::Uuid::from_u128(0xf00);
        let future = RECORD_TYPES
            .iter()
            .filter(|descriptor| descriptor.owner.lifecycle() != ContextLifecycle::Current)
            .collect::<Vec<_>>();
        assert!(!future.is_empty());
        for descriptor in future {
            let path = format!("/{}/{}/{id}", descriptor.module, descriptor.record);
            let record = CatalogRecord::try_from_path(&path).unwrap();
            assert!(!record.collaboration_available(), "{}", descriptor.id);
            assert!(
                is_canonical_console_path(&path, ClientChannel::Web),
                "{path}"
            );
        }

        let source = include_str!("routes.rs");
        let host = source.find("pub fn RecordDetailHost").unwrap();
        let fail_closed = source[host..]
            .find("if !record.collaboration_available()")
            .unwrap();
        let references = source[host..].find("RecordReferences {").unwrap();
        assert!(fail_closed < references);
    }

    #[test]
    fn process_route_cursor_and_presenter_copy_are_bounded_and_bilingual() {
        let route = AppRoute::from_str("/processes?cursor=offset:24")
            .expect("bounded process cursor route parses");
        assert_eq!(route_path(&route), "/processes");
        assert!(matches!(
            route,
            AppRoute::Processes { cursor: Some(value) } if value == "offset:24"
        ));
        assert_eq!(
            process_stage_label(
                crate::process_delivery::ProcessStage::RetryScheduled,
                Locale::En,
            ),
            "Retry scheduled"
        );
        assert_eq!(
            process_stage_label(
                crate::process_delivery::ProcessStage::RetryScheduled,
                Locale::Th,
            ),
            "กำหนดลองใหม่แล้ว"
        );
        assert!(
            process_contribution_message(
                erpx_ui_core::ReadModelAvailability::Unavailable,
                Locale::Th,
            )
            .contains("ไม่สามารถ")
        );
    }

    #[test]
    fn insight_presenter_copy_is_bilingual_and_source_owned() {
        assert_eq!(
            insight_metric_label(
                crate::insight_delivery::InsightMetricKind::Overdue,
                Locale::En,
            ),
            "Items beyond SLA"
        );
        assert_eq!(
            insight_metric_label(
                crate::insight_delivery::InsightMetricKind::Overdue,
                Locale::Th,
            ),
            "รายการที่เกิน SLA"
        );
        assert!(
            insight_source_message(erpx_ui_core::ReadModelAvailability::Unavailable, Locale::En,)
                .contains("no value")
        );
    }

    #[cfg(feature = "server")]
    #[test]
    fn unconnected_generic_routes_are_honest_empty_states() {
        let routes = Workspace::ALL.map(|workspace| RouteQuery::Workspace {
            workspace,
            resource: None,
        });

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
