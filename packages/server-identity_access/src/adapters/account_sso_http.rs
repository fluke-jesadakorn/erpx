use crate::{
    adapters::account_http::{failure, one_header, presented},
    application::account_sso::AccountSso,
    domain::{account_auth::RequestAudience, account_sso::*},
    public::account_contracts::*,
};
use axum::{
    Router,
    extract::{Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use url::Url;
const BROWSER_COOKIE: &str = "__Host-erpx-sso";
pub fn account_sso_router(service: Arc<AccountSso>) -> Router {
    Router::new()
        .route("/sso/start", get(start))
        .route("/sso/authorize", get(authorize))
        .route("/sso/callback", get(callback))
        .layer(axum::middleware::map_response(private_response))
        .with_state(service)
}
async fn private_response(mut response: Response) -> Response {
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}
fn authority(service: &AccountSso, headers: &HeaderMap) -> Result<RequestAudience, BusinessError> {
    let host = one_header(headers, "host")?.ok_or_else(not_found)?;
    service
        .policy
        .account()
        .audience(host)
        .ok_or_else(not_found)
}
fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn query<T: DeserializeOwned>(uri: &axum::http::Uri) -> Result<T, BusinessError> {
    if uri.query().is_some_and(|value| value.len() > 8192) {
        return Err(rejected_sso());
    }
    Query::<T>::try_from_uri(uri)
        .map(|Query(input)| input)
        .map_err(|_| rejected_sso())
}
fn browser_cookie(headers: &HeaderMap) -> Result<SsoBrowserCookie, BusinessError> {
    let mut found = None;
    let mut size = 0;
    for header in headers.get_all(header::COOKIE) {
        let text = header.to_str().map_err(|_| rejected_sso())?;
        size += text.len();
        if size > 8192 {
            return Err(rejected_sso());
        }
        for part in text.split(';').map(str::trim) {
            if let Some(value) = part.strip_prefix(&format!("{BROWSER_COOKIE}=")) {
                if found.is_some() {
                    return Err(rejected_sso());
                }
                found = Some(SsoBrowserCookie::parse(value)?);
            }
        }
    }
    found.ok_or_else(rejected_sso)
}
fn redirect(url: Url) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(url.as_str()).expect("validated URL is a valid Location header"),
    );
    response
}
fn clear_browser(response: &mut Response) {
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "__Host-erpx-sso=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0",
        ),
    );
}
fn finish(
    result: Result<Response, BusinessError>,
    service: &AccountSso,
    audience: &RequestAudience,
) -> Response {
    match result {
        Ok(response) => response,
        Err(error) if error.code == "INVALID_SSO_TRANSACTION" => {
            let mut url = Url::parse(&service.policy.account().account_origin)
                .expect("validated Account origin");
            url.set_path("/login");
            url.query_pairs_mut().append_pair("reason", "sso_expired");
            if let RequestAudience::Holding { audience } = audience {
                url.query_pairs_mut().append_pair("app", audience);
            }
            let mut response = redirect(url);
            clear_browser(&mut response);
            response
        }
        Err(error) => failure(error),
    }
}
#[cfg_attr(feature="contracts",utoipa::path(get,path="/sso/start",params(("next"=Option<String>,Query,description="Holding-relative destination; credentials and fragments are removed")),responses((status=303,description="Exact Account authorization redirect and host-only browser/PKCE cookie"),(status=404),(status=503,body=AccountAuthErrorResponse))))]
async fn start(State(service): State<Arc<AccountSso>>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let audience = match authority(&service, &parts.headers) {
        Ok(value @ RequestAudience::Holding { .. }) => value,
        _ => return failure(not_found()),
    };
    let result = async {
        let input: AccountSsoStartInput = query(&parts.uri)?;
        let RequestAudience::Holding { audience: target } = &audience else {
            return Err(not_found());
        };
        let transaction = service.start(target, input.next.as_deref()).await?;
        let mut url =
            Url::parse(&service.policy.account().account_origin).expect("validated Account origin");
        url.set_path("/sso/authorize");
        url.query_pairs_mut()
            .append_pair("transaction", transaction.transaction_id.as_str())
            .append_pair("state", transaction.state.expose())
            .append_pair("challenge", transaction.challenge.as_str());
        let mut response = redirect(url);
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&format!(
                "{BROWSER_COOKIE}={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=300",
                transaction.browser_cookie.cookie_value()
            ))
            .expect("generated browser proof uses safe ASCII"),
        );
        Ok(response)
    }
    .await;
    finish(result, &service, &audience)
}
#[cfg_attr(feature="contracts",utoipa::path(get,path="/sso/authorize",params(("transaction"=String,Query),("state"=String,Query),("challenge"=String,Query)),responses((status=303,description="Current Account session grants an exact Holding callback; anonymous users continue through Account login"),(status=404),(status=503,body=AccountAuthErrorResponse))))]
async fn authorize(State(service): State<Arc<AccountSso>>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let audience = match authority(&service, &parts.headers) {
        Ok(RequestAudience::Account) => RequestAudience::Account,
        _ => return failure(not_found()),
    };
    let result = async {
        let input: AccountSsoAuthorizeInput = query(&parts.uri)?;
        let id = SsoTransactionId::new(input.transaction)?;
        let state = SsoSecret::new(input.state)?;
        let challenge = SsoDigest::new(input.challenge)?;
        let proof = presented(&parts.headers, &RequestAudience::Account)?;
        match service
            .authorize(id.clone(), &state, challenge.clone(), &proof)
            .await?
        {
            Some(grant) => {
                let mut url =
                    Url::parse(&grant.callback).expect("repository validated exact callback");
                url.query_pairs_mut()
                    .append_pair("code", grant.code.expose())
                    .append_pair("state", state.expose());
                Ok(redirect(url))
            }
            None => {
                let mut url = Url::parse(&service.policy.account().account_origin)
                    .expect("validated Account origin");
                url.set_path("/login");
                url.query_pairs_mut()
                    .append_pair("sso", id.as_str())
                    .append_pair("state", state.expose())
                    .append_pair("challenge", challenge.as_str());
                Ok(redirect(url))
            }
        }
    }
    .await;
    finish(result, &service, &audience)
}
#[cfg_attr(feature="contracts",utoipa::path(get,path="/sso/callback",params(("code"=String,Query),("state"=String,Query)),responses((status=303,description="One atomic grant consumption, host-only Holding session cookie, and clean Holding-relative redirect"),(status=404),(status=503,body=AccountAuthErrorResponse))))]
async fn callback(State(service): State<Arc<AccountSso>>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let audience = match authority(&service, &parts.headers) {
        Ok(value @ RequestAudience::Holding { .. }) => value,
        _ => return failure(not_found()),
    };
    let result = async {
        let input: AccountSsoCallbackInput = query(&parts.uri)?;
        let code = SsoSecret::new(input.code)?;
        let state = SsoSecret::new(input.state)?;
        let browser = browser_cookie(&parts.headers)?;
        let RequestAudience::Holding { audience: target } = &audience else {
            return Err(not_found());
        };
        let issued = service.consume(target, &code, &state, browser).await?;
        let holding = service.policy.holding(target)?;
        let url = Url::parse(&holding.origin)
            .expect("validated Holding origin")
            .join(issued.result.return_path.as_str())
            .expect("canonical relative return");
        let expires = time::OffsetDateTime::from_unix_timestamp_nanos(
            i128::from(issued.result.expires_at) * 1_000_000,
        )
        .map_err(|_| rejected_sso())?
        .format(time::macros::format_description!(
            "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT"
        ))
        .map_err(|_| rejected_sso())?;
        let mut response = redirect(url);
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&format!(
                "__Host-erpx-product={}; Path=/; Secure; HttpOnly; SameSite=Lax; Expires={expires}",
                issued.token.expose()
            ))
            .expect("generated Holding secret uses safe ASCII"),
        );
        clear_browser(&mut response);
        Ok(response)
    }
    .await;
    finish(result, &service, &audience)
}
#[cfg(feature = "contracts")]
pub fn sso_openapi() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    #[derive(OpenApi)]
    #[openapi(
        paths(start, authorize, callback),
        components(schemas(
            AccountSsoStartInput,
            AccountSsoAuthorizeInput,
            AccountSsoCallbackInput
        ))
    )]
    struct SsoApi;
    SsoApi::openapi()
}
