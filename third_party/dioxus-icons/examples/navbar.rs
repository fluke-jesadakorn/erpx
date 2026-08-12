use dioxus::prelude::*;
use dioxus_icons::lucide::{Bell, ChevronDown, Menu, Search, Settings, User};

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        header {
            style: "display:flex;align-items:center;gap:1rem;padding:0.875rem 1rem;border-bottom:1px solid #e5e7eb;font:14px system-ui,sans-serif;color:#111827;",
            button { style: nav_button_style(), Menu { size: 20 } }
            strong { style: "margin-right:auto;", "Workspace" }
            label {
                style: "display:flex;align-items:center;gap:0.5rem;min-width:18rem;border:1px solid #d1d5db;border-radius:6px;padding:0.5rem 0.625rem;color:#6b7280;",
                Search { size: 16 }
                input {
                    aria_label: "Search",
                    placeholder: "Search",
                    style: "width:100%;border:0;outline:0;font:inherit;color:#111827;"
                }
            }
            button { style: nav_button_style(), Bell { size: 18 } }
            button { style: nav_button_style(), Settings { size: 18 } }
            button {
                style: "display:inline-flex;align-items:center;gap:0.375rem;border:0;background:transparent;color:inherit;font:inherit;padding:0.45rem;",
                User { size: 18 }
                "Evan"
                ChevronDown { size: 16 }
            }
        }
    }
}

fn nav_button_style() -> &'static str {
    "display:inline-grid;place-items:center;width:2.25rem;height:2.25rem;border:1px solid #e5e7eb;border-radius:6px;background:white;color:#374151;"
}
