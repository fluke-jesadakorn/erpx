//! Owner-local axum ingress for the native public Marketing form source read.
//!
//! Transport facts only: a registered Host/Holding target and a bounded form
//! id. No SQL is accepted from the wire; the injected pool-backed owner source
//! owns the statement and projection. Failures map to the retained statuses and
//! error codes with no TypeScript fallback.

use crate::adapters::native_public_form::application::{PublicFormSource, read};
use crate::adapters::native_public_form::domain::{FormQuery, invalid};
use crate::adapters::native_public_form::postgres::PgPublicFormSource;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    get, header, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

pub struct PublicFormTarget {
    pub host: String,
    pub holding_slug: String,
}

struct FormState {
    targets: BTreeMap<(String, String), Arc<dyn PublicFormSource>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_PUBLIC_FORM_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

impl FormState {
    fn with_readers(
        entries: Vec<(PublicFormTarget, Arc<dyn PublicFormSource>)>,
    ) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for (target, reader) in entries {
            if target.host.is_empty()
                || !valid_slug(&target.holding_slug)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), reader)
                    .is_some()
            {
                return Err(configuration_invalid());
            }
        }
        if configured.is_empty() {
            return Err(configuration_invalid());
        }
        Ok(Self {
            targets: configured,
        })
    }

    async fn source(&self, headers: &HeaderMap, slug: &str, form_id: &str) -> Response {
        match self.source_inner(headers, slug, form_id).await {
            Ok(row) => bounded(serde_json::json!({"data": row})),
            Err(error) => error_response(error),
        }
    }

    async fn source_inner(
        &self,
        headers: &HeaderMap,
        slug: &str,
        form_id: &str,
    ) -> Result<serde_json::Value, BusinessError> {
        let host = unique_header(headers, header::HOST.as_str())?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let source = self
            .targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)?;
        read(
            source.as_ref(),
            &FormQuery {
                form_id: form_id.to_owned(),
            },
        )
        .await
        .map_err(|error| {
            if matches!(error.kind, FailureKind::Unavailable) {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "MARKETING_PUBLIC_FORM_UNAVAILABLE",
                    "The native Marketing form service is unavailable.",
                )
            } else {
                error
            }
        })
    }
}

async fn source(
    State(state): State<Arc<FormState>>,
    Path((slug, form_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    state.source(&headers, &slug, &form_id).await
}

/// Build the native source router with one pool shared by every target.
pub fn router(pool: DatabasePool, targets: Vec<PublicFormTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgPublicFormSource::new(pool)), targets)
}

/// Router over one injected owner source, kept for single-source tests.
pub fn router_with_reader(
    source: Arc<dyn PublicFormSource>,
    targets: Vec<PublicFormTarget>,
) -> Result<Router, BusinessError> {
    router_with_sources(
        targets
            .into_iter()
            .map(|target| (target, source.clone()))
            .collect(),
    )
}

/// Router over one owner source per registered Holding. Composition supplies
/// a distinct `DatabasePool` for each Holding; no pool is shared by mistake.
pub fn router_for_holdings(
    targets: Vec<(PublicFormTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_sources(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgPublicFormSource::new(pool)) as Arc<dyn PublicFormSource>,
                )
            })
            .collect(),
    )
}

/// Injection path for the per-target map; no pool or SQL is created here.
pub fn router_with_sources(
    entries: Vec<(PublicFormTarget, Arc<dyn PublicFormSource>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/marketing/public-form/{form_id}/source",
            get(source),
        )
        .with_state(Arc::new(FormState::with_readers(entries)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_or_duplicate_target_configuration() {
        let target = || PublicFormTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        struct Noop;
        impl PublicFormSource for Noop {
            fn form<'a>(&'a self, _: &'a str) -> crate::adapters::native_public_form::application::PublicFormFuture<'a, Option<crate::adapters::native_public_form::domain::FormRow>> {
                Box::pin(async { Ok(None) })
            }
        }
        let source = Arc::new(Noop) as Arc<dyn PublicFormSource>;
        assert!(FormState::with_readers(vec![(target(), source.clone())]).is_ok());
        assert!(FormState::with_readers(vec![]).is_err());
        assert!(FormState::with_readers(vec![(target(), source.clone()), (target(), source)]).is_err());
        assert!(valid_slug("acme-dev_1"));
        assert!(!valid_slug("Acme"));
    }
}
