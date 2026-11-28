//! SQL-free ports and handlers for the native Legal portal slice.
//!
//! The verified external actor, the Holding binding and the session-change
//! assertion are composition facts; this owner never mints a session. Pins are
//! selectors only: available ids are read from the same share list the portal
//! shows, and are handed to the existing quick-access save owner unchanged.

use std::{future::Future, pin::Pin};

use erpx_platform_runtime_server::public::http_support::HeaderMap;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

use super::domain::{
    BODY_LIMIT, QUICK_ACCESS_SURFACE, QuickAccessBody, QuickAccessPreferenceWire, PortalShareWire,
    valid_body, valid_key, valid_predicate, valid_slug,
};

pub type PortalFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn key_required() -> BusinessError {
    invalid("COMMAND_KEY_REQUIRED", "A command key is required.")
}

fn invalid_json() -> BusinessError {
    invalid("INVALID_JSON", "Invalid JSON.")
}

fn invalid_shortcuts() -> BusinessError {
    invalid("INVALID_SHORTCUTS", "Choose up to six unique shortcuts.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_PORTAL_UNAVAILABLE",
        "The native Legal portal is unavailable.",
    )
}

/// The actor verified by the external Portal sign-in. `email` is lowercased and
/// `email_verified` was checked before this value was created.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalActor {
    pub user_id: String,
    pub email: String,
    pub session_id: String,
}

/// Binding-scoped facts supplied by composition after session verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalVerified {
    pub actor: PortalActor,
    pub context_key: String,
}

pub struct PortalVerifyRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub trait PortalAuthority: Send + Sync {
    fn verify<'a>(
        &'a self,
        request: PortalVerifyRequest<'a>,
    ) -> PortalFuture<'a, PortalVerified>;
}

pub trait PortalSource: Send + Sync {
    /// The exact share-list SELECT, ordered as the retired owner ordered it.
    fn shares<'a>(
        &'a self,
        actor: &'a PortalActor,
        predicate: &'a str,
    ) -> PortalFuture<'a, Vec<PortalShareWire>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalSaveInput {
    pub user_id: String,
    pub surface: String,
    pub context_key: String,
    pub pinned_ids: Option<Vec<String>>,
    pub expected_version: i64,
    pub available_ids: Vec<String>,
    pub request_key: String,
}

/// The existing quick-access preference owner (`quick_access_preferences.rs`).
/// Composition supplies its reader/writer; no preference SQL is duplicated here.
pub trait PortalPreferences: Send + Sync {
    fn read<'a>(
        &'a self,
        user_id: &'a str,
        context_key: &'a str,
    ) -> PortalFuture<'a, QuickAccessPreferenceWire>;
    fn save<'a>(
        &'a self,
        input: &'a PortalSaveInput,
    ) -> PortalFuture<'a, QuickAccessPreferenceWire>;
}

pub struct PortalReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct PortalSaveRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub body: &'a [u8],
    pub key: &'a str,
}

async fn verified<A: PortalAuthority + ?Sized>(
    authority: &A,
    request: &PortalReadRequest<'_>,
) -> Result<PortalVerified, BusinessError> {
    if !valid_slug(request.holding_slug) {
        return Err(not_found());
    }
    let verified = authority
        .verify(PortalVerifyRequest {
            host: request.host,
            holding_slug: request.holding_slug,
            headers: request.headers,
        })
        .await?;
    if verified.actor.user_id.is_empty()
        || verified.actor.email.is_empty()
        || verified.actor.session_id.is_empty()
        || verified.context_key.is_empty()
    {
        return Err(unavailable());
    }
    Ok(verified)
}

/// The retired Zod schema requires both keys, allows `pinnedIds: null` and
/// rejects unknown fields. Serde treats a missing `Option` as `None`, so the
/// key presence is checked explicitly before deserializing.
fn parse_body(bytes: &[u8]) -> Result<QuickAccessBody, BusinessError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid_json())?;
    let object = value.as_object().ok_or_else(invalid_json)?;
    if !object.contains_key("pinnedIds") || !object.contains_key("expectedVersion") {
        return Err(invalid_json());
    }
    serde_json::from_value(value).map_err(|_| invalid_json())
}

/// `GET /api/legal-portal/{holdingSlug}` with no path.
pub async fn portal_list<A: PortalAuthority + ?Sized, S: PortalSource + ?Sized>(
    authority: &A,
    source: &S,
    predicate: &str,
    request: PortalReadRequest<'_>,
) -> Result<Vec<PortalShareWire>, BusinessError> {
    if !valid_predicate(predicate) {
        return Err(unavailable());
    }
    let verified = verified(authority, &request).await?;
    source.shares(&verified.actor, predicate).await
}

/// `GET /api/legal-portal/{holdingSlug}/quick-access`.
pub async fn portal_quick_access_get<A: PortalAuthority + ?Sized, P: PortalPreferences + ?Sized>(
    authority: &A,
    preferences: &P,
    request: PortalReadRequest<'_>,
) -> Result<QuickAccessPreferenceWire, BusinessError> {
    let verified = verified(authority, &request).await?;
    preferences
        .read(&verified.actor.user_id, &verified.context_key)
        .await
}

/// `POST /api/legal-portal/{holdingSlug}/quick-access`.
pub async fn portal_quick_access_post<
    A: PortalAuthority + ?Sized,
    S: PortalSource + ?Sized,
    P: PortalPreferences + ?Sized,
>(
    authority: &A,
    source: &S,
    preferences: &P,
    predicate: &str,
    request: PortalSaveRequest<'_>,
) -> Result<QuickAccessPreferenceWire, BusinessError> {
    if !valid_predicate(predicate) {
        return Err(unavailable());
    }
    if !valid_key(request.key) {
        return Err(key_required());
    }
    if request.body.len() > BODY_LIMIT {
        return Err(invalid("INPUT_TOO_LARGE", "Shorten the content before saving."));
    }
    let body = parse_body(request.body)?;
    if !valid_body(&body) {
        return Err(invalid_shortcuts());
    }
    let verified = verified(
        authority,
        &PortalReadRequest {
            host: request.host,
            holding_slug: request.holding_slug,
            headers: request.headers,
        },
    )
    .await?;
    let shares = source.shares(&verified.actor, predicate).await?;
    let input = PortalSaveInput {
        user_id: verified.actor.user_id,
        surface: QUICK_ACCESS_SURFACE.to_owned(),
        context_key: verified.context_key,
        pinned_ids: body.pinned_ids,
        expected_version: body.expected_version,
        available_ids: shares.iter().map(PortalShareWire::pin_id).collect(),
        request_key: request.key.to_owned(),
    };
    preferences.save(&input).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Authority {
        seen: Mutex<Vec<String>>,
    }

    impl PortalAuthority for Authority {
        fn verify<'a>(
            &'a self,
            request: PortalVerifyRequest<'a>,
        ) -> PortalFuture<'a, PortalVerified> {
            self.seen
                .lock()
                .unwrap()
                .push(format!("{}:{}", request.host, request.holding_slug));
            Box::pin(async move {
                Ok(PortalVerified {
                    actor: PortalActor {
                        user_id: "user-1".into(),
                        email: "reader@example.test".into(),
                        session_id: "session-1".into(),
                    },
                    context_key: "holding-1".into(),
                })
            })
        }
    }

    struct Source {
        shares: Vec<PortalShareWire>,
        seen: Mutex<Vec<String>>,
    }

    impl PortalSource for Source {
        fn shares<'a>(
            &'a self,
            actor: &'a PortalActor,
            predicate: &'a str,
        ) -> PortalFuture<'a, Vec<PortalShareWire>> {
            self.seen
                .lock()
                .unwrap()
                .push(format!("{}:{}", actor.email, predicate));
            let shares = self.shares.clone();
            Box::pin(async move { Ok(shares) })
        }
    }

    #[derive(Default)]
    struct Preferences {
        saved: Mutex<Vec<PortalSaveInput>>,
    }

    impl PortalPreferences for Preferences {
        fn read<'a>(
            &'a self,
            user_id: &'a str,
            context_key: &'a str,
        ) -> PortalFuture<'a, QuickAccessPreferenceWire> {
            let id = format!("{user_id}:{context_key}");
            Box::pin(async move {
                Ok(QuickAccessPreferenceWire {
                    pinned_ids: None,
                    version: id.len() as i64,
                })
            })
        }
        fn save<'a>(
            &'a self,
            input: &'a PortalSaveInput,
        ) -> PortalFuture<'a, QuickAccessPreferenceWire> {
            self.saved.lock().unwrap().push(input.clone());
            let version = input.expected_version + 1;
            Box::pin(async move {
                Ok(QuickAccessPreferenceWire {
                    pinned_ids: input.pinned_ids.clone(),
                    version,
                })
            })
        }
    }

    fn share(id: &str) -> PortalShareWire {
        PortalShareWire {
            id: id.into(),
            document_id: "doc".into(),
            expires_at: 9,
            user_id: None,
            name: "Name".into(),
            version: 1,
            checksum: "sum".into(),
        }
    }

    fn headers() -> HeaderMap {
        HeaderMap::new()
    }

    #[tokio::test]
    async fn list_binds_the_verified_actor_to_the_owner_select() {
        let authority = Authority { seen: Mutex::new(Vec::new()) };
        let source = Source {
            shares: vec![share("s1")],
            seen: Mutex::new(Vec::new()),
        };
        let rows = portal_list(
            &authority,
            &source,
            "TRUE",
            PortalReadRequest {
                host: "legal.example.test",
                holding_slug: "acme",
                headers: &headers(),
            },
        )
        .await
        .unwrap();
        assert_eq!(rows, vec![share("s1")]);
        assert_eq!(
            authority.seen.lock().unwrap().as_slice(),
            ["legal.example.test:acme"]
        );
        assert_eq!(source.seen.lock().unwrap().as_slice(), ["reader@example.test:TRUE"]);
    }

    #[tokio::test]
    async fn save_derives_available_ids_from_the_same_authorized_list() {
        let authority = Authority { seen: Mutex::new(Vec::new()) };
        let source = Source {
            shares: vec![share("s1"), share("s2")],
            seen: Mutex::new(Vec::new()),
        };
        let preferences = Preferences::default();
        let wire = portal_quick_access_post(
            &authority,
            &source,
            &preferences,
            "TRUE",
            PortalSaveRequest {
                host: "legal.example.test",
                holding_slug: "acme",
                headers: &headers(),
                body: br#"{"pinnedIds":["share:s2"],"expectedVersion":4}"#,
                key: "key-1",
            },
        )
        .await
        .unwrap();
        let saved = preferences.saved.lock().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].surface, "legal_portal");
        assert_eq!(saved[0].context_key, "holding-1");
        assert_eq!(saved[0].expected_version, 4);
        assert_eq!(saved[0].available_ids, ["share:s1", "share:s2"]);
        assert_eq!(saved[0].request_key, "key-1");
        assert_eq!(
            serde_json::to_value(&wire).unwrap(),
            serde_json::json!({"pinnedIds": ["share:s2"], "version": 5})
        );
    }

    #[tokio::test]
    async fn guards_fail_before_any_owner_read_or_write() {
        let authority = Authority { seen: Mutex::new(Vec::new()) };
        let source = Source {
            shares: vec![share("s1")],
            seen: Mutex::new(Vec::new()),
        };
        let preferences = Preferences::default();
        fn read<'a>(slug: &'a str, hs: &'a HeaderMap) -> PortalReadRequest<'a> {
            PortalReadRequest {
                host: "legal.example.test",
                holding_slug: slug,
                headers: hs,
            }
        }
        let hs = headers();
        let cases = [
            portal_list(&authority, &source, "TRUE OR 1=1", read("acme", &hs)).await.unwrap_err().code,
            portal_list(&authority, &source, "TRUE", read("Acme", &hs)).await.unwrap_err().code,
            portal_quick_access_post(
                &authority,
                &source,
                &preferences,
                "TRUE",
                PortalSaveRequest {
                    host: "legal.example.test",
                    holding_slug: "acme",
                    headers: &headers(),
                    body: br#"{"pinnedIds":null,"expectedVersion":0}"#,
                    key: "",
                },
            )
            .await
            .unwrap_err()
            .code,
            portal_quick_access_post(
                &authority,
                &source,
                &preferences,
                "TRUE",
                PortalSaveRequest {
                    host: "legal.example.test",
                    holding_slug: "acme",
                    headers: &headers(),
                    body: b"{bad",
                    key: "key-1",
                },
            )
            .await
            .unwrap_err()
            .code,
            portal_quick_access_post(
                &authority,
                &source,
                &preferences,
                "TRUE",
                PortalSaveRequest {
                    host: "legal.example.test",
                    holding_slug: "acme",
                    headers: &headers(),
                    body: br#"{"expectedVersion":0}"#,
                    key: "key-1",
                },
            )
            .await
            .unwrap_err()
            .code,
        ];
        assert_eq!(cases[0], "LEGAL_PORTAL_UNAVAILABLE");
        assert_eq!(cases[1], "NOT_FOUND");
        assert_eq!(cases[2], "COMMAND_KEY_REQUIRED");
        assert_eq!(cases[3], "INVALID_JSON");
        assert_eq!(cases[4], "INVALID_JSON");
        assert!(authority.seen.lock().unwrap().is_empty());
        assert!(source.seen.lock().unwrap().is_empty());
        assert!(preferences.saved.lock().unwrap().is_empty());
    }
}
