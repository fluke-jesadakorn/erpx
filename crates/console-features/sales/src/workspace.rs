use dioxus::prelude::*;
use erpx_ui_core::Locale;
use erpx_ui_kit::{
    Button, ButtonTone, EmptyState, PageHero, SectionCard, StatusBadge, StatusTone, TabItem, Tabs,
    kit::Input as KitInput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesProjectionState {
    Current,
    Lagging,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesWorkspaceRecord {
    pub key: String,
    pub reference: String,
    pub record_kind: String,
    pub status: String,
    pub amount: String,
    pub version: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerChoice {
    pub key: String,
    pub label: String,
    pub secondary_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogChoice {
    pub key: String,
    pub code: String,
    pub label: String,
    pub price: String,
    pub tax_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalesSuggestionState {
    None,
    Running,
    Ready {
        provider_label: String,
        provenance: String,
        changes: Vec<String>,
    },
    Stale,
    Failed {
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesWorkspaceModel {
    pub scope_label: String,
    pub records: Vec<SalesWorkspaceRecord>,
    pub customers: Vec<CustomerChoice>,
    pub catalog: Vec<CatalogChoice>,
    pub projection: SalesProjectionState,
    pub suggestion: SalesSuggestionState,
    pub selected_customer_key: Option<String>,
    pub selected_catalog_key: Option<String>,
    pub custom_line_reason: String,
    pub language: String,
    pub quote_subtotal: String,
    pub quote_tax: String,
    pub quote_total: String,
    pub approval_status: String,
}

#[component]
pub fn SalesFinanceWorkspace(
    model: SalesWorkspaceModel,
    locale: Locale,
    #[props(default)] busy: bool,
    #[props(default)] initial_tab: Option<String>,
    #[props(default)] on_refresh: Option<EventHandler<()>>,
    #[props(default)] on_new_quote: Option<EventHandler<()>>,
    #[props(default)] on_select_customer: Option<EventHandler<String>>,
    #[props(default)] on_select_catalog: Option<EventHandler<String>>,
    #[props(default)] on_custom_reason: Option<EventHandler<String>>,
    #[props(default)] on_language: Option<EventHandler<String>>,
    #[props(default)] on_suggest: Option<EventHandler<()>>,
    #[props(default)] on_apply_suggestion: Option<EventHandler<()>>,
    #[props(default)] on_submit_approval: Option<EventHandler<()>>,
) -> Element {
    let initial_tab = initial_tab.unwrap_or_else(|| "dashboard".to_owned());
    let mut active = use_signal(|| initial_tab.clone());
    let projection_tone = match model.projection {
        SalesProjectionState::Current => StatusTone::Success,
        SalesProjectionState::Lagging => StatusTone::Warning,
        SalesProjectionState::Failed => StatusTone::Danger,
    };
    let projection_label = match model.projection {
        SalesProjectionState::Current => copy(locale, "Projection current", "ข้อมูลเป็นปัจจุบัน"),
        SalesProjectionState::Lagging => copy(
            locale,
            "Projection lagging · retry available",
            "ข้อมูลล่าช้า · ลองใหม่ได้",
        ),
        SalesProjectionState::Failed => copy(
            locale,
            "Projection failed · retry safely",
            "โหลดข้อมูลไม่สำเร็จ · ลองใหม่ได้อย่างปลอดภัย",
        ),
    };
    let tabs = vec![
        TabItem {
            id: "dashboard".to_owned(),
            label: copy(locale, "Dashboard", "ภาพรวม").to_owned(),
        },
        TabItem {
            id: "quotes".to_owned(),
            label: copy(locale, "Quotes", "ใบเสนอราคา").to_owned(),
        },
        TabItem {
            id: "orders".to_owned(),
            label: copy(locale, "Orders", "คำสั่งขาย").to_owned(),
        },
        TabItem {
            id: "revenue".to_owned(),
            label: copy(locale, "Revenue & fulfillment", "รายได้และการส่งมอบ").to_owned(),
        },
    ];
    let active_snapshot = active.read().clone();
    rsx! {
        section {
            class: "erpx-sales-finance-workspace",
            "data-workspace": "sales-to-finance",
            "data-fixture-provider-label": "Fixture AI",
            PageHero {
                eyebrow: copy(locale, "SALES · HUMAN-CONTROLLED", "การขาย · มนุษย์เป็นผู้ควบคุม").to_owned(),
                title: copy(locale, "Quote-to-revenue workspace", "พื้นที่ทำงานจากใบเสนอราคาถึงรายได้").to_owned(),
                description: copy(locale, "Build customer-ready quotes, route Sales and Finance approval, then follow fulfillment and billing without technical identifiers.", "สร้างใบเสนอราคาสำหรับลูกค้า ส่งอนุมัติฝ่ายขายและการเงิน และติดตามการส่งมอบกับการวางบิลโดยไม่ต้องกรอกรหัสเทคนิค").to_owned(),
                meta: Some(model.scope_label.clone()),
                actions: rsx! {
                    Button { label: copy(locale, "Refresh", "รีเฟรช").to_owned(), tone: ButtonTone::Secondary, disabled: on_refresh.is_none(), on_press: on_refresh }
                    Button { label: copy(locale, "New quote", "สร้างใบเสนอราคา").to_owned(), tone: ButtonTone::Primary, disabled: on_new_quote.is_none(), on_press: on_new_quote }
                },
            }
            div { class: "erpx-workspace-status-strip", role: "status",
                StatusBadge { label: projection_label.to_owned(), tone: projection_tone }
                StatusBadge { label: "Fixture AI".to_owned(), tone: StatusTone::Info }
                StatusBadge { label: copy(locale, "Suggestion only", "เป็นเพียงคำแนะนำ").to_owned(), tone: StatusTone::Neutral }
                StatusBadge { label: copy(locale, "Not posted", "ยังไม่ลงบัญชี").to_owned(), tone: StatusTone::Warning }
            }
            Tabs { items: tabs, active_id: active_snapshot.clone(), on_change: move |next| active.set(next) }
            if active_snapshot == "quotes" {
                {quote_builder(
                    model,
                    locale,
                    busy,
                    on_select_customer,
                    on_select_catalog,
                    on_custom_reason,
                    on_language,
                    on_suggest,
                    on_apply_suggestion,
                    on_submit_approval,
                )}
            } else {
                {record_workspace(model, locale, &active_snapshot)}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn quote_builder(
    model: SalesWorkspaceModel,
    locale: Locale,
    busy: bool,
    on_select_customer: Option<EventHandler<String>>,
    on_select_catalog: Option<EventHandler<String>>,
    on_custom_reason: Option<EventHandler<String>>,
    on_language: Option<EventHandler<String>>,
    on_suggest: Option<EventHandler<()>>,
    on_apply_suggestion: Option<EventHandler<()>>,
    on_submit_approval: Option<EventHandler<()>>,
) -> Element {
    let suggestion_ready = matches!(model.suggestion, SalesSuggestionState::Ready { .. });
    rsx! {
        div { class: "erpx-quote-builder-grid",
            SectionCard {
                title: copy(locale, "Quote builder", "ตัวสร้างใบเสนอราคา").to_owned(),
                eyebrow: Some(copy(locale, "DRAFT · EDITABLE", "ฉบับร่าง · แก้ไขได้").to_owned()),
                description: Some(copy(locale, "Choose business records by name. Custom lines require a reason, tax classification, and Finance approval.", "เลือกข้อมูลธุรกิจจากชื่อ รายการกำหนดเองต้องมีเหตุผล การจัดประเภทภาษี และการอนุมัติจากฝ่ายการเงิน").to_owned()),
                div { class: "erpx-choice-section",
                    h4 { {copy(locale, "Customer", "ลูกค้า")} }
                    if model.customers.is_empty() {
                        EmptyState { title: copy(locale, "No authorized customers", "ไม่มีลูกค้าที่ได้รับอนุญาต").to_owned(), message: copy(locale, "Refresh after the customer projection is ready.", "รีเฟรชเมื่อข้อมูลลูกค้าพร้อม").to_owned() }
                    }
                    for customer in model.customers {
                        {
                            let key = customer.key.clone();
                            let selected = model.selected_customer_key.as_deref() == Some(key.as_str());
                            let handler = on_select_customer;
                            rsx! { Button { label: format!("{} · {}", customer.label, customer.secondary_label), tone: if selected { ButtonTone::Primary } else { ButtonTone::Secondary }, disabled: handler.is_none(), on_press: move |_| if let Some(handler) = handler.as_ref() { handler.call(key.clone()) } } }
                        }
                    }
                }
                div { class: "erpx-choice-section",
                    h4 { {copy(locale, "Catalog", "แค็ตตาล็อก")} }
                    if model.catalog.is_empty() {
                        EmptyState { title: copy(locale, "Catalog not ready", "แค็ตตาล็อกยังไม่พร้อม").to_owned(), message: copy(locale, "You can continue manually; AI availability never blocks quote work.", "ทำงานต่อด้วยตนเองได้ ความพร้อมของ AI ไม่ขัดขวางการสร้างใบเสนอราคา").to_owned() }
                    }
                    for item in model.catalog {
                        {
                            let key = item.key.clone();
                            let selected = model.selected_catalog_key.as_deref() == Some(key.as_str());
                            let handler = on_select_catalog;
                            rsx! { Button { label: format!("{} · {} · {} · {}", item.code, item.label, item.price, item.tax_label), tone: if selected { ButtonTone::Primary } else { ButtonTone::Secondary }, disabled: handler.is_none(), on_press: move |_| if let Some(handler) = handler.as_ref() { handler.call(key.clone()) } } }
                        }
                    }
                }
                div { class: "erpx-custom-line-reason",
                    label { r#for: "quote-custom-line-reason", {copy(locale, "Custom line reason", "เหตุผลของรายการกำหนดเอง")} }
                    KitInput { id: "quote-custom-line-reason", value: model.custom_line_reason, placeholder: copy(locale, "Required before a custom line can be reviewed", "จำเป็นก่อนส่งรายการกำหนดเองให้ตรวจสอบ"), disabled: on_custom_reason.is_none(), oninput: move |event: FormEvent| if let Some(handler) = on_custom_reason.as_ref() { handler.call(event.value()) } }
                }
                div { class: "erpx-language-choice", role: "group", aria_label: copy(locale, "Quote document language", "ภาษาของใบเสนอราคา"),
                    Button { label: "ไทย".to_owned(), tone: if model.language == "th" { ButtonTone::Primary } else { ButtonTone::Secondary }, disabled: on_language.is_none(), on_press: { let handler = on_language; move |_| if let Some(handler) = handler.as_ref() { handler.call("th".to_owned()) } } }
                    Button { label: "English".to_owned(), tone: if model.language == "en" { ButtonTone::Primary } else { ButtonTone::Secondary }, disabled: on_language.is_none(), on_press: move |_| if let Some(handler) = on_language.as_ref() { handler.call("en".to_owned()) } }
                }
                dl { class: "erpx-quote-totals",
                    dt { {copy(locale, "Subtotal", "ยอดก่อนภาษี")} } dd { {model.quote_subtotal} }
                    dt { {copy(locale, "Tax", "ภาษี")} } dd { {model.quote_tax} }
                    dt { {copy(locale, "Total", "ยอดรวม")} } dd { {model.quote_total} }
                }
            }
            SectionCard {
                title: copy(locale, "Suggestion and approval", "คำแนะนำและการอนุมัติ").to_owned(),
                eyebrow: Some("Fixture AI".to_owned()),
                description: Some(copy(locale, "Suggest is explicit. Apply copies the complete diff into this editable draft; it never approves or posts.", "ผู้ใช้ต้องกดขอคำแนะนำเอง Apply จะคัดลอก diff ทั้งชุดลงฉบับร่างที่ยังแก้ไขได้ และไม่ถือเป็นการอนุมัติหรือลงบัญชี").to_owned()),
                Button { label: copy(locale, "Suggest", "ขอคำแนะนำ").to_owned(), tone: ButtonTone::Secondary, busy: matches!(model.suggestion, SalesSuggestionState::Running), disabled: on_suggest.is_none(), on_press: on_suggest }
                {suggestion_panel(&model.suggestion, locale)}
                Button { label: copy(locale, "Apply suggestion", "ใช้คำแนะนำ").to_owned(), tone: ButtonTone::Primary, disabled: !suggestion_ready || on_apply_suggestion.is_none(), on_press: on_apply_suggestion }
                div { class: "erpx-approval-gates",
                    StatusBadge { label: model.approval_status, tone: StatusTone::Warning }
                    p { {copy(locale, "Parallel gates: Sales commercial review + Finance tax/accounting/custom-line review. Maker-checker rules apply.", "ประตูอนุมัติแบบขนาน: ฝ่ายขายตรวจเงื่อนไขการค้า และฝ่ายการเงินตรวจภาษี/บัญชี/รายการกำหนดเอง โดยบังคับ maker-checker")} }
                }
                Button { label: copy(locale, "Submit Sales + Finance approval", "ส่งอนุมัติฝ่ายขาย + การเงิน").to_owned(), tone: ButtonTone::Primary, busy, disabled: on_submit_approval.is_none(), on_press: on_submit_approval }
            }
        }
    }
}

fn suggestion_panel(state: &SalesSuggestionState, locale: Locale) -> Element {
    match state {
        SalesSuggestionState::None => {
            rsx! { p { role: "status", {copy(locale, "No suggestion requested.", "ยังไม่ได้ขอคำแนะนำ")} } }
        }
        SalesSuggestionState::Running => {
            rsx! { p { role: "status", aria_live: "polite", {copy(locale, "Fixture AI is preparing a deterministic suggestion…", "Fixture AI กำลังเตรียมคำแนะนำแบบกำหนดผลลัพธ์ได้…")} } }
        }
        SalesSuggestionState::Ready {
            provider_label,
            provenance,
            changes,
        } => rsx! {
            article { class: "erpx-suggestion-diff", "data-state": "ready",
                header { StatusBadge { label: provider_label.clone(), tone: StatusTone::Info } StatusBadge { label: copy(locale, "Suggestion", "คำแนะนำ").to_owned(), tone: StatusTone::Neutral } }
                p { {provenance.clone()} }
                ul { for change in changes { li { {change.clone()} } } }
            }
        },
        SalesSuggestionState::Stale => {
            rsx! { div { role: "alert", "data-state": "stale", StatusBadge { label: copy(locale, "Stale suggestion", "คำแนะนำล้าสมัย").to_owned(), tone: StatusTone::Danger } p { {copy(locale, "The Quote changed. Request a new suggestion; this one cannot be applied.", "ใบเสนอราคาถูกแก้ไข ต้องขอคำแนะนำใหม่และใช้คำแนะนำเดิมไม่ได้")} } } }
        }
        SalesSuggestionState::Failed { retryable } => {
            rsx! { div { role: "status", "data-state": "failed", StatusBadge { label: copy(locale, "AI unavailable", "AI ไม่พร้อมใช้งาน").to_owned(), tone: StatusTone::Warning } p { {if *retryable { copy(locale, "Retry Suggest, or continue manually.", "ลองขอคำแนะนำใหม่ หรือทำต่อด้วยตนเอง") } else { copy(locale, "Continue manually; quote work remains available.", "ทำต่อด้วยตนเองได้ การสร้างใบเสนอราคายังใช้งานได้") }} } } }
        }
    }
}

fn record_workspace(model: SalesWorkspaceModel, locale: Locale, active: &str) -> Element {
    let filtered = model
        .records
        .into_iter()
        .filter(|record| match active {
            "orders" => record.record_kind == "order",
            "revenue" => matches!(record.record_kind.as_str(), "arrangement" | "fulfillment"),
            _ => true,
        })
        .collect::<Vec<_>>();
    rsx! {
        SectionCard {
            title: match active { "orders" => copy(locale, "Sales orders", "คำสั่งขาย"), "revenue" => copy(locale, "Revenue arrangements and fulfillment", "ข้อตกลงรายได้และการส่งมอบ"), _ => copy(locale, "Sales pipeline", "ขั้นตอนการขาย") }.to_owned(),
            description: Some(copy(locale, "Business references are shown here; technical identities stay behind the authorized gateway.", "แสดงรหัสอ้างอิงทางธุรกิจ ส่วนรหัสเทคนิคอยู่หลัง gateway ที่ตรวจสอบสิทธิ์").to_owned()),
            if filtered.is_empty() {
                EmptyState { title: copy(locale, "No records in this view", "ไม่มีรายการในมุมมองนี้").to_owned(), message: copy(locale, "Create a Quote or refresh when the projection catches up.", "สร้างใบเสนอราคา หรือรีเฟรชเมื่อข้อมูลตามทัน").to_owned() }
            }
            div { class: "erpx-business-record-list",
                for record in filtered {
                    article { class: "erpx-business-record-card", "data-record-key": record.key,
                        header { h4 { {record.reference} } StatusBadge { label: record.status, tone: StatusTone::Info } }
                        p { {record.record_kind} } strong { {record.amount} }
                        small { {format!("v{} · {}", record.version, record.updated_at)} }
                    }
                }
            }
        }
    }
}

const fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_never_asks_users_for_technical_identifiers() {
        let source = include_str!("workspace.rs");
        assert!(!source.contains(concat!("U", "UID")));
        assert!(source.contains("Fixture AI"));
        assert!(source.contains("Apply suggestion"));
        assert!(source.contains("Not posted"));
        assert!(source.contains("data-state\": \"stale"));
    }
}
