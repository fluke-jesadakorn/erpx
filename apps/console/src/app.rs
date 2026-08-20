#[cfg(all(not(feature = "server"), not(feature = "native")))]
pub(crate) fn launch() {
    let mut headers = dioxus::fullstack::http::HeaderMap::new();
    headers.insert(
        dioxus::fullstack::http::HeaderName::from_static("x-erpx-csrf"),
        dioxus::fullstack::http::HeaderValue::from_static("console"),
    );
    dioxus::fullstack::set_request_headers(headers);
    dioxus::launch(crate::App);
}

#[cfg(all(feature = "native", not(feature = "server")))]
pub(crate) fn launch() {
    if !crate::platform::native_distribution_enabled() {
        eprintln!(
            "ERPX Native distribution is disabled by ERPX_NATIVE_RELEASE_KILL_SWITCH; no renderer fallback was started"
        );
        return;
    }
    // Keep the native renderer selection explicit: this is Blitz/WGPU and must
    // never silently fall through to a desktop WebView launcher.
    let spike = std::env::var_os("ERPX_NATIVE_SPIKE").is_some()
        || std::env::var_os("ERPX_NATIVE_MACOS_SMOKE").is_some();
    if spike {
        dioxus::native::launch_cfg(crate::native_spike::NativeSpike, Vec::new(), Vec::new());
    } else {
        dioxus::native::launch_cfg(crate::App, Vec::new(), Vec::new());
    }
}

#[cfg(feature = "server")]
pub(crate) fn launch() {
    dioxus::server::serve(|| async { Ok(crate::server::router().await?) });
}
