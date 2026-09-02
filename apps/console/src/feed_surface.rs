#![forbid(unsafe_code)]

use chrono::{DateTime, NaiveDate, Utc};
use dioxus::prelude::*;
use erpx_delivery_contract::Workspace;
use erpx_kernel::{AuthorityScope, HoldingId, feed as kernel_feed};
use erpx_ui_core::Locale;
use erpx_ui_foundation::feed::{
    ComposerState, FeedCardView, HomeFeedFilter, group_by_day, level_badge_for_scope,
    workspace_chip_for_context,
};
use uuid::Uuid;

fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewerLevel {
    Platform,
    Holding(HoldingId),
    Company(HoldingId, Uuid),
    Branch(HoldingId, Uuid, Uuid),
}

impl ViewerLevel {
    fn to_scope(&self) -> AuthorityScope {
        match self {
            Self::Platform => AuthorityScope::Platform,
            Self::Holding(h) => AuthorityScope::Holding {
                holding_id: h.as_uuid(),
            },
            Self::Company(h, cid) => AuthorityScope::Company {
                holding_id: h.as_uuid(),
                company_id: *cid,
            },
            Self::Branch(h, cid, bid) => AuthorityScope::Branch {
                holding_id: h.as_uuid(),
                company_id: *cid,
                branch_id: *bid,
            },
        }
    }
    fn label(&self, locale: Locale) -> String {
        match (self, locale) {
            (Self::Platform, Locale::En) => "Platform — sees all holdings".into(),
            (Self::Platform, Locale::Th) => "แพลตฟอร์ม — เห็นทุกโฮลดิ้ง".into(),
            (Self::Holding(_), Locale::En) => "CEO / Holding — sees every Company/Branch".into(),
            (Self::Holding(_), Locale::Th) => "CEO / โฮลดิ้ง — เห็นทุกบริษัท/สาขา".into(),
            (Self::Company(_, _), Locale::En) => "Manager / Company — sees its Branches".into(),
            (Self::Company(_, _), Locale::Th) => "ผู้จัดการ / บริษัท — เห็นสาขาในบริษัท".into(),
            (Self::Branch(_, _, _), Locale::En) => "Staff / Branch — sees only branch".into(),
            (Self::Branch(_, _, _), Locale::Th) => "พนักงาน / สาขา — เห็นเฉพาะสาขา".into(),
        }
    }
}

fn demo_holding_id() -> HoldingId {
    HoldingId::try_from_uuid(Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap())
        .unwrap()
}
fn demo_company_id() -> Uuid {
    Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()
}
fn demo_branch_id() -> Uuid {
    Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap()
}
fn demo_branch2_id() -> Uuid {
    Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap()
}

fn mock_holding_scope() -> AuthorityScope {
    AuthorityScope::Holding {
        holding_id: demo_holding_id().as_uuid(),
    }
}
fn mock_company_scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: demo_holding_id().as_uuid(),
        company_id: demo_company_id(),
    }
}
fn mock_branch_scope() -> AuthorityScope {
    AuthorityScope::Branch {
        holding_id: demo_holding_id().as_uuid(),
        company_id: demo_company_id(),
        branch_id: demo_branch_id(),
    }
}
fn mock_branch2_scope() -> AuthorityScope {
    AuthorityScope::Branch {
        holding_id: demo_holding_id().as_uuid(),
        company_id: demo_company_id(),
        branch_id: demo_branch2_id(),
    }
}

fn day_label_for(bucket: NaiveDate) -> String {
    let today = Utc::now().date_naive();
    kernel_feed::day_bucket_label(today, bucket).to_string()
}

const DEMO_RECORD_TOKENS: [&str; 5] = [
    "finance/journal-2026-117",
    "sales/quote-2026-042",
    "project/proj-2026-001",
    "procurement/req-2026-014",
    "inventory/move-2026-883",
];
const DEMO_HASHTAGS: [&str; 5] = ["launch", "q4_2026", "close", "audit", "retention"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatComment {
    pub comment_id: Uuid,
    pub feed_item_id: Uuid,
    pub author_principal_id: Uuid,
    pub author_display_name: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
    pub hashtags: Vec<String>,
    pub record_refs: Vec<String>,
    pub tombstone: bool,
    pub author_scope: AuthorityScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommentInput {
    pub text: String,
    pub hashtags: Vec<String>,
}

fn record_scope_for_token(token: &str) -> AuthorityScope {
    if token.starts_with("finance/") || token.starts_with("project/") {
        mock_holding_scope()
    } else if token.starts_with("sales/") || token.starts_with("procurement/") {
        mock_company_scope()
    } else if token.starts_with("inventory/") {
        mock_branch2_scope()
    } else {
        mock_holding_scope()
    }
}

fn extract_record_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        if let Some(stripped) = raw.strip_prefix('@') {
            let cleaned = stripped
                .trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | ')' | '(' | '"' | '\''))
                .trim_end_matches(|c: char| c == '!' || c == '?')
                .to_string();
            if cleaned.contains('/') && !cleaned.is_empty() && !out.contains(&cleaned) {
                out.push(cleaned);
            }
        }
    }
    out
}

fn extract_hashtags_from_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        if let Some(stripped) = raw.strip_prefix('#') {
            let cleaned = stripped
                .trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | ')' | '(' | '"' | '\''))
                .trim()
                .to_ascii_lowercase();
            if !cleaned.is_empty()
                && kernel_feed::validate_hashtag(&cleaned).is_ok()
                && !out.contains(&cleaned)
            {
                out.push(cleaned);
            }
        }
    }
    out
}

fn card_mention_token(workspace_slug: &str) -> &'static str {
    match workspace_slug {
        "finance" => "finance/journal-2026-117",
        "sales" => "sales/quote-2026-042",
        "project" => "project/proj-2026-001",
        "procurement" => "procurement/req-2026-014",
        "inventory" => "inventory/move-2026-883",
        _ => "finance/journal-2026-117",
    }
}

fn generate_mock_items(now: DateTime<Utc>) -> Vec<(FeedCardView, AuthorityScope)> {
    let hid = demo_holding_id().as_uuid();
    let today = now.date_naive();
    let yesterday = today - chrono::Duration::days(1);
    let earlier = today - chrono::Duration::days(3);
    let principals = [
        (
            Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap(),
            "CEO Amara",
        ),
        (
            Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap(),
            "Manager Boon",
        ),
        (
            Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-cccccccccccc").unwrap(),
            "Staff Chai",
        ),
        (
            Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap(),
            "Staff Dao (branch2)",
        ),
    ];
    let workspaces = [
        "finance",
        "sales",
        "procurement",
        "project",
        "inventory",
        "people",
        "marketing",
        "operations",
    ];
    let hashtags_pool = vec!["launch", "q4_2026", "close", "audit", "retention"];
    let mut items: Vec<(FeedCardView, AuthorityScope)> = Vec::new();
    for (idx, ws) in workspaces.iter().enumerate() {
        let scope = match idx % 4 {
            0 => mock_holding_scope(),
            1 => mock_company_scope(),
            2 => mock_branch_scope(),
            _ => mock_branch2_scope(),
        };
        let created_at = match idx % 3 {
            0 => now - chrono::Duration::minutes((idx as i64) * 13),
            1 => now - chrono::Duration::days(1) - chrono::Duration::minutes((idx as i64) * 7),
            _ => now - chrono::Duration::days(3) - chrono::Duration::minutes((idx as i64) * 5),
        };
        let bucket = created_at.date_naive();
        let day_label = day_label_for(bucket);
        let (pid, name) = principals[idx % principals.len()];
        let level_badge = level_badge_for_scope(&scope).to_string();
        let hashtags = if idx % 2 == 0 {
            vec!["launch".to_string()]
        } else {
            vec![]
        };
        let hashtags = if idx == 1 {
            vec!["launch".to_string(), "q4_2026".to_string()]
        } else {
            hashtags
        };
        let _ = hashtags_pool;
        let body_suffix = match *ws {
            "finance" => " posted journal @finance/journal-2026-117 + #launch",
            "sales" => " created quote @sales/quote-2026-042",
            "project" => " updated @project/proj-2026-001 — dependsOn @procurement/req-2026-014",
            "procurement" => " receipt @inventory/move-2026-883 fulfills @procurement/req-2026-014",
            _ => " operation in workspace",
        };
        let tombstone = idx == 6;
        let classification = match idx % 4 {
            0 => "internal",
            1 => "confidential",
            2 => "internal",
            _ => "public",
        };
        let card = FeedCardView {
            feed_item_id: Uuid::new_v4(),
            holding_id: hid,
            workspace_slug: ws.to_string(),
            author_principal_id: pid,
            author_display_name: name.to_string(),
            visibility_scope_label: level_badge.clone(),
            classification: classification.to_string(),
            created_at,
            day_bucket: bucket,
            day_label,
            workspace_chip: workspace_chip_for_context(ws),
            level_badge,
            hashtags,
            tombstone,
            stale: false,
        };
        // attach body via hack: store in display_name? Actually FeedCardView has no body field; use author_display_name + hashtags to represent.
        // We will render card body as workspace-specific suffix in component.
        let _ = body_suffix;
        items.push((card, scope));
    }
    // Add extra today items for Home aggregation demo
    for i in 0..3 {
        let scope = mock_holding_scope();
        let created_at = now - chrono::Duration::minutes(i * 17 + 5);
        let card = FeedCardView {
            feed_item_id: Uuid::new_v4(),
            holding_id: hid,
            workspace_slug: ["finance", "project", "sales"][i as usize].to_string(),
            author_principal_id: principals[0].0,
            author_display_name: principals[0].1.to_string(),
            visibility_scope_label: level_badge_for_scope(&scope).to_string(),
            classification: "internal".to_string(),
            created_at,
            day_bucket: today,
            day_label: "Today".to_string(),
            workspace_chip: workspace_chip_for_context(["finance", "project", "sales"][i as usize]),
            level_badge: level_badge_for_scope(&scope).to_string(),
            hashtags: vec!["launch".to_string()],
            tombstone: false,
            stale: false,
        };
        items.push((card, scope));
    }
    let _ = yesterday;
    let _ = earlier;
    items
}

fn filter_visible(
    items: &[(FeedCardView, AuthorityScope)],
    viewer_scope: &AuthorityScope,
) -> Vec<FeedCardView> {
    items
        .iter()
        .filter(|(_, item_scope)| viewer_scope.contains(item_scope))
        .map(|(card, _)| card.clone())
        .collect()
}

fn apply_filter(
    cards: Vec<FeedCardView>,
    filter: &HomeFeedFilter,
    hashtag: Option<&str>,
    context: Option<&str>,
    my_author: Option<Uuid>,
) -> Vec<FeedCardView> {
    cards
        .into_iter()
        .filter(|c| match filter {
            HomeFeedFilter::All => true,
            HomeFeedFilter::Following => c.hashtags.contains(&"launch".to_string()),
            HomeFeedFilter::Hashtag(tag) => c.hashtags.iter().any(|t| t == tag),
            HomeFeedFilter::Context(ctx) => c.workspace_slug == *ctx,
            HomeFeedFilter::MyDay => my_author.is_some_and(|id| c.author_principal_id == id),
        })
        .filter(|c| {
            if let Some(tag) = hashtag {
                c.hashtags.iter().any(|t| t == tag)
            } else {
                true
            }
        })
        .filter(|c| {
            if let Some(ctx) = context {
                c.workspace_slug == ctx
            } else {
                true
            }
        })
        .collect()
}

#[component]
pub fn HomeFeedPage() -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let session = use_context::<erpx_delivery_contract::SessionBootstrap>();
    let viewer_principal = session
        .actor_ref
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(|| Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap());
    let hid = demo_holding_id();
    let mut viewer_level = use_signal(|| ViewerLevel::Holding(hid));
    let _composer = use_signal(ComposerState::new);
    let mut filter = use_signal(|| HomeFeedFilter::All);
    let mut selected_hashtag: Signal<Option<String>> = use_signal(|| None);
    let mut selected_context: Signal<Option<String>> = use_signal(|| None);
    let mut show_following = use_signal(|| false);
    let now = Utc::now();
    let mut mock_store = use_signal(|| generate_mock_items(now));
    let reaction_map: Signal<std::collections::BTreeMap<Uuid, String>> =
        use_signal(|| std::collections::BTreeMap::new());
    let following_map: Signal<std::collections::BTreeSet<String>> =
        use_signal(|| std::collections::BTreeSet::new());
    let mut my_day_only = use_signal(|| false);
    let mut new_item_text = use_signal(String::new);
    let mut new_hashtags = use_signal(Vec::<String>::new);
    let mut hashtag_input = use_signal(String::new);
    let mut mention_query: Signal<Option<String>> = use_signal(|| None);
    let mut hashtag_query: Signal<Option<String>> = use_signal(|| None);
    let mut composer_error: Signal<Option<String>> = use_signal(|| None);
    let comments_map: Signal<std::collections::BTreeMap<Uuid, Vec<FlatComment>>> =
        use_signal(|| std::collections::BTreeMap::new());
    let expanded_comments: Signal<std::collections::BTreeSet<Uuid>> =
        use_signal(|| std::collections::BTreeSet::new());
    let show_all_comments: Signal<std::collections::BTreeSet<Uuid>> =
        use_signal(|| std::collections::BTreeSet::new());

    let viewer_scope = viewer_level.read().to_scope();
    let visible_cards = filter_visible(&mock_store.read(), &viewer_scope);
    let total_visible = visible_cards.len();
    let filtered = {
        let mut f = filter.read().clone();
        if *show_following.read() {
            f = HomeFeedFilter::Following;
        }
        if *my_day_only.read() {
            f = HomeFeedFilter::MyDay;
        }
        let ht = selected_hashtag.read().clone();
        let ctx = selected_context.read().clone();
        let mut cards = visible_cards.clone();
        // apply composite filters
        if ht.is_some()
            || ctx.is_some()
            || matches!(f, HomeFeedFilter::Following | HomeFeedFilter::MyDay)
        {
            cards = apply_filter(
                cards,
                &f,
                ht.as_deref(),
                ctx.as_deref(),
                if *my_day_only.read() {
                    Some(viewer_principal)
                } else {
                    None
                },
            );
        } else if matches!(f, HomeFeedFilter::Hashtag(_) | HomeFeedFilter::Context(_)) {
            cards = apply_filter(cards, &f, None, None, None);
        }
        cards
    };
    let groups = group_by_day(filtered.clone());
    let total_filtered = filtered.len();

    let on_create_post = {
        move |_| {
            let text = new_item_text.read().trim().to_string();
            let tags = new_hashtags.read().clone();
            let ws = selected_context
                .read()
                .clone()
                .unwrap_or_else(|| "finance".to_string());
            if text.is_empty() && tags.is_empty() {
                composer_error.set(Some("Post cannot be empty".into()));
                return;
            }
            if tags.len() > kernel_feed::MAX_HASHTAGS_PER_ITEM {
                composer_error.set(Some("Too many hashtags".into()));
                return;
            }
            for t in &tags {
                if kernel_feed::validate_hashtag(t).is_err() {
                    composer_error.set(Some(format!("Invalid hashtag: {}", t)));
                    return;
                }
            }
            composer_error.set(None);
            let now = Utc::now();
            let hid_uuid = demo_holding_id().as_uuid();
            let scope = mock_holding_scope();
            let card = FeedCardView {
                feed_item_id: Uuid::new_v4(),
                holding_id: hid_uuid,
                workspace_slug: ws.clone(),
                author_principal_id: viewer_principal,
                author_display_name: "You".to_string(),
                visibility_scope_label: level_badge_for_scope(&scope).to_string(),
                classification: "internal".to_string(),
                created_at: now,
                day_bucket: now.date_naive(),
                day_label: "Today".to_string(),
                workspace_chip: workspace_chip_for_context(&ws),
                level_badge: level_badge_for_scope(&scope).to_string(),
                hashtags: tags.clone(),
                tombstone: false,
                stale: false,
            };
            mock_store.write().push((card, scope));
            new_item_text.set(String::new());
            new_hashtags.set(Vec::new());
            hashtag_input.set(String::new());
        }
    };

    rsx! {
        section { class: "feed-page", "data-testid": "home-feed",
            div { class: "feed-header",
                h1 { {copy(locale, "Home — Activity Feed", "หน้าแรก — ฟีดกิจกรรม")} }
                p { {copy(locale,
                    "Single day-grouped timeline across all workspaces. Every operation auto-publishes; visibility is AuthorityScope.contains() + Audience. CEO at Holding sees every Company/Branch; Branch sees only branch.",
                    "ไทม์ไลน์รวมทุกพื้นที่ทำงาน แยกตามวัน ทุกการดำเนินการเผยแพร่อัตโนมัติ การมองเห็นผ่าน AuthorityScope.contains() + Audience CEO ที่ Holding เห็นทุก Company/Branch ส่วน Branch เห็นเฉพาะสาขา")} }
                div { class: "feed-level-toggle", role: "group", aria_label: "Level toggle",
                    span { {copy(locale, "View as:", "ดูในฐานะ:")} }
                    for (label, lvl) in [
                        ("CEO Holding", ViewerLevel::Holding(hid)),
                        ("Manager Company", ViewerLevel::Company(hid, demo_company_id())),
                        ("Staff Branch", ViewerLevel::Branch(hid, demo_company_id(), demo_branch_id())),
                        ("Platform", ViewerLevel::Platform),
                    ] {
                        {
                            let is_active = *viewer_level.read() == lvl;
                            let lvl_clone = lvl.clone();
                            rsx! {
                                button {
                                    class: if is_active { "feed-level-btn feed-level-btn--active" } else { "feed-level-btn" },
                                    onclick: move |_| viewer_level.set(lvl_clone.clone()),
                                    "{label}"
                                }
                            }
                        }
                    }
                    span { style: "margin-left:8px;font-size:11px;color:#8aa0b3;", "{viewer_level.read().label(locale)} · {total_filtered}/{total_visible} visible" }
                }
            }

            div { class: "feed-composer", "data-testid": "feed-composer",
                h3 { style: "margin:0;font-size:13px;color:#e8eef5;", {copy(locale, "Create post — @ mention + # hashtag", "สร้างโพสต์ — @ กล่าวถึง + # แฮชแท็ก")} }
                textarea {
                    placeholder: copy(locale, "What's happening? Use @finance/inv-... @sales/quote-... and #launch", "เกิดอะไรขึ้น? ใช้ @finance/inv-... @sales/quote-... และ #launch"),
                    value: "{new_item_text}",
                    oninput: move |e: FormEvent| {
                        let v = e.value();
                        new_item_text.set(v.clone());
                        if let Some(at_pos) = v.rfind('@') {
                            let q = v[at_pos+1..].split_whitespace().next().unwrap_or("").to_string();
                            if !q.is_empty() { mention_query.set(Some(q)); } else { mention_query.set(None); }
                        } else { mention_query.set(None); }
                        if let Some(hash_pos) = v.rfind('#') {
                            let q = v[hash_pos+1..].split_whitespace().next().unwrap_or("").to_string();
                            if !q.is_empty() { hashtag_query.set(Some(q)); } else { hashtag_query.set(None); }
                        } else { hashtag_query.set(None); }
                    },
                }
                if let Some(q) = mention_query.read().as_ref() {
                    div { class: "feed-autocomplete",
                        div { class: "feed-autocomplete__list",
                            for item in ["finance/journal-2026-117", "sales/quote-2026-042", "project/proj-2026-001", "procurement/req-2026-014", "inventory/move-2026-883"].iter().filter(|s| s.to_ascii_lowercase().contains(&q.to_ascii_lowercase())) {
                                {
                                    let item_str = item.to_string();
                                    let initial = item_str.chars().next().unwrap_or('F').to_ascii_uppercase().to_string();
                                    let sub = match item_str.as_str() {
                                        "finance/journal-2026-117" => "Finance · Holding · Journal 117",
                                        "sales/quote-2026-042" => "Sales · Company · Quote 042",
                                        "project/proj-2026-001" => "Projects · Holding · Proj 001",
                                        "procurement/req-2026-014" => "Procurement · Company · Req 014",
                                        "inventory/move-2026-883" => "Inventory · Branch · Move 883",
                                        _ => "Record",
                                    };
                                    let badge = item_str.split('/').next().unwrap_or("record").to_string();
                                    rsx! {
                                        div { class: "feed-autocomplete__item", onclick: {
                                            let item = item_str.clone();
                                            move |_| {
                                                let mut t = new_item_text.read().clone();
                                                t.push_str(&format!(" @{}", item));
                                                new_item_text.set(t);
                                                mention_query.set(None);
                                            }
                                        },
                                            div { class: "feed-autocomplete__item-avatar", "{initial}" }
                                            div { class: "feed-autocomplete__item-main",
                                                div { class: "feed-autocomplete__item-name", "@{item_str}" }
                                                div { class: "feed-autocomplete__item-sub", "{sub}" }
                                            }
                                            span { class: "feed-autocomplete__item-badge", "{badge}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(q) = hashtag_query.read().as_ref() {
                    div { class: "feed-autocomplete",
                        div { class: "feed-autocomplete__list",
                            for tag in ["launch", "q4_2026", "close", "audit"].iter().filter(|t| t.to_ascii_lowercase().contains(&q.to_ascii_lowercase())) {
                                {
                                    let tag_str = tag.to_string();
                                    let initial = "#";
                                    let sub = match tag_str.as_str() {
                                        "launch" => "Topic · 24 posts · trending",
                                        "q4_2026" => "Topic · 12 posts",
                                        "close" => "Topic · 8 posts",
                                        "audit" => "Topic · 18 posts · #audit",
                                        _ => "Hashtag",
                                    };
                                    rsx! {
                                        div { class: "feed-autocomplete__item", onclick: {
                                            let tag = tag_str.clone();
                                            move |_| {
                                                let mut v = new_hashtags.read().clone();
                                                if !v.contains(&tag) && v.len() < 20 { v.push(tag.clone()); new_hashtags.set(v); }
                                                hashtag_query.set(None);
                                            }
                                        },
                                            div { class: "feed-autocomplete__item-avatar", style: "background: linear-gradient(135deg,#1c2533,#2a5a8a);", "{initial}" }
                                            div { class: "feed-autocomplete__item-main",
                                                div { class: "feed-autocomplete__item-name", "#{tag_str}" }
                                                div { class: "feed-autocomplete__item-sub", "{sub}" }
                                            }
                                            span { class: "feed-autocomplete__item-badge", "tag" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "feed-composer__row",
                    input {
                        r#type: "text",
                        placeholder: copy(locale, "Add #hashtag (2-64 a-z0-9_)", "เพิ่ม #แฮชแท็ก"),
                        value: "{hashtag_input}",
                        oninput: move |e: FormEvent| hashtag_input.set(e.value()),
                    }
                    button {
                        class: "feed-action-btn",
                        onclick: move |_| {
                            let raw = hashtag_input.read().trim().trim_start_matches('#').to_ascii_lowercase();
                            if kernel_feed::validate_hashtag(&raw).is_ok() {
                                let mut v = new_hashtags.read().clone();
                                if !v.contains(&raw) && v.len() < 20 { v.push(raw.clone()); new_hashtags.set(v); hashtag_input.set(String::new()); composer_error.set(None); } else if v.len() >= 20 { composer_error.set(Some("Too many hashtags".into())); }
                            } else { composer_error.set(Some("Invalid hashtag".into())); }
                        },
                        {copy(locale, "Add #", "เพิ่ม #")}
                    }
                    span { style: "font-size:11px;color:#8aa0b3;", {copy(locale, "Composer at Global Home + per-workspace (anchors to chosen record)", "ผู้เขียนที่หน้าแรกและแต่ละพื้นที่ทำงาน")} }
                }
                if !new_hashtags.read().is_empty() {
                    div { class: "feed-composer__chips",
                        for tag in new_hashtags.read().clone() {
                            span { class: "feed-chip feed-chip--hashtag", "#{tag}" }
                        }
                    }
                }
                div { class: "feed-composer__row",
                    select {
                        onchange: move |e: FormEvent| selected_context.set(Some(e.value())),
                        option { value: "", {copy(locale, "Anchor workspace (record topic)", "พื้นที่ทำงานอ้างอิง")} }
                        for ws in ["finance","sales","procurement","project","inventory","people","marketing","operations"] {
                            option { value: "{ws}", selected: selected_context.read().as_deref() == Some(ws), "{ws}" }
                        }
                    }
                    button { class: "feed-action-btn feed-action-btn--active", onclick: on_create_post, {copy(locale, "Post", "โพสต์")} }
                    if let Some(err) = composer_error.read().as_ref() {
                        span { style: "color:#f87171;font-size:12px;", "{err}" }
                    }
                }
                small { style: "color:#8aa0b3;font-size:11px;", {copy(locale, "Writes inherit Audience + classification; reactions/comments/shares cannot broaden (stricter-wins). Out-of-scope @ renders as tombstone.", "งานเขียนสืบทอด Audience + classification ปฏิกิริยา/ความคิดเห็น/แชร์ห้ามขยายวงกว้าง")} }
            }

            div { class: "feed-filter-bar", role: "toolbar", aria_label: "Feed filters",
                for (label, f) in [
                    (copy(locale, "All", "ทั้งหมด"), HomeFeedFilter::All),
                    (copy(locale, "Following", "กำลังติดตาม"), HomeFeedFilter::Following),
                    (copy(locale, "My Day", "งานวันนี้"), HomeFeedFilter::MyDay),
                ] {
                    {
                        let is_active = *filter.read() == f;
                        let f_clone = f.clone();
                        rsx! {
                            button {
                                class: if is_active { "feed-filter-btn feed-filter-btn--active" } else { "feed-filter-btn" },
                                onclick: move |_| { filter.set(f_clone.clone()); show_following.set(false); my_day_only.set(false); if f_clone == HomeFeedFilter::Following { show_following.set(true); } if f_clone == HomeFeedFilter::MyDay { my_day_only.set(true); } },
                                "{label}"
                            }
                        }
                    }
                }
                for tag in ["launch", "q4_2026", "audit"] {
                    {
                        let is_active = selected_hashtag.read().as_deref() == Some(tag);
                        let tag = tag.to_string();
                        rsx! {
                            button {
                                class: if is_active { "feed-filter-btn feed-filter-btn--active" } else { "feed-filter-btn" },
                                onclick: move |_| {
                                    let cur = selected_hashtag.read().clone();
                                    if cur.as_deref() == Some(tag.as_str()) { selected_hashtag.set(None); } else { selected_hashtag.set(Some(tag.clone())); }
                                },
                                "#{tag}"
                            }
                        }
                    }
                }
                for ws in ["finance","sales","project","procurement"] {
                    {
                        let is_active = selected_context.read().as_deref() == Some(ws);
                        let ws = ws.to_string();
                        rsx! {
                            button {
                                class: if is_active { "feed-filter-btn feed-filter-btn--active" } else { "feed-filter-btn" },
                                onclick: move |_| {
                                    let cur = selected_context.read().clone();
                                    if cur.as_deref() == Some(ws.as_str()) { selected_context.set(None); } else { selected_context.set(Some(ws.clone())); }
                                },
                                "{ws}"
                            }
                        }
                    }
                }
                if selected_hashtag.read().is_some() || selected_context.read().is_some() || *show_following.read() || *my_day_only.read() {
                    button { class: "feed-action-btn", onclick: move |_| { selected_hashtag.set(None); selected_context.set(None); show_following.set(false); my_day_only.set(false); filter.set(HomeFeedFilter::All); }, {copy(locale, "Clear", "ล้าง")} }
                }
            }

            if groups.is_empty() {
                div { class: "feed-empty", role: "status",
                    h3 { {copy(locale, "No feed items", "ยังไม่มีโพสต์")} }
                    p { {copy(locale, "No operations visible for current level/filter. Try All or switch to CEO Holding.", "ไม่มีการดำเนินการที่มองเห็นได้สำหรับระดับ/ตัวกรองปัจจุบัน ลอง All หรือสลับเป็น CEO Holding")} }
                }
            } else {
                for group in groups {
                    div { class: "feed-day-group", key: "{group.day_bucket}",
                        div { class: "feed-day-separator", "{group.label} · {group.day_bucket} — {group.items.len()} items" }
                        for card in group.items {
                            {
                                let feed_id = card.feed_item_id;
                                let has_reacted = reaction_map.read().contains_key(&feed_id);
                                let is_following = following_map.read().contains(&card.workspace_slug);
                                let comments = comments_map.read().get(&feed_id).cloned().unwrap_or_default();
                                let is_expanded = expanded_comments.read().contains(&feed_id);
                                let show_all = show_all_comments.read().contains(&feed_id);
                                let viewer_scope_for_card = viewer_level.read().to_scope();
                                let card_clone = card.clone();
                                rsx! {
                                    FeedCard {
                                        card: card_clone,
                                        has_reacted: has_reacted,
                                        is_following: is_following,
                                        comments: comments,
                                        is_expanded: is_expanded,
                                        show_all: show_all,
                                        viewer_scope: viewer_scope_for_card,
                                        on_react: {
                                            let mut m = reaction_map;
                                            move |_| {
                                                let mut map = m.read().clone();
                                                if map.contains_key(&feed_id) { map.remove(&feed_id); } else { map.insert(feed_id, "like".to_string()); }
                                                m.set(map);
                                            }
                                        },
                                        on_follow: {
                                            let mut fm = following_map;
                                            let ws = card.workspace_slug.clone();
                                            move |_| {
                                                let mut set = fm.read().clone();
                                                if set.contains(&ws) { set.remove(&ws); } else { set.insert(ws.clone()); }
                                                fm.set(set);
                                            }
                                        },
                                        on_share: {
                                            let mut store = mock_store;
                                            let card_for_share = card.clone();
                                            move |_| {
                                                let now = Utc::now();
                                                let mut s = store.read().clone();
                                                let share = FeedCardView {
                                                    feed_item_id: Uuid::new_v4(),
                                                    holding_id: card_for_share.holding_id,
                                                    workspace_slug: card_for_share.workspace_slug.clone(),
                                                    author_principal_id: viewer_principal,
                                                    author_display_name: "You (share)".to_string(),
                                                    visibility_scope_label: card_for_share.visibility_scope_label.clone(),
                                                    classification: card_for_share.classification.clone(),
                                                    created_at: now,
                                                    day_bucket: now.date_naive(),
                                                    day_label: "Today".to_string(),
                                                    workspace_chip: card_for_share.workspace_chip.clone(),
                                                    level_badge: card_for_share.level_badge.clone(),
                                                    hashtags: card_for_share.hashtags.clone(),
                                                    tombstone: false,
                                                    stale: false,
                                                };
                                                let scope = mock_holding_scope();
                                                s.push((share, scope));
                                                store.set(s);
                                            }
                                        },
                                        on_hashtag_click: {
                                            let mut sel = selected_hashtag;
                                            move |tag: String| {
                                                sel.set(Some(tag));
                                            }
                                        },
                                        on_context_click: {
                                            let mut sel = selected_context;
                                            move |ctx: String| {
                                                sel.set(Some(ctx));
                                            }
                                        },
                                        on_toggle_comments: {
                                            let mut expanded = expanded_comments;
                                            move |_| {
                                                let mut set = expanded.read().clone();
                                                if set.contains(&feed_id) { set.remove(&feed_id); } else { set.insert(feed_id); }
                                                expanded.set(set);
                                            }
                                        },
                                        on_toggle_show_all: {
                                            let mut show_all_sig = show_all_comments;
                                            move |_| {
                                                let mut set = show_all_sig.read().clone();
                                                if set.contains(&feed_id) { set.remove(&feed_id); } else { set.insert(feed_id); }
                                                show_all_sig.set(set);
                                            }
                                        },
                                        on_add_comment: {
                                            let mut cmap = comments_map;
                                            let mut expanded = expanded_comments;
                                            let viewer_principal_cp = viewer_principal;
                                            let viewer_level_sig = viewer_level;
                                            move |payload: NewCommentInput| {
                                                let text = payload.text.trim().to_string();
                                                let mut tags = payload.hashtags.clone();
                                                for t in extract_hashtags_from_text(&text) {
                                                    if !tags.contains(&t) { tags.push(t); }
                                                }
                                                if text.is_empty() && tags.is_empty() {
                                                    return;
                                                }
                                                if tags.len() > kernel_feed::MAX_HASHTAGS_PER_ITEM {
                                                    return;
                                                }
                                                for t in &tags {
                                                    if kernel_feed::validate_hashtag(t).is_err() { return; }
                                                }
                                                let record_refs = extract_record_refs(&text);
                                                let current_scope = viewer_level_sig.read().to_scope();
                                                let tombstone = record_refs.iter().any(|tok| !current_scope.contains(&record_scope_for_token(tok)));
                                                let comment = FlatComment {
                                                    comment_id: Uuid::new_v4(),
                                                    feed_item_id: feed_id,
                                                    author_principal_id: viewer_principal_cp,
                                                    author_display_name: "You".to_string(),
                                                    text: payload.text.clone(),
                                                    created_at: Utc::now(),
                                                    hashtags: tags,
                                                    record_refs,
                                                    tombstone,
                                                    author_scope: current_scope,
                                                };
                                                let mut map = cmap.read().clone();
                                                map.entry(feed_id).or_default().push(comment);
                                                cmap.set(map);
                                                let mut set = expanded.read().clone();
                                                set.insert(feed_id);
                                                expanded.set(set);
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { style: "display:flex;gap:8px;justify-content:center;padding:12px;",
                button { class: "feed-action-btn", onclick: move |_| {
                    let mut s = mock_store.read().clone();
                    let extra = generate_mock_items(Utc::now() - chrono::Duration::days(5));
                    s.extend(extra.into_iter().take(4));
                    mock_store.set(s);
                }, {copy(locale, "Load more (cursor pagination)", "โหลดเพิ่ม")} }
                span { style: "font-size:11px;color:#8aa0b3;align-self:center;", "cursor base64(millis:uuid) · {total_filtered} items · Home aggregates all 31 contexts via single shared projection in holding_write.feed_items" }
            }
        }
    }
}

#[component]
pub fn WorkspaceFeedPage(workspace: Workspace) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let session = use_context::<erpx_delivery_contract::SessionBootstrap>();
    let ws_slug = workspace.slug().to_string();
    let viewer_principal = session
        .actor_ref
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(|| Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap());
    let now = Utc::now();
    let mut mock_store = use_signal(|| generate_mock_items(now));
    let viewer_scope = mock_company_scope();
    let visible = filter_visible(&mock_store.read(), &viewer_scope);
    let ws_cards: Vec<FeedCardView> = visible
        .into_iter()
        .filter(|c| c.workspace_slug == ws_slug)
        .collect();
    let groups = group_by_day(ws_cards.clone());
    let mut composer_text = use_signal(String::new);
    let mut composer_tags: Signal<Vec<String>> = use_signal(Vec::new);
    let mut tag_input = use_signal(String::new);
    let mut err: Signal<Option<String>> = use_signal(|| None);
    let ws_slug_for_header = ws_slug.clone();
    let ws_slug_for_testid = ws_slug.clone();

    rsx! {
        section { class: "feed-page", "data-testid": "workspace-feed-{ws_slug_for_testid}",
            div { class: "feed-header",
                h1 { "{workspace_label(workspace, locale)} — " {copy(locale, "Feed", "ฟีด")} }
                p { {copy(locale, "Built-in tab at /", "แท็บในตัวที่ /")}{ws_slug_for_header.clone()}{"/feed · composer @ + # · day-grouped · level badge + workspace chip"} }
                div { style: "display:flex;gap:6px;flex-wrap:wrap;",
                    span { class: "feed-chip feed-chip--workspace", "{ws_slug_for_header.clone()}" }
                    span { class: "feed-chip feed-chip--level", "Company" }
                    span { class: "feed-chip", "audit: Holding isolation + break-glass intact" }
                }
            }
            div { class: "feed-composer",
                textarea {
                    placeholder: copy(locale, "Share an update in this workspace... @ to mention record, # for topic", "แชร์อัปเดตในพื้นที่งานนี้... @ เพื่อกล่าวถึงระเบียน # สำหรับหัวข้อ"),
                    value: "{composer_text}",
                    oninput: move |e: FormEvent| composer_text.set(e.value()),
                }
                div { class: "feed-composer__row",
                    input { r#type: "text", placeholder: "#hashtag", value: "{tag_input}", oninput: move |e: FormEvent| tag_input.set(e.value()) }
                    button { class: "feed-action-btn", onclick: move |_| {
                        let raw = tag_input.read().trim().trim_start_matches('#').to_ascii_lowercase();
                        if kernel_feed::validate_hashtag(&raw).is_ok() {
                            let mut v = composer_tags.read().clone();
                            if !v.contains(&raw) && v.len() < 20 { v.push(raw); composer_tags.set(v); tag_input.set(String::new()); err.set(None); }
                        } else { err.set(Some("Invalid hashtag".into())); }
                    }, "Add #" }
                    button { class: "feed-action-btn feed-action-btn--active", onclick: move |_| {
                        let text = composer_text.read().trim().to_string();
                        let tags = composer_tags.read().clone();
                        if text.is_empty() && tags.is_empty() { err.set(Some("Empty".into())); return; }
                        let now = Utc::now();
                        let ws_clone = ws_slug.clone();
                        let card = FeedCardView {
                            feed_item_id: Uuid::new_v4(),
                            holding_id: demo_holding_id().as_uuid(),
                            workspace_slug: ws_clone.clone(),
                            author_principal_id: viewer_principal,
                            author_display_name: "You".to_string(),
                            visibility_scope_label: "Company".to_string(),
                            classification: "internal".to_string(),
                            created_at: now,
                            day_bucket: now.date_naive(),
                            day_label: "Today".to_string(),
                            workspace_chip: ws_clone,
                            level_badge: "Company".to_string(),
                            hashtags: tags,
                            tombstone: false,
                            stale: false,
                        };
                        mock_store.write().push((card, mock_company_scope()));
                        composer_text.set(String::new());
                        composer_tags.set(Vec::new());
                    }, {copy(locale, "Post to", "โพสต์ไปที่")} " {ws_slug.clone()}" }
                }
                if let Some(e) = err.read().as_ref() { span { style: "color:#f87171;font-size:12px;", "{e}" } }
                if !composer_tags.read().is_empty() {
                    div { class: "feed-composer__chips",
                        for t in composer_tags.read().clone() { span { class: "feed-chip feed-chip--hashtag", "#{t}" } }
                    }
                }
            }
            if groups.is_empty() {
                div { class: "feed-empty",
                    p { {copy(locale, "No posts in this workspace yet. Be first to share @finance/journal-... #launch", "ยังไม่มีโพสต์ในพื้นที่งานนี้")} }
                }
            } else {
                for g in groups {
                    div { class: "feed-day-group", key: "{g.day_bucket}",
                        div { class: "feed-day-separator", "{g.label} · {g.day_bucket}" }
                        for c in g.items {
                            FeedCard { card: c, has_reacted: false, is_following: false, comments: Vec::new(), is_expanded: false, show_all: false, viewer_scope: mock_company_scope(), on_react: |_| {}, on_follow: |_| {}, on_share: |_| {}, on_hashtag_click: |_| {}, on_context_click: |_| {}, on_toggle_comments: |_| {}, on_toggle_show_all: |_| {}, on_add_comment: |_| {} }
                        }
                    }
                }
            }
            p { style: "font-size:11px;color:#8aa0b3;", {copy(locale, "This workspace feed is ListFeed(filter.contexts=[", "ฟีดพื้นที่งานนี้คือ ListFeed(filter.contexts=[")}{ws_slug.clone()}{"]) · Home unions all without per-workspace tables"} }
        }
    }
}

#[component]
fn FeedCard(
    card: FeedCardView,
    has_reacted: bool,
    is_following: bool,
    comments: Vec<FlatComment>,
    is_expanded: bool,
    show_all: bool,
    viewer_scope: AuthorityScope,
    on_react: EventHandler<()>,
    on_follow: EventHandler<()>,
    on_share: EventHandler<()>,
    on_hashtag_click: EventHandler<String>,
    on_context_click: EventHandler<String>,
    on_toggle_comments: EventHandler<()>,
    on_toggle_show_all: EventHandler<()>,
    on_add_comment: EventHandler<NewCommentInput>,
) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let initial = card
        .author_display_name
        .chars()
        .next()
        .unwrap_or('A')
        .to_ascii_uppercase();
    let body_text = if card.workspace_slug == "finance" {
        format!(
            "{} posted finance operation @finance/journal-2026-117",
            card.author_display_name
        )
    } else if card.workspace_slug == "project" {
        format!(
            "{} updated @project/proj-2026-001 — dependsOn @procurement/req-2026-014",
            card.author_display_name
        )
    } else {
        format!(
            "{} shared update in {} — see #{} ",
            card.author_display_name,
            card.workspace_slug,
            card.hashtags
                .first()
                .cloned()
                .unwrap_or_else(|| "launch".to_string())
        )
    };
    let card_class = if card.tombstone {
        "feed-card feed-card--tombstone"
    } else {
        "feed-card"
    };
    let date_str = card.created_at.format("%d %b %Y %H:%M UTC").to_string();
    let mut comment_text = use_signal(String::new);
    let mut comment_hashtags = use_signal(Vec::<String>::new);
    let mut comment_hashtag_input = use_signal(String::new);
    let mut comment_error: Signal<Option<String>> = use_signal(|| None);
    let mut mention_query: Signal<Option<String>> = use_signal(|| None);
    let mut hashtag_query: Signal<Option<String>> = use_signal(|| None);
    rsx! {
        article { class: "{card_class}", "data-feed-id": "{card.feed_item_id}", "data-workspace": "{card.workspace_slug}",
            div { class: "feed-card__head",
                div { class: "feed-card__avatar", aria_hidden: "true", "{initial}" }
                div { class: "feed-card__meta",
                    div { class: "feed-card__author", "{card.author_display_name}",
                        span { style: "margin-left:6px;font-weight:400;font-size:11px;color:#8aa0b3;", "{date_str}" }
                    }
                    div { class: "feed-card__sub",
                        span { class: "feed-chip feed-chip--workspace", onclick: {
                            let ws = card.workspace_slug.clone();
                            move |_| on_context_click.call(ws.clone())
                        }, "{card.workspace_chip}" }
                        span { class: "feed-chip feed-chip--level", "{card.level_badge}" }
                        span { style: "font-size:11px;color:#8aa0b3;", "{card.classification} · {card.visibility_scope_label}" }
                        if card.stale { span { class: "feed-chip", "Stale" } }
                    }
                }
            }
            if card.tombstone {
                div { class: "feed-card__body", style: "color:#fecaca;",
                    {copy(locale, "Tombstone — referenced record is missing or you lack access (read-time tombstone, authorized via ReferenceQueryService).", "ทูมบ์สโตน — ระเบียนที่อ้างอิงขาดหายหรือไม่มีสิทธิ์")}
                }
            } else {
                div { class: "feed-card__body", "{body_text}" }
            }
            div { class: "feed-card__chips",
                for tag in card.hashtags.clone() {
                    {
                        let t = tag.clone();
                        rsx! {
                            button {
                                class: "feed-chip feed-chip--hashtag",
                                onclick: move |_| on_hashtag_click.call(t.clone()),
                                "#{tag}"
                            }
                        }
                    }
                }
                span { class: "feed-chip", "@{card.workspace_slug}/record-..." }
                if card.tombstone {
                    span { class: "feed-chip feed-chip--tombstone", "@finance/journal-... (tombstone)" }
                }
            }
            div { class: "feed-card__actions", role: "group", aria_label: "Feed actions",
                button {
                    class: if has_reacted { "feed-action-btn feed-action-btn--active" } else { "feed-action-btn" },
                    onclick: move |_| on_react.call(()),
                    span { aria_hidden: "true", "♥" } {copy(locale, "Like", "ถูกใจ")}
                }
                button {
                    class: if is_expanded { "feed-action-btn feed-action-btn--active" } else { "feed-action-btn" },
                    onclick: move |_| on_toggle_comments.call(()),
                    {copy(locale, "Comment", "แสดงความคิดเห็น")}
                    if !comments.is_empty() {
                        span { style: "margin-left:4px;font-size:10px;background:#1e3a5f;padding:1px 6px;border-radius:999px;", "{comments.len()}" }
                    }
                }
                button { class: "feed-action-btn", onclick: move |_| on_share.call(()), {copy(locale, "Share", "แชร์")} }
                button {
                    class: if is_following { "feed-action-btn feed-action-btn--active" } else { "feed-action-btn" },
                    onclick: move |_| on_follow.call(()),
                    if is_following { {copy(locale, "Following", "ติดตามแล้ว")} } else { {copy(locale, "Follow", "ติดตาม")} }
                }
                button {
                    class: "feed-action-btn",
                    onclick: {
                        let mut ct = comment_text;
                        let is_exp = is_expanded;
                        let toggle = on_toggle_comments;
                        let token = card_mention_token(&card.workspace_slug).to_string();
                        move |_| {
                            if !is_exp {
                                toggle.call(());
                            }
                            let mut t = ct.read().clone();
                            if !t.contains(&token) {
                                if !t.is_empty() && !t.ends_with(' ') { t.push(' '); }
                                t.push_str(&format!("@{}", token));
                                t.push(' ');
                                ct.set(t);
                            }
                        }
                    },
                    "@" {copy(locale, "Mention", "กล่าวถึง")}
                }
            }
            if is_expanded {
                div { class: "feed-card__replies feed-comment-thread",
                    if comments.is_empty() {
                        div { class: "feed-empty", style: "padding:12px;",
                            p { style: "font-size:12px;", {copy(locale, "No comments yet. Be first!", "ยังไม่มีความคิดเห็น — เป็นคนแรก!")} }
                        }
                    } else {
                        {
                            let visible: Vec<FlatComment> = if show_all { comments.clone() } else { comments.iter().take(5).cloned().collect() };
                            let hidden = if comments.len() > 5 && !show_all { comments.len() - 5 } else { 0 };
                            rsx! {
                                div { class: "feed-comment-list",
                                    for cmt in visible {
                                        {
                                            let cmt_clone = cmt.clone();
                                            let author_initial = cmt_clone.author_display_name.chars().next().unwrap_or('Y').to_ascii_uppercase();
                                            let time_str = cmt_clone.created_at.format("%H:%M").to_string();
                                            let is_tombstone_comment = cmt_clone.tombstone;
                                            rsx! {
                                                div { class: if is_tombstone_comment { "feed-comment feed-comment--tombstone" } else { "feed-comment" }, key: "{cmt_clone.comment_id}",
                                                    div { class: "feed-comment__head",
                                                        div { class: "feed-comment__avatar", "{author_initial}" }
                                                        div { class: "feed-comment__meta",
                                                            span { class: "feed-comment__author", "{cmt_clone.author_display_name}" }
                                                            span { style: "font-size:11px;color:#8aa0b3;margin-left:6px;", "{time_str}" }
                                                            if is_tombstone_comment { span { class: "feed-chip feed-chip--tombstone", style: "margin-left:6px;", "tombstone" } }
                                                        }
                                                    }
                                                    div { class: "feed-comment__body", "{cmt_clone.text}" }
                                                    if !cmt_clone.hashtags.is_empty() || !cmt_clone.record_refs.is_empty() {
                                                        div { class: "feed-comment__chips",
                                                            for tag in cmt_clone.hashtags.clone() {
                                                                span { class: "feed-chip feed-chip--hashtag", "#{tag}" }
                                                            }
                                                            for rec in cmt_clone.record_refs.clone() {
                                                                {
                                                                    let is_tomb = !viewer_scope.contains(&record_scope_for_token(&rec));
                                                                    rsx! {
                                                                        span { class: if is_tomb { "feed-chip feed-chip--tombstone" } else { "feed-chip" }, "@{rec}" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if hidden > 0 {
                                    button {
                                        class: "feed-comment__more",
                                        onclick: move |_| on_toggle_show_all.call(()),
                                        {
                                            if locale == Locale::En { format!("Show {} more", hidden) } else { format!("แสดงอีก {} รายการ", hidden) }
                                        }
                                    }
                                } else if show_all && comments.len() > 5 {
                                    button {
                                        class: "feed-comment__more",
                                        onclick: move |_| on_toggle_show_all.call(()),
                                        {copy(locale, "Show less", "แสดงน้อยลง")}
                                    }
                                }
                            }
                        }
                    }
                    div { class: "feed-comment__composer",
                        textarea {
                            placeholder: copy(locale, "Write a comment... @ to mention, # for topic", "เขียนความคิดเห็น... @ เพื่อกล่าวถึง # สำหรับหัวข้อ"),
                            value: "{comment_text}",
                            oninput: move |e: FormEvent| {
                                let v = e.value();
                                comment_text.set(v.clone());
                                if let Some(at_pos) = v.rfind('@') {
                                    let after = &v[at_pos+1..];
                                    let q = after.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == ':' || c == ')' || c == '(').next().unwrap_or("").to_string();
                                    if !q.is_empty() { mention_query.set(Some(q)); } else { mention_query.set(None); }
                                } else { mention_query.set(None); }
                                if let Some(hash_pos) = v.rfind('#') {
                                    let after = &v[hash_pos+1..];
                                    let q = after.split_whitespace().next().unwrap_or("").to_string();
                                    if !q.is_empty() { hashtag_query.set(Some(q)); } else { hashtag_query.set(None); }
                                } else { hashtag_query.set(None); }
                            },
                        }
                        if let Some(q) = mention_query.read().as_ref().cloned() {
                            div { class: "feed-autocomplete",
                                div { class: "feed-autocomplete__list",
                                    for item in DEMO_RECORD_TOKENS.iter().filter(|s| s.to_ascii_lowercase().contains(&q.to_ascii_lowercase())) {
                                        {
                                            let item_str = item.to_string();
                                            let initial = item_str.chars().next().unwrap_or('F').to_ascii_uppercase().to_string();
                                            let sub = match item_str.as_str() {
                                                "finance/journal-2026-117" => "Finance · Holding · Journal 117",
                                                "sales/quote-2026-042" => "Sales · Company · Quote 042",
                                                "project/proj-2026-001" => "Projects · Holding · Proj 001",
                                                "procurement/req-2026-014" => "Procurement · Company · Req 014",
                                                "inventory/move-2026-883" => "Inventory · Branch · Move 883",
                                                _ => "Record",
                                            };
                                            let badge = item_str.split('/').next().unwrap_or("record").to_string();
                                            rsx! {
                                                div { class: "feed-autocomplete__item",
                                                    onclick: {
                                                        let item = item_str.clone();
                                                        let mut ct = comment_text;
                                                        let mut mq = mention_query;
                                                        move |_| {
                                                            let mut t = ct.read().clone();
                                                            if let Some(pos) = t.rfind('@') {
                                                                let before = t[..pos].to_string();
                                                                let after_q = &t[pos+1..];
                                                                let q_end = after_q.find(|c: char| c.is_whitespace()).unwrap_or(after_q.len());
                                                                let rest = if q_end < after_q.len() { &after_q[q_end..] } else { "" };
                                                                t = format!("{}@{}{}", before, item, rest);
                                                                if !t.ends_with(' ') { t.push(' '); }
                                                            } else {
                                                                if !t.is_empty() && !t.ends_with(' ') { t.push(' '); }
                                                                t.push_str(&format!("@{} ", item));
                                                            }
                                                            ct.set(t);
                                                            mq.set(None);
                                                        }
                                                    },
                                                    div { class: "feed-autocomplete__item-avatar", "{initial}" }
                                                    div { class: "feed-autocomplete__item-main",
                                                        div { class: "feed-autocomplete__item-name", "@{item_str}" }
                                                        div { class: "feed-autocomplete__item-sub", "{sub}" }
                                                    }
                                                    span { class: "feed-autocomplete__item-badge", "{badge}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(q) = hashtag_query.read().as_ref().cloned() {
                            div { class: "feed-autocomplete",
                                div { class: "feed-autocomplete__list",
                                    for tag in DEMO_HASHTAGS.iter().filter(|s| s.to_ascii_lowercase().contains(&q.to_ascii_lowercase())) {
                                        {
                                            let tag_str = tag.to_string();
                                            let sub = match tag_str.as_str() {
                                                "launch" => "Topic · 24 posts · trending",
                                                "q4_2026" => "Topic · 12 posts",
                                                "close" => "Topic · 8 posts",
                                                "audit" => "Topic · 18 posts",
                                                "retention" => "Topic · 6 posts",
                                                _ => "Hashtag",
                                            };
                                            rsx! {
                                                div { class: "feed-autocomplete__item",
                                                    onclick: {
                                                        let tag = tag_str.clone();
                                                        let mut ch = comment_hashtags;
                                                        let mut hq = hashtag_query;
                                                        move |_| {
                                                            let mut v = ch.read().clone();
                                                            if !v.contains(&tag) && v.len() < kernel_feed::MAX_HASHTAGS_PER_ITEM {
                                                                v.push(tag.clone());
                                                                ch.set(v);
                                                            }
                                                            hq.set(None);
                                                        }
                                                    },
                                                    div { class: "feed-autocomplete__item-avatar", style: "background: linear-gradient(135deg,#1c2533,#2a5a8a);", "#" }
                                                    div { class: "feed-autocomplete__item-main",
                                                        div { class: "feed-autocomplete__item-name", "#{tag_str}" }
                                                        div { class: "feed-autocomplete__item-sub", "{sub}" }
                                                    }
                                                    span { class: "feed-autocomplete__item-badge", "tag" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "feed-composer__row",
                            input {
                                r#type: "text",
                                placeholder: "#hashtag",
                                value: "{comment_hashtag_input}",
                                oninput: move |e: FormEvent| comment_hashtag_input.set(e.value()),
                            }
                            button {
                                class: "feed-action-btn",
                                onclick: {
                                    let mut ch = comment_hashtags;
                                    let mut inp = comment_hashtag_input;
                                    let mut err = comment_error;
                                    move |_| {
                                        let raw = inp.read().trim().trim_start_matches('#').to_ascii_lowercase();
                                        if raw.is_empty() { err.set(Some("Hashtag cannot be empty".into())); return; }
                                        if kernel_feed::validate_hashtag(&raw).is_ok() {
                                            let mut v = ch.read().clone();
                                            if v.contains(&raw) { err.set(Some("Duplicate hashtag".into())); return; }
                                            if v.len() >= kernel_feed::MAX_HASHTAGS_PER_ITEM { err.set(Some("Too many hashtags".into())); return; }
                                            v.push(raw.clone());
                                            ch.set(v);
                                            inp.set(String::new());
                                            err.set(None);
                                        } else {
                                            err.set(Some(format!("Invalid hashtag: {}", raw)));
                                        }
                                    }
                                },
                                "Add #"
                            }
                            if !comment_hashtags.read().is_empty() {
                                span { style: "font-size:11px;color:#8aa0b3;", "{comment_hashtags.read().len()}/20 tags" }
                            }
                        }
                        if !comment_hashtags.read().is_empty() {
                            div { class: "feed-composer__chips",
                                for tag in comment_hashtags.read().clone() {
                                    {
                                        let tag_clone = tag.clone();
                                        rsx! {
                                            span { class: "feed-chip feed-chip--hashtag",
                                                "#{tag}"
                                                button {
                                                    style: "margin-left:4px;background:transparent;border:none;color:#bfdbfe;cursor:pointer;",
                                                    onclick: {
                                                        let mut ch = comment_hashtags;
                                                        let tag_inner = tag_clone.clone();
                                                        move |_| {
                                                            let mut v = ch.read().clone();
                                                            v.retain(|x| x != &tag_inner);
                                                            ch.set(v);
                                                        }
                                                    },
                                                    "×"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "feed-composer__row",
                            button {
                                class: "feed-action-btn feed-action-btn--active",
                                onclick: {
                                    let mut ct = comment_text;
                                    let mut ch = comment_hashtags;
                                    let mut err = comment_error;
                                    let mut inp = comment_hashtag_input;
                                    let on_add = on_add_comment;
                                    move |_| {
                                        let text = ct.read().trim().to_string();
                                        let tags = ch.read().clone();
                                        let has_text = !text.is_empty();
                                        let has_tags = !tags.is_empty();
                                        if !has_text && !has_tags {
                                            err.set(Some("Comment cannot be empty".into()));
                                            return;
                                        }
                                        if tags.len() > kernel_feed::MAX_HASHTAGS_PER_ITEM {
                                            err.set(Some("Too many hashtags".into()));
                                            return;
                                        }
                                        for t in &tags {
                                            if kernel_feed::validate_hashtag(t).is_err() {
                                                err.set(Some(format!("Invalid hashtag: {}", t)));
                                                return;
                                            }
                                        }
                                        for t in extract_hashtags_from_text(&text) {
                                            if kernel_feed::validate_hashtag(&t).is_err() {
                                                err.set(Some(format!("Invalid hashtag in text: {}", t)));
                                                return;
                                            }
                                        }
                                        let payload = NewCommentInput { text: ct.read().clone(), hashtags: tags.clone() };
                                        on_add.call(payload);
                                        ct.set(String::new());
                                        ch.set(Vec::new());
                                        inp.set(String::new());
                                        err.set(None);
                                    }
                                },
                                {copy(locale, "Post", "โพสต์")}
                            }
                            button {
                                class: "feed-action-btn",
                                onclick: move |_| {
                                    comment_text.set(String::new());
                                    comment_hashtags.set(Vec::new());
                                    comment_hashtag_input.set(String::new());
                                    comment_error.set(None);
                                    mention_query.set(None);
                                    hashtag_query.set(None);
                                },
                                {copy(locale, "Cancel", "ยกเลิก")}
                            }
                            if let Some(e) = comment_error.read().as_ref() {
                                span { style: "color:#f87171;font-size:12px;", "{e}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn workspace_label(workspace: Workspace, locale: Locale) -> &'static str {
    match (workspace, locale) {
        (Workspace::Operations, Locale::En) => "Operations",
        (Workspace::Operations, Locale::Th) => "ปฏิบัติการ",
        (Workspace::Finance, Locale::En) => "Finance",
        (Workspace::Finance, Locale::Th) => "การเงิน",
        (Workspace::Sales, Locale::En) => "Sales",
        (Workspace::Sales, Locale::Th) => "การขาย",
        (Workspace::Procurement, Locale::En) => "Procurement",
        (Workspace::Procurement, Locale::Th) => "จัดซื้อ",
        (Workspace::Inventory, Locale::En) => "Inventory",
        (Workspace::Inventory, Locale::Th) => "สินค้าคงคลัง",
        (Workspace::Asset, Locale::En) => "Assets",
        (Workspace::Asset, Locale::Th) => "สินทรัพย์",
        (Workspace::Maintenance, Locale::En) => "Maintenance",
        (Workspace::Maintenance, Locale::Th) => "ซ่อมบำรุง",
        (Workspace::Quality, Locale::En) => "Quality",
        (Workspace::Quality, Locale::Th) => "คุณภาพ",
        (Workspace::Marketing, Locale::En) => "Marketing",
        (Workspace::Marketing, Locale::Th) => "การตลาด",
        (Workspace::People, Locale::En) => "People",
        (Workspace::People, Locale::Th) => "บุคลากร",
        (Workspace::Pos, Locale::En) => "Point of sale",
        (Workspace::Pos, Locale::Th) => "จุดขาย",
        (Workspace::Legal, Locale::En) => "Legal",
        (Workspace::Legal, Locale::Th) => "กฎหมาย",
        (Workspace::Manufacturing, Locale::En) => "Manufacturing",
        (Workspace::Manufacturing, Locale::Th) => "การผลิต",
        (Workspace::Planning, Locale::En) => "Planning",
        (Workspace::Planning, Locale::Th) => "การวางแผน",
        (Workspace::Project, Locale::En) => "Projects",
        (Workspace::Project, Locale::Th) => "โครงการ",
        (Workspace::Commerce, Locale::En) => "Commerce",
        (Workspace::Commerce, Locale::Th) => "อีคอมเมิร์ซ",
        (Workspace::Talent, Locale::En) => "Talent",
        (Workspace::Talent, Locale::Th) => "สรรหาบุคลากร",
        (Workspace::Learning, Locale::En) => "Learning",
        (Workspace::Learning, Locale::Th) => "การเรียนรู้",
        (Workspace::Benefits, Locale::En) => "Benefits",
        (Workspace::Benefits, Locale::Th) => "สวัสดิการ",
        (Workspace::FieldService, Locale::En) => "Field service",
        (Workspace::FieldService, Locale::Th) => "บริการภาคสนาม",
        (Workspace::Ai, Locale::En) => "AI",
        (Workspace::Ai, Locale::Th) => "เอไอ",
    }
}
