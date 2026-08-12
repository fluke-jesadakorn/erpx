use dioxus::prelude::*;
use dioxus_icons::lucide::{Heart, HeartOff};

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut liked = use_signal(|| false);

    rsx! {
        main {
            style: "display:grid;min-height:100vh;place-items:center;font:16px system-ui,sans-serif;background:#fff7ed;color:#111827;",
            button {
                onclick: move |_| liked.set(!liked()),
                style: "display:inline-flex;align-items:center;gap:0.5rem;border:1px solid #fed7aa;border-radius:6px;background:white;color:#c2410c;padding:0.625rem 0.875rem;font:inherit;",
                if liked() {
                    HeartOff { size: 18 }
                    "Undo favorite"
                } else {
                    Heart { size: 18 }
                    "Favorite"
                }
            }
        }
    }
}
