//! Client-local authentication session state.
//!
//! The server remains the authorization authority. This module only owns the
//! in-memory lifecycle needed by a Dioxus boundary and exposes display-safe
//! snapshots to the shell. Tokens and password input are deliberately absent
//! from serializable/UI metadata and have redacted debug output.

use std::{fmt, future::Future, pin::Pin};

use crate::{RouteState, SafeSessionMetadata, SafeUiText, redact_deep_link};

/// A future returned by an application-provided HTTP authentication adapter.
pub type SessionFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AuthTransportError>> + 'a>>;

/// Non-sensitive transport failures that the session boundary can classify.
#[derive(Clone, PartialEq, Eq)]
pub enum AuthTransportError {
    /// The server rejected or revoked the presented credentials.
    Unauthorized,
    /// The backend could not be reached.
    Offline,
    /// The request was malformed or rejected before authentication.
    InvalidRequest,
    /// A safe, non-secret server error code.
    Server { code: String },
}

impl fmt::Debug for AuthTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => formatter.write_str("AuthTransportError::Unauthorized"),
            Self::Offline => formatter.write_str("AuthTransportError::Offline"),
            Self::InvalidRequest => formatter.write_str("AuthTransportError::InvalidRequest"),
            Self::Server { .. } => formatter.write_str("AuthTransportError::Server(REDACTED)"),
        }
    }
}

impl fmt::Display for AuthTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unauthorized => "the server rejected the session",
            Self::Offline => "the authentication service is unavailable",
            Self::InvalidRequest => "the authentication request was invalid",
            // The server code is transport metadata, not display copy. Never render it.
            Self::Server { .. } => "the authentication service returned an error",
        };
        formatter.write_str(message)
    }
}

/// Errors raised by the local session state machine.
#[derive(Clone, PartialEq, Eq)]
pub enum SessionCommandError {
    /// A command requiring an active session was requested while signed out.
    NotAuthenticated,
    /// The transport returned malformed session material.
    InvalidSession(String),
    /// The application transport failed.
    Transport(AuthTransportError),
    /// The target secure-session adapter failed. There is no insecure fallback.
    SecureStorage(SecureSessionError),
}

impl fmt::Debug for SessionCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAuthenticated => formatter.write_str("SessionCommandError::NotAuthenticated"),
            Self::InvalidSession(_) => {
                formatter.write_str("SessionCommandError::InvalidSession(REDACTED)")
            }
            Self::Transport(error) => formatter
                .debug_tuple("SessionCommandError::Transport")
                .field(error)
                .finish(),
            Self::SecureStorage(error) => formatter
                .debug_tuple("SessionCommandError::SecureStorage")
                .field(error)
                .finish(),
        }
    }
}

impl fmt::Display for SessionCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAuthenticated => formatter.write_str("no authenticated session"),
            Self::InvalidSession(_) => formatter.write_str("invalid session material"),
            Self::Transport(error) => write!(formatter, "authentication transport failed: {error}"),
            Self::SecureStorage(error) => write!(formatter, "{error}"),
        }
    }
}

/// Validation error for opaque session identifiers and token material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMaterialError(&'static str);

impl fmt::Display for SessionMaterialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

/// An opaque server-issued session identifier. It is not a bearer credential.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Validate and retain a non-empty server session identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, SessionMaterialError> {
        let value = value.into();
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(SessionMaterialError(
                "session identifier is empty or invalid",
            ));
        }
        Ok(Self(value))
    }

    /// Borrow the identifier for transport metadata.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionId(REDACTED)")
    }
}

macro_rules! secret_material {
    ($name:ident, $label:literal) => {
        #[derive(Clone, PartialEq, Eq)]
        pub struct $name(String);

        impl $name {
            /// Construct an opaque value received from the server or user input.
            pub fn new(value: impl Into<String>) -> Result<Self, SessionMaterialError> {
                let value = value.into();
                if value.trim().is_empty() || value.chars().any(char::is_control) {
                    return Err(SessionMaterialError($label));
                }
                Ok(Self(value))
            }

            /// Borrow the value only while constructing an authenticated transport request.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "(REDACTED)"))
            }
        }
    };
}

secret_material!(AccessToken, "access token is empty or invalid");
secret_material!(RefreshToken, "refresh token is empty or invalid");
secret_material!(SecretInput, "secret input is empty or invalid");

/// A typed, display-only timestamp returned by the server.
#[derive(Clone, PartialEq, Eq)]
pub struct ExpiryTimestamp(String);

impl fmt::Debug for ExpiryTimestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ExpiryTimestamp")
            .field(&SafeUiText::new(&self.0))
            .finish()
    }
}

impl ExpiryTimestamp {
    /// Retain a non-empty server timestamp without parsing or trusting it for authorization.
    pub fn new(value: impl Into<String>) -> Result<Self, SessionMaterialError> {
        let value = value.into();
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(SessionMaterialError("expiry timestamp is empty or invalid"));
        }
        Ok(Self(value))
    }

    /// Borrow the timestamp for safe display or transport DTOs.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Login input. The password is held only for the duration of the transport call.
pub struct LoginRequest {
    pub principal_ref: String,
    pub password: SecretInput,
    pub device_label: String,
}

impl fmt::Debug for LoginRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginRequest")
            .field("principal_ref", &SafeUiText::new(&self.principal_ref))
            .field("password", &self.password)
            .field("device_label", &SafeUiText::new(&self.device_label))
            .finish()
    }
}

impl LoginRequest {
    /// Build a login request without exposing the password through debug output.
    pub fn new(
        principal_ref: impl Into<String>,
        password: impl Into<String>,
        device_label: impl Into<String>,
    ) -> Result<Self, SessionMaterialError> {
        let principal_ref = principal_ref.into();
        let device_label = device_label.into();
        if principal_ref.trim().is_empty() || device_label.trim().is_empty() {
            return Err(SessionMaterialError(
                "principal and device labels are required",
            ));
        }
        Ok(Self {
            principal_ref,
            password: SecretInput::new(password)?,
            device_label,
        })
    }
}

/// Typed response from the server login/refresh contract.
#[derive(Debug)]
pub struct IssuedSession {
    session_id: SessionId,
    access_token: AccessToken,
    refresh_token: RefreshToken,
    access_expires_at: ExpiryTimestamp,
    refresh_expires_at: ExpiryTimestamp,
}

impl IssuedSession {
    /// Validate the existing server HTTP auth response fields.
    pub fn new(
        session_id: impl Into<String>,
        access_token: impl Into<String>,
        refresh_token: impl Into<String>,
        access_expires_at: impl Into<String>,
        refresh_expires_at: impl Into<String>,
    ) -> Result<Self, SessionMaterialError> {
        Ok(Self {
            session_id: SessionId::new(session_id)?,
            access_token: AccessToken::new(access_token)?,
            refresh_token: RefreshToken::new(refresh_token)?,
            access_expires_at: ExpiryTimestamp::new(access_expires_at)?,
            refresh_expires_at: ExpiryTimestamp::new(refresh_expires_at)?,
        })
    }

    fn into_parts(
        self,
    ) -> (
        SessionId,
        AccessToken,
        RefreshToken,
        ExpiryTimestamp,
        ExpiryTimestamp,
    ) {
        (
            self.session_id,
            self.access_token,
            self.refresh_token,
            self.access_expires_at,
            self.refresh_expires_at,
        )
    }
}

/// Errors returned by a target-owned secure session adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecureSessionError {
    /// The target has no secure-session implementation for this build.
    Unsupported,
    /// The protected store could not be reached or unlocked.
    Unavailable,
    /// Stored material failed validation and must not be used.
    Corrupt,
    /// A platform storage operation failed without exposing its details.
    Failed,
}

impl fmt::Display for SecureSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unsupported => "secure session storage is unsupported",
            Self::Unavailable => "secure session storage is unavailable",
            Self::Corrupt => "secure session storage is corrupt",
            Self::Failed => "secure session storage failed",
        };
        formatter.write_str(message)
    }
}

/// A future returned by a target-owned secure session adapter.
pub type SecureSessionFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SecureSessionError>> + 'a>>;

/// Validated session material that may cross only the typed secure-storage boundary.
///
/// This type intentionally has no serialization implementation and redacts all bearer values
/// from `Debug`. Production clients must provide an OS-backed implementation of
/// `SecureSessionStore`; the shared crate provides no plaintext fallback.
#[derive(Clone, PartialEq, Eq)]
pub struct StoredSession {
    principal_ref: String,
    session_id: SessionId,
    access_token: AccessToken,
    refresh_token: RefreshToken,
    access_expires_at: ExpiryTimestamp,
    refresh_expires_at: ExpiryTimestamp,
}

impl fmt::Debug for StoredSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredSession")
            .field("principal_ref", &SafeUiText::new(&self.principal_ref))
            .field("session_id", &self.session_id)
            .field("access_token", &self.access_token)
            .field("refresh_token", &self.refresh_token)
            .field("access_expires_at", &self.access_expires_at)
            .field("refresh_expires_at", &self.refresh_expires_at)
            .finish()
    }
}

impl StoredSession {
    #[allow(dead_code)]
    const SECURE_STORE_MAGIC: &'static [u8] = b"ERPX-SESSION-1\0";
    #[allow(dead_code)]
    pub(crate) const MAX_SECURE_STORE_BYTES: usize = 64 * 1024;

    /// Validate material loaded by a target secure store.
    pub fn new(
        principal_ref: impl Into<String>,
        session_id: impl Into<String>,
        access_token: impl Into<String>,
        refresh_token: impl Into<String>,
        access_expires_at: impl Into<String>,
        refresh_expires_at: impl Into<String>,
    ) -> Result<Self, SessionMaterialError> {
        let principal_ref = principal_ref.into();
        if principal_ref.trim().is_empty() || principal_ref.chars().any(char::is_control) {
            return Err(SessionMaterialError(
                "stored session principal is empty or invalid",
            ));
        }
        Ok(Self {
            principal_ref,
            session_id: SessionId::new(session_id)?,
            access_token: AccessToken::new(access_token)?,
            refresh_token: RefreshToken::new(refresh_token)?,
            access_expires_at: ExpiryTimestamp::new(access_expires_at)?,
            refresh_expires_at: ExpiryTimestamp::new(refresh_expires_at)?,
        })
    }

    /// Return the principal reference needed to rebuild display-safe session metadata.
    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }

    /// Return the opaque session identifier for server refresh attribution.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Return access material only to the target secure adapter or authenticated transport.
    pub fn access_token(&self) -> &AccessToken {
        &self.access_token
    }

    /// Return refresh material only to the target secure adapter or authenticated transport.
    pub fn refresh_token(&self) -> &RefreshToken {
        &self.refresh_token
    }

    /// Return the server-provided access expiry timestamp.
    pub fn access_expires_at(&self) -> &ExpiryTimestamp {
        &self.access_expires_at
    }

    /// Return the server-provided refresh expiry timestamp.
    pub fn refresh_expires_at(&self) -> &ExpiryTimestamp {
        &self.refresh_expires_at
    }

    /// Encode the validated session for a target OS-protected store.
    ///
    /// This is an internal, bounded wire format rather than a public
    /// serialization implementation. The resulting bytes are only allowed to
    /// cross a [`SecureSessionStore`] boundary; callers must never write them
    /// to a file, browser storage, log, or URL.
    #[allow(dead_code)]
    pub(crate) fn encode_secure_store(&self) -> Result<Vec<u8>, SecureSessionError> {
        let fields = [
            self.principal_ref.as_bytes(),
            self.session_id.as_str().as_bytes(),
            self.access_token.as_str().as_bytes(),
            self.refresh_token.as_str().as_bytes(),
            self.access_expires_at.as_str().as_bytes(),
            self.refresh_expires_at.as_str().as_bytes(),
        ];
        let total = Self::SECURE_STORE_MAGIC.len()
            + fields
                .iter()
                .map(|field| 4usize.saturating_add(field.len()))
                .sum::<usize>();
        if total > Self::MAX_SECURE_STORE_BYTES
            || fields.iter().any(|field| field.len() > u32::MAX as usize)
        {
            return Err(SecureSessionError::Failed);
        }
        let mut encoded = Vec::with_capacity(total);
        encoded.extend_from_slice(Self::SECURE_STORE_MAGIC);
        for field in fields {
            encoded.extend_from_slice(&(field.len() as u32).to_be_bytes());
            encoded.extend_from_slice(field);
        }
        Ok(encoded)
    }

    /// Decode and revalidate bytes returned by a target OS-protected store.
    #[allow(dead_code)]
    pub(crate) fn decode_secure_store(bytes: &[u8]) -> Result<Self, SecureSessionError> {
        if bytes.len() > Self::MAX_SECURE_STORE_BYTES
            || !bytes.starts_with(Self::SECURE_STORE_MAGIC)
        {
            return Err(SecureSessionError::Corrupt);
        }
        let mut cursor = Self::SECURE_STORE_MAGIC.len();
        let mut fields = Vec::with_capacity(6);
        for _ in 0..6 {
            let length_bytes = bytes
                .get(cursor..cursor.saturating_add(4))
                .ok_or(SecureSessionError::Corrupt)?;
            let length = u32::from_be_bytes(
                length_bytes
                    .try_into()
                    .map_err(|_| SecureSessionError::Corrupt)?,
            ) as usize;
            cursor = cursor.saturating_add(4);
            let end = cursor
                .checked_add(length)
                .ok_or(SecureSessionError::Corrupt)?;
            let field = bytes.get(cursor..end).ok_or(SecureSessionError::Corrupt)?;
            fields.push(
                std::str::from_utf8(field)
                    .map_err(|_| SecureSessionError::Corrupt)?
                    .to_owned(),
            );
            cursor = end;
        }
        if cursor != bytes.len() {
            return Err(SecureSessionError::Corrupt);
        }
        Self::new(
            fields.remove(0),
            fields.remove(0),
            fields.remove(0),
            fields.remove(0),
            fields.remove(0),
            fields.remove(0),
        )
        .map_err(|_| SecureSessionError::Corrupt)
    }
}

/// Typed boundary for OS-backed secure session storage.
///
/// Implementations belong to target applications and must use platform-protected storage. This
/// trait has no default implementation so unsupported Android/iOS/web/desktop targets cannot
/// silently fall back to plaintext local storage.
pub trait SecureSessionStore {
    /// Load a validated session, if one is present in protected storage.
    fn load(&self) -> SecureSessionFuture<'_, Option<StoredSession>>;
    /// Save validated session material in protected storage.
    fn save(&self, session: &StoredSession) -> SecureSessionFuture<'_, ()>;
    /// Remove protected session material after explicit sign-out or revocation.
    fn clear(&self) -> SecureSessionFuture<'_, ()>;
}

/// Display-safe session information retained by the shell.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub session_id: SessionId,
    pub principal_ref: String,
    pub access_expires_at: ExpiryTimestamp,
    pub refresh_expires_at: ExpiryTimestamp,
    pub metadata: SafeSessionMetadata,
}

impl fmt::Debug for SessionSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionSnapshot")
            .field("session_id", &self.session_id)
            .field("principal_ref", &SafeUiText::new(&self.principal_ref))
            .field(
                "access_expires_at",
                &SafeUiText::new(self.access_expires_at.as_str()),
            )
            .field(
                "refresh_expires_at",
                &SafeUiText::new(self.refresh_expires_at.as_str()),
            )
            .field("metadata", &self.metadata)
            .finish()
    }
}

/// Explicit client-local authentication lifecycle states.
#[derive(Clone, PartialEq, Eq)]
pub enum SessionState {
    SignedOut {
        return_to: String,
    },
    Restoring {
        return_to: String,
    },
    Authenticating {
        return_to: String,
    },
    Authenticated(SessionSnapshot),
    Refreshing(SessionSnapshot),
    Revoking(SessionSnapshot),
    Expired {
        return_to: String,
        reason: String,
    },
    Revoked {
        return_to: String,
    },
    Offline {
        last_known: Option<SessionSnapshot>,
        reason: String,
    },
    Error {
        return_to: String,
        message: String,
    },
}

impl fmt::Debug for SessionState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SignedOut { return_to } => formatter
                .debug_struct("SessionState::SignedOut")
                .field("return_to", &SafeUiText::new(redact_deep_link(return_to)))
                .finish(),
            Self::Restoring { return_to } => formatter
                .debug_struct("SessionState::Restoring")
                .field("return_to", &SafeUiText::new(redact_deep_link(return_to)))
                .finish(),
            Self::Authenticating { return_to } => formatter
                .debug_struct("SessionState::Authenticating")
                .field("return_to", &SafeUiText::new(redact_deep_link(return_to)))
                .finish(),
            Self::Authenticated(snapshot) => formatter
                .debug_tuple("SessionState::Authenticated")
                .field(snapshot)
                .finish(),
            Self::Refreshing(snapshot) => formatter
                .debug_tuple("SessionState::Refreshing")
                .field(snapshot)
                .finish(),
            Self::Revoking(snapshot) => formatter
                .debug_tuple("SessionState::Revoking")
                .field(snapshot)
                .finish(),
            Self::Expired { return_to, reason } => formatter
                .debug_struct("SessionState::Expired")
                .field("return_to", &SafeUiText::new(redact_deep_link(return_to)))
                .field("reason", &SafeUiText::new(reason))
                .finish(),
            Self::Revoked { return_to } => formatter
                .debug_struct("SessionState::Revoked")
                .field("return_to", &SafeUiText::new(redact_deep_link(return_to)))
                .finish(),
            Self::Offline { last_known, reason } => formatter
                .debug_struct("SessionState::Offline")
                .field("last_known", last_known)
                .field("reason", &SafeUiText::new(reason))
                .finish(),
            Self::Error { return_to, message } => formatter
                .debug_struct("SessionState::Error")
                .field("return_to", &SafeUiText::new(redact_deep_link(return_to)))
                .field("message", &SafeUiText::new(message))
                .finish(),
        }
    }
}

impl SessionState {
    /// The security contract keeps server-side revocation within five minutes.
    pub const REVOCATION_BUDGET_SECONDS: u32 = 5 * 60;

    /// Return the fail-closed high-risk/MFA marker for the UI boundary.
    ///
    /// An authenticated session is not a high-risk step-up receipt. This shared client state has
    /// no backend-issued MFA proof, so active sessions must remain visibly MFA-gated and every
    /// other lifecycle state must remain denied. Applications may replace this marker only after
    /// their authoritative server boundary supplies and validates a fresh step-up result.
    pub const fn high_risk_mfa_marker(&self) -> &'static str {
        match self {
            Self::Authenticated(_) => "backend-mfa-required",
            Self::SignedOut { .. }
            | Self::Restoring { .. }
            | Self::Authenticating { .. }
            | Self::Refreshing(_)
            | Self::Revoking(_)
            | Self::Expired { .. }
            | Self::Revoked { .. }
            | Self::Offline { .. }
            | Self::Error { .. } => "denied",
        }
    }

    /// High-risk content is never authorized from local session state or local fallback.
    pub const fn may_render_high_risk_access(&self) -> bool {
        false
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::SignedOut {
            return_to: "/".to_string(),
        }
    }
}

impl SessionState {
    /// Return the safe banner metadata, if a session is still known locally.
    pub fn metadata(&self) -> Option<&SafeSessionMetadata> {
        match self {
            Self::Authenticated(snapshot)
            | Self::Refreshing(snapshot)
            | Self::Revoking(snapshot) => Some(&snapshot.metadata),
            Self::Offline {
                last_known: Some(snapshot),
                ..
            } => Some(&snapshot.metadata),
            _ => None,
        }
    }

    /// Return `(title, message)` copy safe for any target renderer.
    ///
    /// Titles are fixed policy copy. Reasons supplied by transports or target adapters pass
    /// through `SafeUiText`, which bounds their size and redacts credential-bearing content.
    pub fn display_copy(&self) -> (SafeUiText, SafeUiText) {
        let (title, message) = match self {
            Self::SignedOut { .. } => ("Signed out", "No authenticated session is active."),
            Self::Restoring { .. } => ("Restoring session", "Checking secure session storage."),
            Self::Authenticating { .. } => ("Signing in", "Authentication is in progress."),
            Self::Authenticated(_) => ("Authenticated", "The session is active."),
            Self::Refreshing(_) => ("Refreshing session", "The session is being refreshed."),
            Self::Revoking(_) => (
                "Signing out",
                "The server is processing session revocation.",
            ),
            Self::Expired { reason, .. } => ("Session expired", reason.as_str()),
            Self::Revoked { .. } => ("Session revoked", "The session was revoked by the server."),
            Self::Offline { reason, .. } => ("Authentication offline", reason.as_str()),
            Self::Error { message, .. } => ("Authentication error", message.as_str()),
        };
        (SafeUiText::new(title), SafeUiText::new(message))
    }

    /// Convert local lifecycle state to the route boundary without making an authorization decision.
    pub fn route_state(&self) -> RouteState {
        match self {
            Self::SignedOut { return_to } => RouteState::Unauthenticated {
                return_to: safe_return_to(return_to),
            },
            Self::Restoring { return_to } => RouteState::Unauthenticated {
                return_to: safe_return_to(return_to),
            },
            Self::Authenticating { return_to } => RouteState::Unauthenticated {
                return_to: safe_return_to(return_to),
            },
            Self::Authenticated(snapshot) => authenticated_route(snapshot),
            // A token rotation or revocation request is not an authorization result. Block route
            // content until the server operation completes and the state becomes explicit.
            Self::Refreshing(_) | Self::Revoking(_) => RouteState::Offline {
                cached_view_available: false,
                reason: "The authentication session is being verified.".into(),
            },
            Self::Expired { return_to, .. } => RouteState::SessionExpired {
                return_to: safe_return_to(return_to),
            },
            Self::Revoked { return_to } => RouteState::SessionRevoked {
                return_to: safe_return_to(return_to),
            },
            Self::Offline {
                last_known, reason, ..
            } => RouteState::Offline {
                cached_view_available: last_known.is_some(),
                reason: safe_display_text(reason),
            },
            Self::Error { message, .. } => RouteState::Offline {
                cached_view_available: false,
                reason: safe_display_text(message),
            },
        }
    }
}

/// Application-provided HTTP/Web transport seam. Implementations belong to the target client;
/// this trait contains no browser storage or authorization policy.
pub trait SessionTransport {
    /// Exchange credentials for a short-lived access token and rotating refresh token.
    fn login(&self, request: LoginRequest) -> SessionFuture<'_, IssuedSession>;
    /// Rotate the refresh token and receive a new access token.
    fn refresh(
        &self,
        session_id: &SessionId,
        refresh_token: &RefreshToken,
    ) -> SessionFuture<'_, IssuedSession>;
    /// Revoke the current session at the server.
    fn revoke(&self, session_id: &SessionId, access_token: &AccessToken) -> SessionFuture<'_, ()>;
}

#[derive(Debug)]
struct SessionCredentials {
    access_token: AccessToken,
    refresh_token: RefreshToken,
    snapshot: SessionSnapshot,
}

/// Memory-only session controller for a Dioxus application boundary.
#[derive(Debug, Default)]
pub struct SessionController {
    state: SessionState,
    credentials: Option<SessionCredentials>,
}

impl SessionController {
    /// The server revocation contract remains five minutes; clients do not shorten or extend it.
    pub const REVOCATION_BUDGET_SECONDS: u32 = SessionState::REVOCATION_BUDGET_SECONDS;

    /// Return the current lifecycle state; no token is included.
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// Return safe display metadata for shell banners.
    pub fn metadata(&self) -> Option<&SafeSessionMetadata> {
        self.state.metadata()
    }

    /// Return the current access token only for an in-memory authenticated request adapter.
    pub fn access_token(&self) -> Option<&AccessToken> {
        self.credentials.as_ref().map(|value| &value.access_token)
    }

    /// Return the current session identifier for transport attribution.
    pub fn session_id(&self) -> Option<&SessionId> {
        self.credentials
            .as_ref()
            .map(|value| &value.snapshot.session_id)
    }

    /// Replace display-only metadata supplied by the authenticated application boundary.
    pub fn set_metadata(&mut self, metadata: SafeSessionMetadata) {
        if let Some(credentials) = self.credentials.as_mut() {
            credentials.snapshot.metadata = metadata.clone();
        }
        match &mut self.state {
            SessionState::Authenticated(snapshot)
            | SessionState::Refreshing(snapshot)
            | SessionState::Revoking(snapshot) => snapshot.metadata = metadata.clone(),
            SessionState::Offline {
                last_known: Some(snapshot),
                ..
            } => snapshot.metadata = metadata,
            _ => {}
        }
    }

    /// Persist the active session through a target-owned secure adapter.
    ///
    /// Calling this method is the only shared path that hands session material to a store. A
    /// storage error is returned to the caller; the controller never substitutes plaintext or
    /// browser storage.
    pub async fn persist_secure_session<S: SecureSessionStore + ?Sized>(
        &self,
        store: &S,
    ) -> Result<(), SessionCommandError> {
        let session = self
            .secure_session()
            .ok_or(SessionCommandError::NotAuthenticated)?;
        store
            .save(&session)
            .await
            .map_err(SessionCommandError::SecureStorage)
    }

    /// Restore a session from a target-owned secure adapter without trusting route or display
    /// metadata from storage.
    ///
    /// A protected local copy is not proof that the server session is still active: the server
    /// may have revoked it since the copy was written. A successful restore therefore retains
    /// credentials only for the next authoritative [`Self::refresh`] call and remains in
    /// `Restoring`, which maps to a blocked route state. Target applications must revalidate
    /// before rendering authenticated content.
    pub async fn restore_secure_session<S: SecureSessionStore + ?Sized>(
        &mut self,
        store: &S,
        return_to: impl AsRef<str>,
    ) -> Result<bool, SessionCommandError> {
        let return_to = safe_return_to(return_to.as_ref());
        self.credentials = None;
        self.state = SessionState::Restoring {
            return_to: return_to.clone(),
        };
        match store.load().await {
            Ok(Some(session)) => {
                self.install_stored(session, return_to);
                Ok(true)
            }
            Ok(None) => {
                self.state = SessionState::SignedOut { return_to };
                Ok(false)
            }
            Err(error) => {
                self.state = SessionState::Error {
                    return_to,
                    message: secure_storage_message(&error).into(),
                };
                Err(SessionCommandError::SecureStorage(error))
            }
        }
    }

    /// Restore and immediately revalidate a protected session with the server.
    ///
    /// This is the safe one-call target boundary for applications that support session restore.
    /// Terminal server rejection clears the protected copy so a revoked session cannot be
    /// retried locally. Offline failures preserve the copy for an explicit retry; no local copy
    /// is treated as authorization while revalidation is pending.
    pub async fn restore_and_revalidate_secure_session<
        T: SessionTransport + ?Sized,
        S: SecureSessionStore + ?Sized,
    >(
        &mut self,
        transport: &T,
        store: &S,
        return_to: impl AsRef<str>,
    ) -> Result<bool, SessionCommandError> {
        let return_to = safe_return_to(return_to.as_ref());
        if !self.restore_secure_session(store, &return_to).await? {
            return Ok(false);
        }

        match self.refresh(transport, &return_to).await {
            Ok(()) => Ok(true),
            Err(error) if terminal_revalidation_error(&error) => {
                store.clear().await.map_err(|storage_error| {
                    self.state = SessionState::Error {
                        return_to: return_to.clone(),
                        message: secure_storage_message(&storage_error).into(),
                    };
                    SessionCommandError::SecureStorage(storage_error)
                })?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    /// Clear only the target-owned secure store. The controller has no plaintext persistence
    /// fallback when the adapter reports an error.
    pub async fn clear_secure_session<S: SecureSessionStore + ?Sized>(
        &self,
        store: &S,
    ) -> Result<(), SessionCommandError> {
        store
            .clear()
            .await
            .map_err(SessionCommandError::SecureStorage)
    }

    /// Revoke the server session and remove its target-owned persisted copy.
    ///
    /// The lower-level [`Self::revoke`] method intentionally has no storage dependency and only
    /// clears in-memory credentials. Callers that enabled secure persistence must use this
    /// compound operation so a successfully revoked session cannot be restored from the target
    /// store. An offline failure preserves both the in-memory credentials and secure copy for a
    /// later retry; a secure-store clear failure is returned without any plaintext fallback.
    pub async fn revoke_and_clear_secure_session<
        T: SessionTransport + ?Sized,
        S: SecureSessionStore + ?Sized,
    >(
        &mut self,
        transport: &T,
        store: &S,
        return_to: impl AsRef<str>,
    ) -> Result<(), SessionCommandError> {
        let return_to = safe_return_to(return_to.as_ref());
        self.revoke(transport, &return_to).await?;
        store.clear().await.map_err(|error| {
            self.state = SessionState::Error {
                return_to: return_to.clone(),
                message: secure_storage_message(&error).into(),
            };
            SessionCommandError::SecureStorage(error)
        })
    }

    fn secure_session(&self) -> Option<StoredSession> {
        let credentials = self.credentials.as_ref()?;
        StoredSession::new(
            credentials.snapshot.principal_ref.clone(),
            credentials.snapshot.session_id.as_str(),
            credentials.access_token.as_str(),
            credentials.refresh_token.as_str(),
            credentials.snapshot.access_expires_at.as_str(),
            credentials.snapshot.refresh_expires_at.as_str(),
        )
        .ok()
    }

    /// Begin login and retain only the resulting credentials in memory.
    pub async fn login<T: SessionTransport + ?Sized>(
        &mut self,
        transport: &T,
        request: LoginRequest,
        return_to: impl AsRef<str>,
    ) -> Result<(), SessionCommandError> {
        let return_to = safe_return_to(return_to.as_ref());
        let principal_ref = request.principal_ref.clone();
        self.credentials = None;
        self.state = SessionState::Authenticating {
            return_to: return_to.clone(),
        };
        match transport.login(request).await {
            Ok(session) => {
                self.install(session, principal_ref)?;
                Ok(())
            }
            Err(error) => {
                self.state = state_for_transport_error(error.clone(), return_to);
                Err(SessionCommandError::Transport(error))
            }
        }
    }

    /// Rotate a refresh token without exposing it to the UI state or persistence layer.
    pub async fn refresh<T: SessionTransport + ?Sized>(
        &mut self,
        transport: &T,
        return_to: impl AsRef<str>,
    ) -> Result<(), SessionCommandError> {
        let return_to = safe_return_to(return_to.as_ref());
        let credentials = self
            .credentials
            .take()
            .ok_or(SessionCommandError::NotAuthenticated)?;
        let previous = credentials.snapshot.clone();
        self.state = SessionState::Refreshing(previous.clone());
        let principal_ref = previous.principal_ref.clone();
        match transport
            .refresh(&previous.session_id, &credentials.refresh_token)
            .await
        {
            Ok(session) => {
                self.install_with_metadata(
                    session,
                    principal_ref,
                    Some(previous.metadata.clone()),
                )?;
                Ok(())
            }
            Err(error) => {
                if matches!(error, AuthTransportError::Offline) {
                    self.credentials = Some(credentials);
                }
                self.state = state_for_transport_error(error.clone(), return_to);
                Err(SessionCommandError::Transport(error))
            }
        }
    }

    /// Revoke the session server-side and clear local credentials on success or rejection.
    pub async fn revoke<T: SessionTransport + ?Sized>(
        &mut self,
        transport: &T,
        return_to: impl AsRef<str>,
    ) -> Result<(), SessionCommandError> {
        let return_to = safe_return_to(return_to.as_ref());
        let credentials = self
            .credentials
            .take()
            .ok_or(SessionCommandError::NotAuthenticated)?;
        let previous = credentials.snapshot.clone();
        self.state = SessionState::Revoking(previous);
        match transport
            .revoke(&credentials.snapshot.session_id, &credentials.access_token)
            .await
        {
            Ok(()) => {
                self.state = SessionState::SignedOut { return_to };
                Ok(())
            }
            Err(AuthTransportError::Unauthorized) => {
                self.state = SessionState::Revoked { return_to };
                Ok(())
            }
            Err(AuthTransportError::Offline) => {
                let snapshot = match &self.state {
                    SessionState::Revoking(snapshot) => Some(snapshot.clone()),
                    _ => None,
                };
                self.credentials = Some(credentials);
                self.state = SessionState::Offline {
                    last_known: snapshot,
                    reason: "The session could not be revoked while offline.".into(),
                };
                Err(SessionCommandError::Transport(AuthTransportError::Offline))
            }
            Err(error) => {
                self.state = SessionState::Error {
                    return_to,
                    message: "The session could not be revoked.".into(),
                };
                Err(SessionCommandError::Transport(error))
            }
        }
    }

    fn install(
        &mut self,
        issued: IssuedSession,
        principal_ref: String,
    ) -> Result<(), SessionCommandError> {
        self.install_with_metadata(issued, principal_ref, None)
    }

    fn install_stored(&mut self, stored: StoredSession, return_to: String) {
        let metadata = SafeSessionMetadata {
            principal_label: Some(stored.principal_ref.clone()),
            session_expires_at: Some(stored.refresh_expires_at.as_str().to_string()),
            ..SafeSessionMetadata::default()
        };
        let snapshot = SessionSnapshot {
            session_id: stored.session_id.clone(),
            principal_ref: stored.principal_ref.clone(),
            access_expires_at: stored.access_expires_at.clone(),
            refresh_expires_at: stored.refresh_expires_at.clone(),
            metadata,
        };
        self.credentials = Some(SessionCredentials {
            access_token: stored.access_token,
            refresh_token: stored.refresh_token,
            snapshot: snapshot.clone(),
        });
        self.state = SessionState::Restoring { return_to };
    }

    fn install_with_metadata(
        &mut self,
        issued: IssuedSession,
        principal_ref: String,
        existing_metadata: Option<SafeSessionMetadata>,
    ) -> Result<(), SessionCommandError> {
        let (session_id, access_token, refresh_token, access_expires_at, refresh_expires_at) =
            issued.into_parts();
        let metadata = existing_metadata.unwrap_or_else(|| SafeSessionMetadata {
            principal_label: Some(principal_ref.clone()),
            session_expires_at: Some(refresh_expires_at.as_str().to_string()),
            ..SafeSessionMetadata::default()
        });
        let snapshot = SessionSnapshot {
            session_id,
            principal_ref,
            access_expires_at,
            refresh_expires_at,
            metadata,
        };
        self.credentials = Some(SessionCredentials {
            access_token,
            refresh_token,
            snapshot: snapshot.clone(),
        });
        self.state = SessionState::Authenticated(snapshot);
        Ok(())
    }
}

fn state_for_transport_error(error: AuthTransportError, return_to: String) -> SessionState {
    match error {
        AuthTransportError::Unauthorized => SessionState::Expired {
            return_to,
            reason: "The server rejected the session.".into(),
        },
        AuthTransportError::Offline => SessionState::Offline {
            last_known: None,
            reason: "The authentication service is unavailable.".into(),
        },
        AuthTransportError::InvalidRequest | AuthTransportError::Server { .. } => {
            SessionState::Error {
                return_to,
                message: "Authentication could not be completed.".into(),
            }
        }
    }
}

fn terminal_revalidation_error(error: &SessionCommandError) -> bool {
    matches!(
        error,
        SessionCommandError::Transport(
            AuthTransportError::Unauthorized | AuthTransportError::InvalidRequest
        )
    )
}

fn secure_storage_message(error: &SecureSessionError) -> &'static str {
    match error {
        SecureSessionError::Unsupported => {
            "Secure session storage is not configured; no local fallback is available."
        }
        SecureSessionError::Unavailable => {
            "Secure session storage is unavailable; no local fallback is available."
        }
        SecureSessionError::Corrupt => {
            "Stored secure session data was rejected; no local fallback is available."
        }
        SecureSessionError::Failed => {
            "Secure session storage failed; no local fallback is available."
        }
    }
}

fn safe_display_text(value: &str) -> String {
    SafeUiText::new(value).as_str().to_string()
}

/// Normalize a return target for a native or browser router without ever producing a partial
/// route.  `SafeUiText` is intentionally a display-text boundary and may append an ellipsis when
/// input is too long; that is safe for a banner, but unsafe when the same value is handed to a
/// router.  Return targets therefore use a strict app-relative policy and fall back to the home
/// route instead of being truncated or interpreted as an external URI.
fn safe_return_to(value: &str) -> String {
    let raw = value.trim();
    if has_external_origin(raw) {
        return "/".to_string();
    }

    let normalized = redact_deep_link(raw);
    if normalized == "/" || is_mobile_safe_return_path(&normalized) {
        normalized
    } else {
        "/".to_string()
    }
}

/// Reject URI schemes and protocol-relative targets before route normalization can make them
/// look like ordinary paths.  This is deliberately target-neutral: both a Dioxus router and a
/// browser/native deep-link bridge must see the same-origin decision made here.
fn has_external_origin(value: &str) -> bool {
    if value.starts_with("//") || value.starts_with("\\\\") {
        return true;
    }

    let separator = value.find(['/', '\\']);
    let colon = value.find(':');
    match (colon, separator) {
        (Some(colon), Some(separator)) => colon <= separator,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Return targets are restricted to bounded, unencoded, whitespace-free app paths.  Rejecting
/// percent-encoding and backslashes avoids target-specific decoding differences in mobile
/// bridges, where a path can otherwise be reinterpreted as a new origin or route hierarchy.
fn is_mobile_safe_return_path(path: &str) -> bool {
    !path.is_empty()
        && path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= SafeUiText::MAX_BYTES
        && path.chars().all(|character| {
            !character.is_control()
                && !character.is_whitespace()
                && !matches!(character, '\\' | '%' | ':')
        })
}

fn authenticated_route(snapshot: &SessionSnapshot) -> RouteState {
    if let Some(read_only) = snapshot.metadata.read_only.as_ref() {
        RouteState::ReadOnly {
            session: snapshot.metadata.clone(),
            reason: safe_display_text(&read_only.reason),
        }
    } else {
        RouteState::Authenticated {
            session: snapshot.metadata.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::future::Future;

    #[derive(Debug)]
    struct FakeTransport {
        result: Result<IssuedSession, AuthTransportError>,
    }

    #[derive(Debug, Default)]
    struct OfflineRevokeTransport;

    impl SessionTransport for OfflineRevokeTransport {
        fn login(&self, _request: LoginRequest) -> SessionFuture<'_, IssuedSession> {
            Box::pin(async {
                IssuedSession::new(
                    "session-offline-revoke",
                    "access-secret",
                    "refresh-secret",
                    "access-expiry",
                    "refresh-expiry",
                )
                .map_err(|error| AuthTransportError::Server {
                    code: error.to_string(),
                })
            })
        }

        fn refresh(
            &self,
            _session_id: &SessionId,
            _refresh_token: &RefreshToken,
        ) -> SessionFuture<'_, IssuedSession> {
            Box::pin(async { Err(AuthTransportError::Offline) })
        }

        fn revoke(
            &self,
            _session_id: &SessionId,
            _access_token: &AccessToken,
        ) -> SessionFuture<'_, ()> {
            Box::pin(async { Err(AuthTransportError::Offline) })
        }
    }

    /// Deliberately test-only adapter. No production target can accidentally select this as a
    /// fallback because it is compiled only with the shared crate's tests.
    #[derive(Debug, Default)]
    struct InMemorySessionStore {
        value: RefCell<Option<StoredSession>>,
    }

    impl SecureSessionStore for InMemorySessionStore {
        fn load(&self) -> SecureSessionFuture<'_, Option<StoredSession>> {
            let value = self.value.borrow().clone();
            Box::pin(async move { Ok(value) })
        }

        fn save(&self, session: &StoredSession) -> SecureSessionFuture<'_, ()> {
            let value = session.clone();
            self.value.replace(Some(value));
            Box::pin(async { Ok(()) })
        }

        fn clear(&self) -> SecureSessionFuture<'_, ()> {
            self.value.replace(None);
            Box::pin(async { Ok(()) })
        }
    }

    impl SessionTransport for FakeTransport {
        fn login(&self, _request: LoginRequest) -> SessionFuture<'_, IssuedSession> {
            let result = match &self.result {
                Ok(_) => Ok(IssuedSession::new(
                    "session-1",
                    "access-secret",
                    "refresh-secret",
                    "a",
                    "r",
                )
                .expect("valid fake session")),
                Err(error) => Err(error.clone()),
            };
            Box::pin(async move { result })
        }

        fn refresh(
            &self,
            _session_id: &SessionId,
            _refresh_token: &RefreshToken,
        ) -> SessionFuture<'_, IssuedSession> {
            Box::pin(async {
                IssuedSession::new(
                    "session-2",
                    "access-secret-2",
                    "refresh-secret-2",
                    "a2",
                    "r2",
                )
                .map_err(|error| AuthTransportError::Server {
                    code: error.to_string(),
                })
            })
        }

        fn revoke(
            &self,
            _session_id: &SessionId,
            _access_token: &AccessToken,
        ) -> SessionFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn token_debug_and_safe_metadata_never_disclose_bearers() {
        let token = AccessToken::new("access-secret").expect("token");
        let refresh = RefreshToken::new("refresh-secret").expect("refresh");
        assert_eq!(format!("{token:?}"), "AccessToken(REDACTED)");
        assert_eq!(format!("{refresh:?}"), "RefreshToken(REDACTED)");
        let issued = IssuedSession::new("session-1", "access-secret", "refresh-secret", "a", "r")
            .expect("issued");
        assert!(!format!("{issued:?}").contains("access-secret"));
        assert!(!format!("{issued:?}").contains("refresh-secret"));
    }

    #[test]
    fn lifecycle_display_copy_is_bounded_and_redacts_untrusted_details() {
        let states = [
            SessionState::Expired {
                return_to: "/platform".into(),
                reason: "authorization: bearer expired-secret".into(),
            },
            SessionState::Revoked {
                return_to: "/platform".into(),
            },
            SessionState::Offline {
                last_known: None,
                reason: "postgres://user:password=secret@example.test/db".into(),
            },
            SessionState::Error {
                return_to: "/platform".into(),
                message: "x".repeat(SafeUiText::MAX_BYTES + 64),
            },
        ];

        for state in states {
            let (title, message) = state.display_copy();
            assert!(title.as_str().len() <= SafeUiText::MAX_BYTES);
            assert!(message.as_str().len() <= SafeUiText::MAX_BYTES + '…'.len_utf8());
            assert!(!format!("{title:?}").contains("secret"));
            assert!(!format!("{message:?}").contains("secret"));
        }

        let offline = SessionState::Offline {
            last_known: None,
            reason: "access_token=secret".into(),
        };
        assert_eq!(
            match offline.route_state() {
                RouteState::Offline { reason, .. } => reason,
                other => panic!("unexpected route state: {other:?}"),
            },
            "[redacted]"
        );
    }

    #[test]
    fn command_error_display_never_renders_transport_details() {
        let invalid = SessionCommandError::InvalidSession("refresh_token=secret".into());
        assert_eq!(invalid.to_string(), "invalid session material");
        let server = SessionCommandError::Transport(AuthTransportError::Server {
            code: "database_url=postgres://secret".into(),
        });
        assert!(!server.to_string().contains("database_url"));
        assert!(!server.to_string().contains("secret"));
    }

    #[test]
    fn offline_revocation_preserves_retryable_memory_state_with_five_minute_budget() {
        assert_eq!(SessionState::REVOCATION_BUDGET_SECONDS, 300);
        assert_eq!(SessionController::REVOCATION_BUDGET_SECONDS, 300);

        let transport = OfflineRevokeTransport;
        let mut controller = SessionController::default();
        let request =
            LoginRequest::new("principal-1", "password", "target-neutral").expect("request");
        block_on(controller.login(&transport, request, "/platform")).expect("login");
        assert!(matches!(
            block_on(controller.revoke(&transport, "/platform")),
            Err(SessionCommandError::Transport(AuthTransportError::Offline))
        ));
        assert!(controller.access_token().is_some());
        assert!(matches!(
            controller.state(),
            SessionState::Offline {
                last_known: Some(_),
                ..
            }
        ));
        let (title, message) = controller.state().display_copy();
        assert_eq!(title.as_str(), "Authentication offline");
        assert_eq!(
            message.as_str(),
            "The session could not be revoked while offline."
        );
    }

    #[test]
    fn high_risk_access_requires_backend_mfa_and_never_local_fallback() {
        let snapshot = SessionSnapshot {
            session_id: SessionId::new("session-high-risk").expect("session id"),
            principal_ref: "principal-1".into(),
            access_expires_at: ExpiryTimestamp::new("access-expiry").expect("access expiry"),
            refresh_expires_at: ExpiryTimestamp::new("refresh-expiry").expect("refresh expiry"),
            metadata: SafeSessionMetadata::default(),
        };
        let states = [
            (
                SessionState::Authenticated(snapshot.clone()),
                "backend-mfa-required",
            ),
            (
                SessionState::Offline {
                    last_known: Some(snapshot),
                    reason: "backend unavailable".into(),
                },
                "denied",
            ),
            (
                SessionState::SignedOut {
                    return_to: "/platform".into(),
                },
                "denied",
            ),
        ];

        for (state, expected_marker) in states {
            assert_eq!(state.high_risk_mfa_marker(), expected_marker);
            assert!(!state.may_render_high_risk_access());
            assert!(!state.high_risk_mfa_marker().contains("local"));
        }
    }

    #[test]
    fn secure_storage_failure_is_explicit_and_has_no_local_fallback() {
        if cfg!(any(target_os = "ios", target_os = "macos")) {
            // Do not touch a developer's real Keychain from the host test suite. The Apple
            // target checks compile the adapter; device/desktop certification exercises it.
            return;
        }
        let adapter = crate::TargetSecureSessionAdapter::new();
        let mut controller = SessionController::default();
        assert!(matches!(
            block_on(controller.restore_secure_session(&adapter, "/platform?refresh_token=secret")),
            Err(SessionCommandError::SecureStorage(
                SecureSessionError::Unsupported
            ))
        ));
        assert!(controller.access_token().is_none());
        let (title, message) = controller.state().display_copy();
        assert_eq!(title.as_str(), "Authentication error");
        assert!(message.as_str().contains("no local fallback"));
        assert!(!message.as_str().contains("secret"));
        assert!(matches!(
            controller.state().route_state(),
            RouteState::Offline {
                cached_view_available: false,
                ..
            }
        ));
    }

    #[test]
    fn return_targets_are_bounded_and_mobile_safe() {
        assert_eq!(
            safe_return_to("/#/platform/customers?tab=health"),
            "/platform/customers"
        );
        assert_eq!(safe_return_to("https://evil.example/platform"), "/");
        assert_eq!(safe_return_to("//evil.example/platform"), "/");
        assert_eq!(safe_return_to("javascript:alert(1)"), "/");
        assert_eq!(safe_return_to("/platform\\customers"), "/");
        assert_eq!(safe_return_to("/platform/%2Fcustomers"), "/");

        let oversized = format!("/{}", "customer/".repeat(SafeUiText::MAX_BYTES));
        assert_eq!(safe_return_to(&oversized), "/");
    }

    #[test]
    fn login_input_and_controller_debug_never_disclose_password_or_tokens() {
        let request = LoginRequest::new("principal-1", "password-secret", "web").expect("request");
        assert!(!format!("{request:?}").contains("password-secret"));

        let controller = SessionController::default();
        assert!(!format!("{controller:?}").contains("password-secret"));
        assert!(!format!("{controller:?}").contains("access-secret"));
        assert!(!format!("{controller:?}").contains("refresh-secret"));
    }

    #[test]
    fn secure_store_boundary_has_only_a_test_memory_adapter() {
        let store = InMemorySessionStore::default();
        let stored = StoredSession::new(
            "principal-1",
            "session-1",
            "access-secret",
            "refresh-secret",
            "access-expiry",
            "refresh-expiry",
        )
        .expect("stored session");
        assert!(!format!("{store:?}").contains("access-secret"));
        block_on(store.save(&stored)).expect("save");
        let loaded = block_on(store.load())
            .expect("load")
            .expect("stored session present");
        assert_eq!(loaded.session_id().as_str(), "session-1");
        assert!(!format!("{loaded:?}").contains("refresh-secret"));
        block_on(store.clear()).expect("clear");
        assert!(block_on(store.load()).expect("load after clear").is_none());
    }

    #[test]
    fn restore_and_secure_clear_preserve_lifecycle_and_redact_return_paths() {
        let store = InMemorySessionStore::default();
        let stored = StoredSession::new(
            "principal-1",
            "session-1",
            "access-secret",
            "refresh-secret",
            "access-expiry",
            "refresh-expiry",
        )
        .expect("stored session");
        block_on(store.save(&stored)).expect("save");
        let mut controller = SessionController::default();
        assert!(
            block_on(
                controller
                    .restore_secure_session(&store, "/platform/customers?access_token=secret")
            )
            .expect("restore")
        );
        assert!(matches!(controller.state(), SessionState::Restoring { .. }));
        assert!(matches!(
            controller.state().route_state(),
            RouteState::Unauthenticated { .. }
        ));
        block_on(
            controller.refresh(
                &FakeTransport {
                    result: Ok(IssuedSession::new(
                        "session-restore-refresh",
                        "access-secret-2",
                        "refresh-secret-2",
                        "a2",
                        "r2",
                    )
                    .expect("revalidated session")),
                },
                "/platform/customers",
            ),
        )
        .expect("server revalidation");
        assert!(matches!(controller.state(), SessionState::Authenticated(_)));
        assert_eq!(
            controller.state().route_state().return_to(),
            None,
            "authenticated state does not expose a return path"
        );
        assert!(!format!("{controller:?}").contains("access-secret"));
        block_on(controller.clear_secure_session(&store)).expect("clear");
        assert!(block_on(store.load()).expect("load after clear").is_none());

        let state = SessionState::SignedOut {
            return_to: "/#/platform/customers?refresh_token=secret".into(),
        };
        assert_eq!(state.route_state().return_to(), Some("/".into()));
    }

    #[test]
    fn refresh_and_revoke_states_block_route_content_until_the_server_finishes() {
        let snapshot = SessionSnapshot {
            session_id: SessionId::new("session-in-flight").expect("session id"),
            principal_ref: "principal-1".into(),
            access_expires_at: ExpiryTimestamp::new("access-expiry").expect("access expiry"),
            refresh_expires_at: ExpiryTimestamp::new("refresh-expiry").expect("refresh expiry"),
            metadata: SafeSessionMetadata::default(),
        };

        for state in [
            SessionState::Refreshing(snapshot.clone()),
            SessionState::Revoking(snapshot.clone()),
        ] {
            assert!(matches!(
                state.route_state(),
                RouteState::Offline {
                    cached_view_available: false,
                    ..
                }
            ));
            assert_eq!(
                state.route_state().ui_state(),
                Some(crate::UiStateKind::Offline)
            );
        }
    }

    #[test]
    fn restore_and_revalidate_clears_revoked_protected_copy() {
        #[derive(Debug, Default)]
        struct UnauthorizedTransport;

        impl SessionTransport for UnauthorizedTransport {
            fn login(&self, _request: LoginRequest) -> SessionFuture<'_, IssuedSession> {
                Box::pin(async { Err(AuthTransportError::Unauthorized) })
            }

            fn refresh(
                &self,
                _session_id: &SessionId,
                _refresh_token: &RefreshToken,
            ) -> SessionFuture<'_, IssuedSession> {
                Box::pin(async { Err(AuthTransportError::Unauthorized) })
            }

            fn revoke(
                &self,
                _session_id: &SessionId,
                _access_token: &AccessToken,
            ) -> SessionFuture<'_, ()> {
                Box::pin(async { Err(AuthTransportError::Unauthorized) })
            }
        }

        let store = InMemorySessionStore::default();
        let stored = StoredSession::new(
            "principal-1",
            "session-revoked",
            "access-secret",
            "refresh-secret",
            "access-expiry",
            "refresh-expiry",
        )
        .expect("stored session");
        block_on(store.save(&stored)).expect("save");

        let mut controller = SessionController::default();
        assert!(matches!(
            block_on(controller.restore_and_revalidate_secure_session(
                &UnauthorizedTransport,
                &store,
                "/platform"
            )),
            Err(SessionCommandError::Transport(
                AuthTransportError::Unauthorized
            ))
        ));
        assert!(
            block_on(store.load())
                .expect("load after rejection")
                .is_none()
        );
        assert!(matches!(controller.state(), SessionState::Expired { .. }));
    }

    #[test]
    fn revoke_and_clear_secure_session_removes_persisted_credentials_only_after_server_success() {
        let transport = FakeTransport {
            result: Ok(IssuedSession::new(
                "session-1",
                "access-secret",
                "refresh-secret",
                "a",
                "r",
            )
            .expect("session")),
        };
        let store = InMemorySessionStore::default();
        let mut controller = SessionController::default();
        let request = LoginRequest::new("principal-1", "password", "desktop").expect("request");
        block_on(controller.login(&transport, request, "/platform")).expect("login");
        block_on(controller.persist_secure_session(&store)).expect("persist");

        block_on(controller.revoke_and_clear_secure_session(&transport, &store, "/platform"))
            .expect("server revoke and secure clear");
        assert!(block_on(store.load()).expect("load after revoke").is_none());
        assert!(controller.access_token().is_none());
        assert!(matches!(controller.state(), SessionState::SignedOut { .. }));
    }

    #[test]
    fn login_refresh_and_revoke_are_explicit_state_transitions() {
        let transport = FakeTransport {
            result: Ok(IssuedSession::new(
                "session-1",
                "access-secret",
                "refresh-secret",
                "a",
                "r",
            )
            .expect("session")),
        };
        let request = LoginRequest::new("principal-1", "password", "desktop").expect("request");
        let mut controller = SessionController::default();
        block_on(controller.login(&transport, request, "/platform/customers")).expect("login");
        assert!(matches!(controller.state(), SessionState::Authenticated(_)));
        assert_eq!(
            controller
                .metadata()
                .and_then(|value| value.principal_label.as_deref()),
            Some("principal-1")
        );
        assert_eq!(
            controller.access_token().map(AccessToken::as_str),
            Some("access-secret")
        );
        assert!(!format!("{controller:?}").contains("access-secret"));
        let existing_metadata = controller.metadata().cloned().unwrap_or_default();
        controller.set_metadata(SafeSessionMetadata {
            support: Some(crate::SupportAccessMetadata {
                ticket_label: "SUP-1".into(),
                expires_at: "2099-01-01T00:00:00Z".into(),
                mode_label: "Diagnostic read".into(),
            }),
            read_only: Some(crate::ReadOnlyMetadata {
                reason: "billing grace".into(),
            }),
            ..existing_metadata
        });
        assert!(matches!(
            controller.state().route_state(),
            RouteState::ReadOnly { .. }
        ));

        block_on(controller.refresh(&transport, "/platform/customers")).expect("refresh");
        assert!(matches!(controller.state(), SessionState::Authenticated(_)));
        assert_eq!(
            controller.session_id().map(SessionId::as_str),
            Some("session-2")
        );
        assert!(
            controller
                .metadata()
                .and_then(|value| value.support.as_ref())
                .is_some()
        );
        assert!(matches!(
            controller.state().route_state(),
            RouteState::ReadOnly { .. }
        ));

        block_on(controller.revoke(&transport, "/platform/customers")).expect("revoke");
        assert!(matches!(controller.state(), SessionState::SignedOut { .. }));
        assert!(controller.access_token().is_none());
    }

    #[test]
    fn unauthorized_and_offline_states_do_not_claim_success() {
        let transport = FakeTransport {
            result: Err(AuthTransportError::Unauthorized),
        };
        let mut controller = SessionController::default();
        let request = LoginRequest::new("principal-1", "password", "desktop").expect("request");
        assert!(matches!(
            block_on(controller.login(&transport, request, "/platform")),
            Err(SessionCommandError::Transport(
                AuthTransportError::Unauthorized
            ))
        ));
        assert!(matches!(controller.state(), SessionState::Expired { .. }));
        assert!(matches!(
            controller.state().route_state(),
            RouteState::SessionExpired { .. }
        ));
    }
}
