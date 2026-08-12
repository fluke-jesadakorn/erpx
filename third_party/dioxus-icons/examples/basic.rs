use dioxus::prelude::*;
use dioxus_icons::lucide::Trash;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        main {
            style: "display:grid;min-height:100vh;place-items:center;font:16px system-ui,sans-serif;color:#1f2937;background:#f8fafc;",
            button {
                style: "display:inline-flex;align-items:center;gap:0.5rem;border:1px solid #d1d5db;border-radius:6px;background:white;color:#b91c1c;padding:0.625rem 0.875rem;font:inherit;",
                Trash { size: 16 }
                "Delete"
            }
        }
    }
}
