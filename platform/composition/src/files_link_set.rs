//! Adapt the generic Files route to the existing Projects authorization boundary.
use std::{collections::BTreeMap, sync::Arc};

use erpx_files_server::public::native_r3_link_set::{
    LinkRow, LinkSetFuture, LinkSetRead, LinkSetRequest,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::get,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use erpx_projects_server::public::project_attachment_read_http::{
    ProjectAttachmentReadHttpBackend, ProjectAttachmentReadLinksBySetHttpRequest,
};

pub struct ProjectLinkSetTarget {
    pub host: String,
    pub holding_slug: String,
    pub backend: Arc<dyn ProjectAttachmentReadHttpBackend>,
}

pub struct ProjectLinkSetReader {
    targets: BTreeMap<(String, String), Arc<dyn ProjectAttachmentReadHttpBackend>>,
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

impl ProjectLinkSetReader {
    pub fn new(targets: Vec<ProjectLinkSetTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || !erpx_files_server::public::native_r3_link_set::valid_slug(&target.holding_slug)
                || configured
                    .insert((target.host, target.holding_slug), target.backend)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "FILES_LINK_SET_ROUTE_CONFIG_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        if configured.is_empty() {
            return Err(BusinessError::invalid(
                "FILES_LINK_SET_ROUTE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
        Ok(Self {
            targets: configured,
        })
    }
}

impl LinkSetRead for ProjectLinkSetReader {
    fn links<'a>(&'a self, request: LinkSetRequest<'a>) -> LinkSetFuture<'a, Vec<LinkRow>> {
        Box::pin(async move {
            let backend = self
                .targets
                .get(&(request.host.to_owned(), request.holding_slug.to_owned()))
                .ok_or_else(not_found)?;
            let reply = backend
                .links_by_set(ProjectAttachmentReadLinksBySetHttpRequest {
                    host: request.host,
                    holding_slug: request.holding_slug,
                    headers: request.headers,
                    set_id: request.set_id,
                })
                .await
                .map_err(|error| match error.code {
                    "ATTACHMENT_SET_NOT_FOUND" | "ATTACHMENT_LINK_NOT_FOUND" => not_found(),
                    _ => error,
                })?;
            serde_json::from_value(reply.data).map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "ATTACHMENT_READ_INVALID",
                    "The file metadata could not be read safely.",
                )
            })
        })
    }
}

/// Mount only after real-session and cross-owner denial verification.
pub fn router(targets: Vec<ProjectLinkSetTarget>) -> Result<Router, BusinessError> {
    let reader = Arc::new(ProjectLinkSetReader::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/attachment-sets/{set_id}/links",
            get(links),
        )
        .with_state(reader))
}

async fn links(
    State(reader): State<Arc<ProjectLinkSetReader>>,
    Path((slug, set_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        let host = one_host(&headers)?;
        if !erpx_files_server::public::native_r3_link_set::valid_set_id(&set_id) {
            return Err(not_found());
        }
        if request.uri().query().is_some_and(|query| !query.is_empty())
            || !to_bytes(request.into_body(), 1)
                .await
                .map_err(|_| malformed())?
                .is_empty()
        {
            return Err(malformed());
        }
        let rows = reader
            .links(LinkSetRequest {
                host: &host,
                holding_slug: &slug,
                headers: &headers,
                set_id: &set_id,
            })
            .await?;
        erpx_files_server::public::native_r3_link_set::handle(rows)
    }
    .await;
    match result {
        Ok(body) => {
            let mut response = Json(body).into_response();
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            response
        }
        Err(error) => {
            let status = match error.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (
                status,
                Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
            )
                .into_response()
        }
    }
}

fn malformed() -> BusinessError {
    BusinessError::invalid("ATTACHMENT_REQUEST_INVALID", "Review the file request.")
}

fn one_host(headers: &HeaderMap) -> Result<String, BusinessError> {
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts
        .next()
        .ok_or_else(not_found)?
        .to_str()
        .map_err(|_| malformed())?;
    if host.is_empty() || hosts.next().is_some() {
        return Err(not_found());
    }
    Ok(host.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_projects_server::public::project_attachment_read_http::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Backend(AtomicUsize);
    macro_rules! unused_method {
        ($name:ident, $request:ty, $reply:ty) => {
            fn $name<'a>(&'a self, _: $request) -> ProjectAttachmentReadHttpFuture<'a, $reply> {
                Box::pin(async { Err(not_found()) })
            }
        };
    }
    impl ProjectAttachmentReadHttpBackend for Backend {
        unused_method!(
            sets,
            ProjectAttachmentReadSetsHttpRequest<'a>,
            ProjectAttachmentReadHttpResponse
        );
        unused_method!(
            links,
            ProjectAttachmentReadLinksHttpRequest<'a>,
            ProjectAttachmentReadHttpResponse
        );
        unused_method!(
            status,
            ProjectAttachmentReadStatusHttpRequest<'a>,
            ProjectAttachmentReadHttpResponse
        );
        unused_method!(
            content,
            ProjectAttachmentReadContentHttpRequest<'a>,
            ProjectAttachmentContentHttpResponse
        );
        unused_method!(
            status_by_link,
            ProjectAttachmentReadStatusByLinkHttpRequest<'a>,
            ProjectAttachmentReadHttpResponse
        );
        unused_method!(
            content_by_link,
            ProjectAttachmentReadContentByLinkHttpRequest<'a>,
            ProjectAttachmentContentHttpResponse
        );
        fn links_by_set<'a>(
            &'a self,
            request: ProjectAttachmentReadLinksBySetHttpRequest<'a>,
        ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                assert_eq!(request.headers.get("cookie").unwrap(), "session=proof");
                if request.set_id == "denied" {
                    return Err(BusinessError::new(
                        FailureKind::NotFound,
                        "ATTACHMENT_SET_NOT_FOUND",
                        "hidden",
                    ));
                }
                Ok(ProjectAttachmentReadHttpResponse {
                    data: serde_json::json!([]),
                    context: serde_json::json!({}),
                })
            })
        }
    }

    #[tokio::test]
    async fn delegates_to_matching_authority_and_hides_denials() {
        let backend = Arc::new(Backend(AtomicUsize::new(0)));
        let reader = ProjectLinkSetReader::new(vec![ProjectLinkSetTarget {
            host: "acme.example".into(),
            holding_slug: "acme".into(),
            backend: backend.clone(),
        }])
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("session=proof"));
        let request = |host, holding_slug, set_id| LinkSetRequest {
            host,
            holding_slug,
            headers: &headers,
            set_id,
        };
        assert!(
            reader
                .links(request("acme.example", "acme", "ok"))
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            reader
                .links(request("acme.example", "acme", "denied"))
                .await
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
        assert_eq!(
            reader
                .links(request("other.example", "acme", "ok"))
                .await
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
        assert_eq!(backend.0.load(Ordering::SeqCst), 2);
    }
}
