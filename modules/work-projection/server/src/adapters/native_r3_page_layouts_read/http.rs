//! Owner-local binding for the R3 page-layout read. Composition supplies the
//! exact registered Holding target and the session/Company/navigation authority
//! facts; this adapter keeps the pool and SQL inside the owner and delegates
//! transport to the already-tested `page_layouts_http` router.

use crate::adapters::native_r3_page_layouts_read::application::{
    handle, PageLayoutsReadPort,
};
use crate::adapters::native_r3_page_layouts_read::domain::ReadQuery;
use crate::adapters::native_r3_page_layouts_read::postgres::PgPageLayoutRead;
use crate::adapters::page_layouts_http::{
    page_layouts_router, PageLayoutsCommandHttpRequest, PageLayoutsHttpBackend,
    PageLayoutsHttpFuture, PageLayoutsHttpResponse, PageLayoutsHttpTarget,
    PageLayoutsReadHttpRequest,
};
use erpx_platform_runtime_server::public::http_support::{HeaderMap, Router};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

pub type ContextFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition-resolved facts that reproduce the retained read context. None
/// of these are taken from the caller's query string or body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageLayoutsReadContext {
    pub company: String,
    pub user: String,
    pub base: String,
    pub allowed_widget_ids: BTreeSet<String>,
    pub can_publish: bool,
}

/// Composition resolves the current Account session, Holding work context,
/// navigation widget allow-list and Company authority from the request headers.
/// The owner read cannot be reached without a fact set from this trait.
pub trait PageLayoutsReadAuthority: Send + Sync {
    fn context<'a>(
        &'a self,
        host: &'a str,
        holding_slug: &'a str,
        page_key: &'a str,
        headers: &'a HeaderMap,
    ) -> ContextFuture<'a, PageLayoutsReadContext>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_LAYOUT_COMMAND_UNAVAILABLE",
        "The native Work layout command is not migrated.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PAGE_LAYOUTS_R3_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
}

/// One exact registered Holding target for the native read router.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

/// Read-only `PageLayoutsHttpBackend`. Authoritative facts come from
/// composition; SQL and projection stay in the owner. Commands fail closed:
/// the advisory-lock/receipt writer remains TypeScript-owned until its own
/// migration, and the lead must not point the POST path at this backend.
pub struct NativePageLayoutsBackend {
    read: Arc<dyn PageLayoutsReadPort>,
    authority: Arc<dyn PageLayoutsReadAuthority>,
}

impl NativePageLayoutsBackend {
    pub fn new(
        read: Arc<dyn PageLayoutsReadPort>,
        authority: Arc<dyn PageLayoutsReadAuthority>,
    ) -> Self {
        Self { read, authority }
    }
}

impl PageLayoutsHttpBackend for NativePageLayoutsBackend {
    fn read<'a>(
        &'a self,
        request: PageLayoutsReadHttpRequest<'a>,
    ) -> PageLayoutsHttpFuture<'a, PageLayoutsHttpResponse> {
        Box::pin(async move {
            let context = self
                .authority
                .context(
                    request.host,
                    request.holding_slug,
                    request.page_key,
                    request.headers,
                )
                .await?;
            let query = ReadQuery {
                company: context.company,
                page_key: request.page_key.to_owned(),
                user: context.user,
                base: context.base,
                allowed_widget_ids: context.allowed_widget_ids,
                can_publish: context.can_publish,
            };
            let data = handle(self.read.as_ref(), query).await?;
            Ok(PageLayoutsHttpResponse { data })
        })
    }

    fn command<'a>(
        &'a self,
        _request: PageLayoutsCommandHttpRequest<'a>,
    ) -> PageLayoutsHttpFuture<'a, PageLayoutsHttpResponse> {
        Box::pin(async { Err(unavailable()) })
    }
}

fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn PageLayoutsReadPort>)>,
    authority: Arc<dyn PageLayoutsReadAuthority>,
) -> Result<Router, BusinessError> {
    let mut configured = Vec::with_capacity(targets.len());
    let mut seen = BTreeSet::new();
    for (target, read) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || target.origin.is_empty()
            || !seen.insert((target.host.clone(), target.holding_slug.clone()))
        {
            return Err(configuration_invalid());
        }
        configured.push(PageLayoutsHttpTarget {
            host: target.host,
            holding_slug: target.holding_slug,
            origin: target.origin,
            backend: Arc::new(NativePageLayoutsBackend::new(read, authority.clone())),
        });
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    page_layouts_router(configured)
}

/// Build the native read router with one Holding-scoped Pg pool per exact
/// registered target. The lead mounts the returned Router; no composition or
/// root Cargo file is edited in this module.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
    authority: Arc<dyn PageLayoutsReadAuthority>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgPageLayoutRead::new(pool)) as Arc<dyn PageLayoutsReadPort>,
                )
            })
            .collect(),
        authority,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::page_layouts_read::PageLayoutRow;
    use erpx_platform_runtime_server::public::http_support::serve;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    struct Fixed {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl PageLayoutsReadPort for Fixed {
        fn read(&self, query: ReadQuery) -> crate::adapters::native_r3_page_layouts_read::application::ReadFuture<'_, Vec<PageLayoutRow>> {
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(format!(
                    "{}:{}:{}:{}",
                    query.company, query.user, query.page_key, query.base
                ));
                Ok(vec![
                    PageLayoutRow {
                        owner_id: "user-1".into(),
                        layout_json: Some(
                            r#"{"schemaVersion":1,"widgets":[{"id":"w2","columns":12,"height":null,"hidden":false},{"id":"gone","columns":6,"height":null,"hidden":false}]}"#
                                .into(),
                        ),
                        version: 2,
                    },
                    PageLayoutRow {
                        owner_id: "organization".into(),
                        layout_json: Some(
                            r#"{"schemaVersion":1,"widgets":[{"id":"w1","columns":6,"height":null,"hidden":false}]}"#
                                .into(),
                        ),
                        version: 5,
                    },
                ])
            })
        }
    }

    struct Authority;

    impl PageLayoutsReadAuthority for Authority {
        fn context<'a>(
            &'a self,
            _host: &'a str,
            _holding_slug: &'a str,
            _page_key: &'a str,
            _headers: &'a HeaderMap,
        ) -> ContextFuture<'a, PageLayoutsReadContext> {
            Box::pin(async {
                Ok(PageLayoutsReadContext {
                    company: "acme-co".into(),
                    user: "user-1".into(),
                    base: "/holdings/acme/people".into(),
                    allowed_widget_ids: BTreeSet::from(["w1".to_owned(), "w2".to_owned()]),
                    can_publish: true,
                })
            })
        }
    }

    fn target() -> LaneTarget {
        LaneTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
        }
    }

    fn router_with(seen: Arc<Mutex<Vec<String>>>) -> Router {
        let read: Arc<dyn PageLayoutsReadPort> = Arc::new(Fixed { seen });
        let authority: Arc<dyn PageLayoutsReadAuthority> = Arc::new(Authority);
        router_with_readers(vec![(target(), read)], authority).unwrap()
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let value = serde_json::from_str(body.trim()).unwrap_or(Value::Null);
        (status, head.to_owned(), value)
    }

    async fn spawn(router: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr, server)
    }

    fn get(host: &str, path: &str) -> String {
        format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: https://acme.jesadakorn.com\r\nConnection: close\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn native_get_returns_the_exact_data_envelope_and_headers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (addr, server) = spawn(router_with(seen.clone())).await;
        let (status, head, value) = http(
            addr,
            get(
                "acme.jesadakorn.com",
                "/api/v1/holdings/acme/page-layouts/people-directory",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert!(head.to_ascii_lowercase().contains("cache-control: private, no-store"));
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert_eq!(value["data"]["version"], 2);
        assert_eq!(value["data"]["organizationVersion"], 5);
        assert_eq!(value["data"]["canPublish"], true);
        assert_eq!(value["data"]["personal"]["widgets"].as_array().unwrap().len(), 1);
        assert_eq!(value["data"]["organization"]["widgets"][0]["id"], "w1");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            ["acme-co:user-1:people-directory:/holdings/acme/people"]
        );
        let _ = server.abort();
    }

    #[tokio::test]
    async fn unknown_page_query_and_wrong_host_fail_closed() {
        let (addr, server) = spawn(router_with(Arc::new(Mutex::new(Vec::new())))).await;
        assert_eq!(
            http(
                addr,
                get("acme.jesadakorn.com", "/api/v1/holdings/acme/page-layouts/platform-home"),
            )
            .await
            .0,
            404
        );
        assert_eq!(
            http(
                addr,
                get(
                    "acme.jesadakorn.com",
                    "/api/v1/holdings/acme/page-layouts/people-directory?x=1",
                ),
            )
            .await
            .0,
            422
        );
        assert_eq!(
            http(
                addr,
                get("other.jesadakorn.com", "/api/v1/holdings/acme/page-layouts/people-directory"),
            )
            .await
            .0,
            404
        );
        let _ = server.abort();
    }

    #[tokio::test]
    async fn post_is_not_served_by_the_read_module() {
        let (addr, server) = spawn(router_with(Arc::new(Mutex::new(Vec::new())))).await;
        let body = r#"{"action":"save","expectedVersion":2,"layout":null}"#;
        let raw = format!(
            "POST /api/v1/holdings/acme/page-layouts/people-directory HTTP/1.1\r\nHost: acme.jesadakorn.com\r\nOrigin: https://acme.jesadakorn.com\r\nContent-Type: application/json\r\nIdempotency-Key: wp-layout:save:1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (status, _, value) = http(addr, raw).await;
        assert_eq!(status, 503);
        assert_eq!(value["error"]["code"], "WORK_LAYOUT_COMMAND_UNAVAILABLE");
        let _ = server.abort();
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        assert!(router_with_readers(vec![], Arc::new(Authority)).is_err());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let read: Arc<dyn PageLayoutsReadPort> = Arc::new(Fixed { seen });
        let authority: Arc<dyn PageLayoutsReadAuthority> = Arc::new(Authority);
        assert!(router_with_readers(
            vec![(target(), read.clone()), (target(), read)],
            authority
        )
        .is_err());
        let mut bad = target();
        bad.host.clear();
        assert!(router_with_readers(vec![(bad, Arc::new(Fixed { seen: Arc::new(Mutex::new(Vec::new())) }))], Arc::new(Authority)).is_err());
    }

}
