use std::path::Path;

fn main() {
    // sqlx::migrate! embeds directory contents at compile time. Explicitly
    // watch every migration directory so adding a numbered migration cannot
    // leave an already-built migrator with a stale embedded set.
    for directory in [
        "migrations",
        "migrations_read",
        "migrations_tenant_write",
        "migrations_tenant_read",
    ] {
        let path = Path::new(directory);
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
