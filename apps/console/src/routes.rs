#[cfg(feature = "server")]
use std::collections::BTreeSet;
#[cfg(any(feature = "web", feature = "server"))]
use std::time::Duration;
use std::{cell::RefCell, rc::Rc, str::FromStr};

use dioxus::prelude::*;
#[cfg(feature = "native")]
use erpx_delivery_contract::{BootstrapRequestV2, ConsoleGateway};
use erpx_delivery_contract::{
    ClientChannelV2, PublicMarketingPreviewRequestV2, QueryRequestV2, ResourceNameV2, RouteDataV2,
    RouteQueryV2, RouteUuidV2, SessionBootstrapV2, SessionCacheV2, WorkspaceV2,
};
#[cfg(feature = "server")]
use erpx_delivery_contract::{PageV2, ResourceSummaryV2};
use erpx_ui_kit::{
    Link as KitLink,
    button::{Button as KitButton, ButtonVariant},
    input::Input as KitInput,
    textarea::Textarea as KitTextarea,
};

#[cfg(any(feature = "web", feature = "server"))]
type RouteCacheV2 = Rc<RefCell<SessionCacheV2<String, RouteDataV2>>>;

#[rustfmt::skip]
#[derive(Clone, Debug, PartialEq, Routable)]
pub enum AppRoute {
    #[redirect("/", || AppRoute::Work {})]
    #[layout(PublicLayout)]
        #[route("/public/marketing/form/:id")]
        PublicMarketingForm { id: RouteUuidV2 },
        #[route("/public/marketing/preview/:id")]
        PublicMarketingPreview { id: RouteUuidV2 },
    #[end_layout]
    #[layout(AuthLayout)]
        #[route("/auth/sign-in")]
        SignIn {},
        #[route("/auth/accounts")]
        Accounts {},
        #[route("/auth/security")]
        Security {},
        #[route("/auth/devices")]
        Devices {},
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
        #[route("/reference/item/:id")]
        ReferenceItem { id: RouteUuidV2 },
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
            WorkspaceResource { workspace: WorkspaceV2, resource: ResourceNameV2 },
            #[route("/:workspace/:resource/:id")]
            WorkspaceRecord {
                workspace: WorkspaceV2,
                resource: ResourceNameV2,
                id: RouteUuidV2,
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
        let loader = use_loader(|| crate::delivery::session_bootstrap_v2("en".to_owned()))?;
        loader.read().clone()
    };

    #[cfg(feature = "native")]
    let session = {
        let gateway =
            use_context_provider(crate::native_delivery::NativeGrpcGatewayV2::from_environment);
        let gateway = gateway.clone().map_err(render_delivery_error)?;
        use_context_provider(|| gateway.clone());
        let session = use_resource(move || {
            let gateway = gateway.clone();
            async move {
                gateway
                    .bootstrap(BootstrapRequestV2 {
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
        Rc::new(RefCell::new(
            SessionCacheV2::<String, RouteDataV2>::default(),
        ))
    });
    cache.borrow_mut().set_scope(session.cache_scope());
    use_context_provider(|| session);

    rsx! {
        document::Stylesheet { href: asset!("/assets/console-surfaces.css") }
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

#[component]
fn PublicLayout() -> Element {
    rsx! {
        div { class: "public-layout",
            header { KitLink { href: "/", "ERPX" } }
            Outlet::<AppRoute> {}
        }
    }
}

#[component]
fn AuthLayout() -> Element {
    rsx! {
        main { class: "auth-layout",
            aside {
                p { "ERPX" }
                h1 { "Secure business operations" }
            }
            section { Outlet::<AppRoute> {} }
        }
    }
}

#[component]
fn ConsoleShell() -> Element {
    let session = use_context::<SessionBootstrapV2>();
    rsx! {
        div { class: "console-v2-shell",
            aside { class: "console-v2-nav",
                KitLink { href: "/work", "Work" }
                KitLink { href: "/processes", "Processes" }
                KitLink { href: "/insights", "Insights" }
                KitLink { href: "/modules", "Modules" }
                nav { aria_label: "Workspaces",
                    for workspace in WorkspaceV2::ALL {
                        if workspace.available_on(session.channel) {
                            KitLink { href: "/{workspace}", "{workspace}" }
                        }
                    }
                }
            }
            div { class: "console-v2-main",
                header { class: "console-v2-header",
                    span { "Delivery v{session.catalog_version}" }
                    span { "{session.locale}" }
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

#[derive(Store, Default)]
struct RouteUiStateV2 {
    search: String,
    selection: Option<RouteUuidV2>,
    command: CommandDraftV2,
}

#[derive(Store, Default)]
struct CommandDraftV2 {
    title: String,
    expected_version: Option<u64>,
}

#[component]
fn ProtectedRouteSurface(query: RouteQueryV2) -> Element {
    let ui = use_store(RouteUiStateV2::default);
    let mut search = ui.search();
    let selection = ui.selection();
    let command = ui.command();
    let mut command_title = command.title();
    let expected_version = command.expected_version();
    let normalized_search = use_memo(move || search.read().trim().to_ascii_lowercase());
    let data = use_route_data(query)?;
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

    rsx! {
        document::Title { "{data.title} · ERPX" }
        article { class: "workspace-route", "data-freshness": "{data.freshness_tag}",
            header {
                p { class: "eyebrow", "Canonical v2 route" }
                h1 { "{data.title}" }
                p { "{data.description}" }
            }
            label {
                span { "Filter this route" }
                KitInput {
                    value: "{search}",
                    oninput: move |event: FormEvent| search.set(event.value()),
                }
            }
            p { "{visible_items.len()} visible records" }
            section { class: "command-draft", aria_label: "Route command draft",
                label {
                    span { "Command title" }
                    KitInput {
                        value: "{command_title}",
                        oninput: move |event: FormEvent| command_title.set(event.value()),
                    }
                }
                p {
                    "Selection: {selection.read().is_some()} · expected version: {expected_version.read().map_or_else(|| \"new\".to_owned(), |value| value.to_string())}"
                }
            }
            if visible_items.is_empty() {
                section { class: "empty-state",
                    h2 { "No records in this view" }
                    p { "The typed loader returned no authorized rows for this route." }
                }
            } else {
                ul {
                    for item in visible_items {
                        li { key: "{item.id}",
                            strong { "{item.title}" }
                            span { "{item.status}" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn use_route_data(query: RouteQueryV2) -> Result<RouteDataV2, RenderError> {
    let cache = use_context::<RouteCacheV2>();
    let key = canonical_path_for_query(&query);
    let request = QueryRequestV2 {
        route: query,
        cursor: None,
        limit: 50,
        locale: "en".to_owned(),
    };
    let resource: Resource<Result<RouteDataV2, String>> = use_server_future(move || {
        let cache = Rc::clone(&cache);
        let key = key.clone();
        let request = request.clone();
        async move {
            if let Some(value) = cache.borrow_mut().get_fresh(&key) {
                return Ok::<RouteDataV2, String>(value);
            }
            let value = crate::delivery::query_route_v2(request)
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
            Ok::<RouteDataV2, String>(value)
        }
    })?;
    resource
        .read()
        .clone()
        .expect("resolved server future has a value")
        .map_err(|error| RenderError::from(std::io::Error::other(error)))
}

#[cfg(feature = "native")]
fn use_route_data(query: RouteQueryV2) -> Result<RouteDataV2, RenderError> {
    let gateway = use_context::<crate::native_delivery::NativeGrpcGatewayV2>();
    let resource = use_resource(move || {
        let gateway = gateway.clone();
        let query = query.clone();
        async move {
            gateway
                .query(QueryRequestV2 {
                    route: query,
                    cursor: None,
                    limit: 50,
                    locale: "en".to_owned(),
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
    rsx! { ProtectedRouteSurface { query: RouteQueryV2::Work } }
}

#[component]
fn Processes() -> Element {
    rsx! { ProtectedRouteSurface { query: RouteQueryV2::Processes } }
}

#[component]
fn Insights() -> Element {
    rsx! { ProtectedRouteSurface { query: RouteQueryV2::Insights } }
}

#[component]
fn Modules() -> Element {
    rsx! { ProtectedRouteSurface { query: RouteQueryV2::Modules } }
}

#[component]
fn WorkspaceResource(workspace: WorkspaceV2, resource: ResourceNameV2) -> Element {
    if !workspace_allowed(workspace) {
        return rsx! { ChannelRestricted { workspace } };
    }
    rsx! { ProtectedRouteSurface { query: RouteQueryV2::Workspace { workspace, resource: Some(resource) } } }
}

#[component]
fn WorkspaceRecord(workspace: WorkspaceV2, resource: ResourceNameV2, id: RouteUuidV2) -> Element {
    if !workspace_allowed(workspace) {
        return rsx! { ChannelRestricted { workspace } };
    }
    rsx! { ProtectedRouteSurface { query: RouteQueryV2::Record { workspace, resource, id } } }
}

macro_rules! workspace_entry_route {
    ($name:ident, $workspace:ident) => {
        #[component]
        fn $name() -> Element {
            let workspace = WorkspaceV2::$workspace;
            if !workspace_allowed(workspace) {
                return rsx! { ChannelRestricted { workspace } };
            }
            rsx! { ProtectedRouteSurface { query: RouteQueryV2::Workspace { workspace, resource: None } } }
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
fn ReferenceItem(id: RouteUuidV2) -> Element {
    rsx! { ProtectedRouteSurface { query: RouteQueryV2::Reference { id } } }
}

#[component]
fn ChannelRestricted(workspace: WorkspaceV2) -> Element {
    rsx! {
        section { class: "route-denied", role: "alert",
            h1 { "{workspace} is available on Web only" }
            p { "Native clients do not load this workspace chunk or service graph." }
        }
    }
}

#[component]
fn SignIn() -> Element {
    rsx! {
        document::Title { "Sign in · ERPX" }
        form { method: "post", action: "/api/v2/auth/password/form",
            h2 { "Sign in" }
            label { "Account" KitInput { name: "account", autocomplete: "username" } }
            label { "Password" KitInput { name: "password", r#type: "password", autocomplete: "current-password" } }
            KitInput { name: "return_path", r#type: "hidden", value: "/work" }
            KitButton { variant: ButtonVariant::Primary, r#type: "submit", "Continue" }
        }
    }
}

macro_rules! auth_page {
    ($name:ident, $title:literal) => {
        #[component]
        fn $name() -> Element {
            let title = concat!($title, " · ERPX");
            rsx! {
                document::Title { "{title}" }
                article { h2 { $title } p { "This security surface is session-scoped and no-store." } }
            }
        }
    };
}

auth_page!(Accounts, "Accounts");
auth_page!(Security, "Security");
auth_page!(Devices, "Devices");
auth_page!(Recovery, "Recovery");

#[component]
fn PublicMarketingForm(id: RouteUuidV2) -> Element {
    rsx! {
        document::Title { "Marketing form · ERPX" }
        document::Meta { name: "description", content: "Consent-aware ERPX marketing form" }
        article {
            h1 { "Marketing form" }
            p { "Form {id}" }
            form {
                label { "Name" KitInput { name: "name" } }
                label { "Email" KitInput { name: "email", r#type: "email" } }
                label { "Message" KitTextarea { name: "message" } }
                label { KitInput { name: "consent", r#type: "checkbox" } "I consent to this submission" }
                KitButton { variant: ButtonVariant::Primary, r#type: "submit", "Submit" }
            }
        }
    }
}

#[component]
fn PublicMarketingPreview(id: RouteUuidV2) -> Element {
    let request = PublicMarketingPreviewRequestV2 {
        preview_id: id,
        access_code: String::new(),
        locale: "en".to_owned(),
    };
    #[cfg(any(feature = "web", feature = "server"))]
    let data = {
        let loader =
            use_loader(move || crate::delivery::public_marketing_preview_v2(request.clone()))?;
        loader.read().clone().map_err(RenderError::from)?
    };
    #[cfg(feature = "native")]
    let data = {
        let gateway = use_context::<crate::native_delivery::NativeGrpcGatewayV2>();
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
        article { h1 { "{data.title}" } p { "{data.description}" } }
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    #[cfg(feature = "server")]
    dioxus::fullstack::FullstackContext::commit_http_status(
        dioxus::prelude::StatusCode::NOT_FOUND,
        None,
    );
    let attempted = format!("/{}", segments.join("/"));
    rsx! {
        document::Title { "Not found · ERPX" }
        main { class: "route-not-found",
            h1 { "Route not found" }
            p { "{attempted}" }
            KitLink { href: "/work", "Return to work" }
        }
    }
}

fn workspace_allowed(workspace: WorkspaceV2) -> bool {
    let session = use_context::<SessionBootstrapV2>();
    workspace.available_on(session.channel)
}

#[cfg(feature = "native")]
fn render_delivery_error(error: erpx_delivery_contract::DeliveryErrorV2) -> RenderError {
    RenderError::from(std::io::Error::other(error.to_string()))
}

pub fn is_canonical_console_path(path: &str, channel: ClientChannelV2) -> bool {
    if !path.starts_with('/') || path.contains(['?', '#']) || path != path.trim() {
        return false;
    }
    match AppRoute::from_str(path) {
        Ok(AppRoute::NotFound { .. }) | Err(_) => false,
        Ok(route @ (AppRoute::CommerceWorkspace {} | AppRoute::TalentWorkspace {})) => {
            route.to_string() == path && channel == ClientChannelV2::Web
        }
        Ok(
            route @ (AppRoute::WorkspaceResource { workspace, .. }
            | AppRoute::WorkspaceRecord { workspace, .. }),
        ) => route.to_string() == path && workspace.available_on(channel),
        Ok(route) => route.to_string() == path,
    }
}

#[cfg(feature = "server")]
pub(crate) fn route_data_for_query(request: &QueryRequestV2) -> RouteDataV2 {
    let canonical_path = canonical_path_for_query(&request.route);
    let title = match &request.route {
        RouteQueryV2::Work => "Work".to_owned(),
        RouteQueryV2::Processes => "Processes".to_owned(),
        RouteQueryV2::Insights => "Insights".to_owned(),
        RouteQueryV2::Modules => "Modules".to_owned(),
        RouteQueryV2::Workspace {
            workspace,
            resource,
        } => resource
            .as_ref()
            .map_or_else(|| workspace.to_string(), ToString::to_string),
        RouteQueryV2::Record { resource, id, .. } => format!("{resource} · {id}"),
        RouteQueryV2::Reference { id } => format!("Reference · {id}"),
    };
    let tag = match &request.route {
        RouteQueryV2::Workspace { workspace, .. } | RouteQueryV2::Record { workspace, .. } => {
            format!("workspace:{workspace}")
        }
        _ => format!(
            "route:{}",
            canonical_path.trim_start_matches('/').replace('/', ":")
        ),
    };
    RouteDataV2 {
        canonical_path,
        title,
        description:
            "Authorized route data is delivered once during SSR and reused during hydration."
                .to_owned(),
        page: PageV2 {
            items: Vec::<ResourceSummaryV2>::new(),
            next_cursor: None,
        },
        freshness_tag: "fresh".to_owned(),
        invalidation_tags: BTreeSet::from([tag]),
        public_validator: None,
    }
}

#[cfg(feature = "server")]
pub(crate) fn public_preview_data(request: &PublicMarketingPreviewRequestV2) -> RouteDataV2 {
    RouteDataV2 {
        canonical_path: format!("/public/marketing/preview/{}", request.preview_id),
        title: "Published marketing preview".to_owned(),
        description: "Public preview data uses a validator and an explicit cache policy."
            .to_owned(),
        page: PageV2 {
            items: Vec::new(),
            next_cursor: None,
        },
        freshness_tag: "public-validated".to_owned(),
        invalidation_tags: BTreeSet::from([format!("marketing:preview:{}", request.preview_id)]),
        public_validator: Some(format!("\"preview-{}-v2\"", request.preview_id)),
    }
}

#[cfg(any(feature = "web", feature = "server"))]
fn canonical_path_for_query(route: &RouteQueryV2) -> String {
    match route {
        RouteQueryV2::Work => "/work".to_owned(),
        RouteQueryV2::Processes => "/processes".to_owned(),
        RouteQueryV2::Insights => "/insights".to_owned(),
        RouteQueryV2::Modules => "/modules".to_owned(),
        RouteQueryV2::Workspace {
            workspace,
            resource,
        } => resource.as_ref().map_or_else(
            || format!("/{workspace}"),
            |resource| format!("/{workspace}/{resource}"),
        ),
        RouteQueryV2::Record {
            workspace,
            resource,
            id,
        } => format!("/{workspace}/{resource}/{id}"),
        RouteQueryV2::Reference { id } => format!("/reference/item/{id}"),
    }
}

#[cfg(test)]
fn workspace_home_route(workspace: WorkspaceV2) -> AppRoute {
    match workspace {
        WorkspaceV2::Operations => AppRoute::OperationsWorkspace {},
        WorkspaceV2::Finance => AppRoute::FinanceWorkspace {},
        WorkspaceV2::Sales => AppRoute::SalesWorkspace {},
        WorkspaceV2::Procurement => AppRoute::ProcurementWorkspace {},
        WorkspaceV2::Inventory => AppRoute::InventoryWorkspace {},
        WorkspaceV2::Asset => AppRoute::AssetWorkspace {},
        WorkspaceV2::Maintenance => AppRoute::MaintenanceWorkspace {},
        WorkspaceV2::Quality => AppRoute::QualityWorkspace {},
        WorkspaceV2::Marketing => AppRoute::MarketingWorkspace {},
        WorkspaceV2::People => AppRoute::PeopleWorkspace {},
        WorkspaceV2::Pos => AppRoute::PosWorkspace {},
        WorkspaceV2::Legal => AppRoute::LegalWorkspace {},
        WorkspaceV2::Manufacturing => AppRoute::ManufacturingWorkspace {},
        WorkspaceV2::Planning => AppRoute::PlanningWorkspace {},
        WorkspaceV2::Project => AppRoute::ProjectWorkspace {},
        WorkspaceV2::Commerce => AppRoute::CommerceWorkspace {},
        WorkspaceV2::Talent => AppRoute::TalentWorkspace {},
        WorkspaceV2::Learning => AppRoute::LearningWorkspace {},
        WorkspaceV2::Benefits => AppRoute::BenefitsWorkspace {},
        WorkspaceV2::FieldService => AppRoute::FieldServiceWorkspace {},
        WorkspaceV2::Ai => AppRoute::AiWorkspace {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_routes_round_trip_for_every_workspace_and_channel() {
        let id = RouteUuidV2::try_new(uuid::Uuid::new_v4()).unwrap();
        for workspace in WorkspaceV2::ALL {
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
                is_canonical_console_path(&home.to_string(), ClientChannelV2::MacOs),
                !workspace.is_web_only()
            );
        }
    }

    #[test]
    fn malformed_and_legacy_routes_have_no_compatibility_alias() {
        for path in [
            "/login",
            "/platform/customers",
            "/crm",
            "/payroll",
            "/commerce/",
            "/finance/orders/not-a-uuid",
            "/finance/PurchaseOrders",
        ] {
            assert!(
                !is_canonical_console_path(path, ClientChannelV2::Web),
                "{path}"
            );
        }
        assert!(is_canonical_console_path(
            "/auth/sign-in",
            ClientChannelV2::Web
        ));
    }

    #[test]
    fn canonical_uuid_and_reference_routes_round_trip() {
        let id = RouteUuidV2::try_new(uuid::Uuid::new_v4()).unwrap();
        let route = AppRoute::ReferenceItem { id };
        assert_eq!(
            AppRoute::from_str(&route.to_string()).expect("reference route parses"),
            route
        );
    }
}
