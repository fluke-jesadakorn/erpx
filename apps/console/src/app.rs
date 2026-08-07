#[cfg(not(feature = "server"))]
pub(crate) fn launch() {
    dioxus::launch(crate::App);
}

#[cfg(feature = "server")]
pub(crate) fn launch() {
    dioxus::server::serve(|| async { Ok(crate::server::router().await?) });
}
