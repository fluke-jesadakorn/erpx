//! Process-local access credentials used by native transports.
//!
//! Persistent refresh/session material belongs in the platform secure-session
//! store. This slot intentionally owns only the short-lived credential needed
//! by an active HTTP or gRPC request.

use std::{
    fmt,
    sync::{Arc, RwLock},
};

#[derive(Default)]
struct SecretBytes(Vec<u8>);

impl SecretBytes {
    fn new(value: impl Into<Vec<u8>>) -> Self {
        Self(value.into())
    }

    fn expose<R>(&self, use_secret: impl FnOnce(&str) -> R) -> Option<R> {
        std::str::from_utf8(&self.0).ok().map(use_secret)
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        // This is a best-effort overwrite of the owned allocation. The type is
        // deliberately not Clone, so replacing a slot does not leave another
        // application-owned copy behind.
        self.0.fill(0);
    }
}

#[derive(Default)]
struct CredentialState {
    generation: u64,
    access_token: Option<SecretBytes>,
}

/// Replaceable, process-local credential storage shared by native transports.
///
/// Debug output is always redacted. Consumers receive access through a closure
/// so the slot never exposes a borrowed secret beyond its read lock.
#[derive(Clone, Default)]
pub struct RuntimeCredentialSlot {
    state: Arc<RwLock<CredentialState>>,
}

impl RuntimeCredentialSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomically replaces the active access token and returns its generation.
    pub fn replace(&self, access_token: impl Into<Vec<u8>>) -> u64 {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|error| error.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.access_token = Some(SecretBytes::new(access_token));
        state.generation
    }

    /// Clears the active token after logout, revocation, or terminal refresh
    /// failure. The generation changes even when the slot was already empty.
    pub fn clear(&self) -> u64 {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|error| error.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.access_token = None;
        state.generation
    }

    pub fn generation(&self) -> u64 {
        self.state
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .generation
    }

    pub fn is_populated(&self) -> bool {
        self.state
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .access_token
            .is_some()
    }

    /// Invokes `use_token` while the current credential is protected from
    /// concurrent replacement. Returning the token from the closure is
    /// discouraged because it creates an unmanaged copy.
    pub fn with_access_token<R>(&self, use_token: impl FnOnce(&str) -> R) -> Option<R> {
        let state = self.state.read().unwrap_or_else(|error| error.into_inner());
        state.access_token.as_ref()?.expose(use_token)
    }
}

impl fmt::Debug for RuntimeCredentialSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state.read().unwrap_or_else(|error| error.into_inner());
        formatter
            .debug_struct("RuntimeCredentialSlot")
            .field("generation", &state.generation)
            .field(
                "access_token",
                &state.access_token.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_can_be_replaced_and_cleared_across_clones() {
        let slot = RuntimeCredentialSlot::new();
        let transport_slot = slot.clone();
        assert!(!slot.is_populated());

        assert_eq!(slot.replace("first-token"), 1);
        assert_eq!(
            transport_slot.with_access_token(str::to_owned).as_deref(),
            Some("first-token")
        );

        assert_eq!(slot.replace("refreshed-token"), 2);
        assert_eq!(
            transport_slot.with_access_token(str::to_owned).as_deref(),
            Some("refreshed-token")
        );

        assert_eq!(transport_slot.clear(), 3);
        assert!(!slot.is_populated());
        assert_eq!(slot.with_access_token(str::to_owned), None);
    }

    #[test]
    fn debug_output_never_contains_the_secret() {
        let slot = RuntimeCredentialSlot::new();
        slot.replace("top-secret-bearer-value");
        let output = format!("{slot:?}");
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("top-secret-bearer-value"));
    }

    #[test]
    fn poisoned_locks_do_not_make_logout_unavailable() {
        let slot = RuntimeCredentialSlot::new();
        slot.replace("temporary-token");
        let state = Arc::clone(&slot.state);
        let _ = std::panic::catch_unwind(move || {
            let _guard = state.write().expect("lock before poisoning");
            panic!("poison credential lock");
        });
        slot.clear();
        assert!(!slot.is_populated());
    }
}
