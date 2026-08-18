//! Navigation adapters shared by the Web and native composition roots.

use std::{fmt, rc::Rc};

use dioxus::prelude::History;

use crate::platform::{
    DeepLinkSubscriber, DioxusNavigationHost, NavigationHost, UnsafeDeepLink, validate_deep_link,
};

/// Final route check supplied by the console's authoritative route registry.
pub type RouteValidator = Rc<dyn Fn(&str) -> bool>;

/// Native navigation backed by Dioxus' in-process history.
///
/// Startup URLs and later protocol callbacks enter through the same validated
/// path, so neither can bypass the route registry.
#[derive(Clone)]
pub struct MacOsNavigationHost {
    inner: DioxusNavigationHost,
    route_validator: RouteValidator,
}

impl fmt::Debug for MacOsNavigationHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MacOsNavigationHost")
            .field("current_path", &self.current_path())
            .finish_non_exhaustive()
    }
}

impl MacOsNavigationHost {
    pub fn new(
        initial_path: &str,
        route_validator: RouteValidator,
    ) -> Result<Self, UnsafeDeepLink> {
        let initial_path = checked_route(initial_path, &route_validator)?;
        Ok(Self {
            inner: DioxusNavigationHost::memory(&initial_path)?,
            route_validator,
        })
    }

    pub fn with_history(
        history: Rc<dyn History>,
        route_validator: RouteValidator,
    ) -> Result<Self, UnsafeDeepLink> {
        checked_route(&history.current_route(), &route_validator)?;
        Ok(Self {
            inner: DioxusNavigationHost::new(history),
            route_validator,
        })
    }

    /// Apply a URL delivered by the application delegate or custom-protocol
    /// handler. The OS integration owns URL-to-path extraction; this boundary
    /// accepts only local console paths.
    pub fn receive_protocol_callback(&self, path: &str) -> Result<(), UnsafeDeepLink> {
        self.receive_deep_link(path)
    }

    /// Validate and apply a custom-scheme URL such as
    /// `erpx://console/finance`. Authority is fixed to `console`; query and
    /// fragment input is rejected by the local-path validator.
    pub fn receive_protocol_url(
        &self,
        url: &str,
        expected_scheme: &str,
    ) -> Result<(), UnsafeDeepLink> {
        self.receive_deep_link(protocol_url_path(url, expected_scheme)?)
    }

    fn checked(&self, path: &str) -> Result<String, UnsafeDeepLink> {
        checked_route(path, &self.route_validator)
    }
}

pub fn protocol_url_path<'a>(
    url: &'a str,
    expected_scheme: &str,
) -> Result<&'a str, UnsafeDeepLink> {
    if expected_scheme.is_empty()
        || !expected_scheme.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')
        })
    {
        return Err(UnsafeDeepLink);
    }
    let prefix = format!("{expected_scheme}://console");
    let path = url.strip_prefix(&prefix).ok_or(UnsafeDeepLink)?;
    validate_deep_link(path)?;
    Ok(path)
}

fn checked_route(path: &str, route_validator: &RouteValidator) -> Result<String, UnsafeDeepLink> {
    let path = validate_deep_link(path)?;
    if route_validator(&path) {
        Ok(path)
    } else {
        Err(UnsafeDeepLink)
    }
}

impl NavigationHost for MacOsNavigationHost {
    fn current_path(&self) -> String {
        self.inner.current_path()
    }

    fn push(&self, path: &str) -> Result<(), UnsafeDeepLink> {
        self.inner.push(&self.checked(path)?)
    }

    fn replace(&self, path: &str) -> Result<(), UnsafeDeepLink> {
        self.inner.replace(&self.checked(path)?)
    }

    fn back(&self) {
        self.inner.back();
    }

    fn forward(&self) {
        self.inner.forward();
    }

    fn subscribe_deep_links(&self, subscriber: DeepLinkSubscriber) {
        self.inner.subscribe_deep_links(subscriber);
    }

    fn receive_deep_link(&self, path: &str) -> Result<(), UnsafeDeepLink> {
        self.inner.receive_deep_link(&self.checked(path)?)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;

    fn registered_routes() -> RouteValidator {
        Rc::new(|path| matches!(path, "/" | "/finance" | "/finance/ledger"))
    }

    #[test]
    fn startup_and_protocol_routes_use_registry_validation() {
        assert!(MacOsNavigationHost::new("/unknown", registered_routes()).is_err());
        let navigation =
            MacOsNavigationHost::new("/", registered_routes()).expect("registered startup route");
        assert!(navigation.receive_protocol_callback("/unknown").is_err());
        navigation
            .receive_protocol_callback("/finance")
            .expect("registered callback route");
        assert_eq!(navigation.current_path(), "/finance");
        navigation
            .receive_protocol_url("erpx://console/finance/ledger", "erpx")
            .expect("validated custom protocol URL");
        assert_eq!(navigation.current_path(), "/finance/ledger");
        assert!(
            navigation
                .receive_protocol_url("erpx://evil.test/finance", "erpx")
                .is_err()
        );
    }

    #[test]
    fn callbacks_notify_only_after_validation() {
        let navigation =
            MacOsNavigationHost::new("/", registered_routes()).expect("registered startup route");
        let received = Rc::new(RefCell::new(Vec::new()));
        let subscriber = Rc::clone(&received);
        navigation.subscribe_deep_links(Rc::new(move |path| {
            subscriber.borrow_mut().push(path);
        }));

        assert!(navigation.receive_protocol_callback("/unknown").is_err());
        navigation
            .receive_protocol_callback("/finance/ledger")
            .expect("registered callback route");
        assert_eq!(received.borrow().as_slice(), ["/finance/ledger"]);
    }
}
