fn main() {
    #[cfg(feature = "preview-provider")]
    {
        // Explicit build environment only; this never installs a native library.
        // Keep native search/link flags out of the portable/API dependency graph.
        pkg_config::Config::new()
            .atleast_version("8.18.7")
            .probe("vips")
            .expect("preview-provider requires a qualified libvips development prefix");
    }
}
