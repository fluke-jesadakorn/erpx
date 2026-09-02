//! Finance workspace shell with dense ERP UX and cross-department @ mentions.

use dioxus::prelude::*;

use erpx_ui_ai_collaboration::{TypedToken, parse_pasted_typed_token};
use erpx_ui_core::Locale;
use erpx_ui_finance::{
    BankingImportJobRow, FinanceKpiTone, FinanceMentionKind, FinanceRoute, FinanceSurface,
    FinanceWorkspaceModel, PeriodCloseRow,
};
use erpx_ui_kit::{Button, ButtonTone, StatusBadge, StatusTone};

fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinanceShellStatus {
    Fresh,
    Stale { lag_millis: Option<u64> },
    Offline,
    Denied,
}

impl FinanceShellStatus {
    pub fn banner(&self, locale: Locale) -> Option<String> {
        match self {
            Self::Fresh => Some(copy(locale, "Projection: fresh", "โพรเจกชัน: สด").to_owned()),
            Self::Stale { lag_millis } => Some(format!(
                "{} {}",
                copy(locale, "Stale", "ล้าสมัย"),
                lag_millis.map(|v| format!("{v} ms")).unwrap_or_default()
            )),
            Self::Offline => {
                Some(copy(locale, "Offline — showing cached", "ออฟไลน์ — แสดงแคช").to_owned())
            }
            Self::Denied => Some(copy(locale, "Access denied", "ถูกปฏิเสธการเข้าถึง").to_owned()),
        }
    }
}

#[component]
pub fn FinanceWorkspaceShell(
    model: FinanceWorkspaceModel,
    #[props(default)] on_navigate: Option<EventHandler<FinanceRoute>>,
    #[props(default)] on_create_journal: Option<EventHandler<()>>,
    #[props(default)] on_retry: Option<EventHandler<()>>,
    #[props(default)] status: Option<FinanceShellStatus>,
    #[props(default)] can_create: bool,
    #[props(default)] can_post: bool,
) -> Element {
    let _ = on_navigate;
    let locale = model.locale;
    let banner = status
        .as_ref()
        .and_then(|s| s.banner(locale))
        .or(model.freshness_label.clone());
    let is_overview = model.active_surface == FinanceSurface::Overview;
    rsx! {
        section { class: "finance-workspace", "data-surface": format!("{:?}", model.active_surface),
            header { class: "finance-workspace__header",
                div { class: "finance-workspace__title",
                    h1 { {copy(locale, "Finance", "การเงิน")} }
                    if let Some(scope) = model.scope_label.clone() {
                        span { class: "finance-workspace__scope", "{scope}" }
                    }
                }
                if let Some(label) = banner.clone() {
                    span { class: "finance-workspace__freshness", role: "status", "{label}" }
                }
            }
            div {
                class: if is_overview { "finance-workspace__layout finance-workspace__layout--with-rail" } else { "finance-workspace__layout" },
                div { class: "finance-workspace__main",
                    if is_overview {
                        FinanceKpiStrip { kpis: model.kpis.clone(), locale }
                        div { class: "finance-workspace__actions",
                            Button { label: copy(locale, "New journal", "สมุดรายวันใหม่").to_owned(), tone: ButtonTone::Primary, on_press: on_create_journal, disabled: !can_create }
                            Button { label: copy(locale, "Import bank CSV", "นำเข้าบัญชีธนาคาร").to_owned(), tone: ButtonTone::Secondary, disabled: !can_create }
                            Button { label: copy(locale, "Suggest review (AI)", "แนะนำตรวจสอบ (AI)").to_owned(), tone: ButtonTone::Secondary, disabled: !can_post }
                        }
                    }
                    FinanceSurfaceContent { surface: model.active_surface, locale, on_retry }
                    if is_overview {
                        FinanceMentionComposer { locale }
                        FinanceCrossDeptPalette { locale }
                    }
                }
                if is_overview {
                    aside { class: "finance-workspace__rail", aria_label: copy(locale, "Activity and references", "กิจกรรมและการอ้างอิง"),
                        FinanceActivityRail { locale }
                    }
                }
            }
        }
    }
}

#[component]
fn FinanceKpiStrip(kpis: Vec<erpx_ui_finance::FinanceKpi>, locale: Locale) -> Element {
    rsx! {
        section { class: "finance-kpi-strip", role: "region", aria_label: copy(locale, "Key indicators", "ตัวชี้วัดหลัก"),
            for kpi in kpis {
                article { class: format!("finance-kpi finance-kpi--{}", match kpi.tone { FinanceKpiTone::Positive => "positive", FinanceKpiTone::Negative => "negative", FinanceKpiTone::Warning => "warning", FinanceKpiTone::Neutral => "neutral" }),
                    h3 { {kpi.label.text(locale)} }
                    p { class: "finance-kpi__value", "{kpi.value}" }
                    if let Some(delta) = kpi.delta {
                        span { class: "finance-kpi__delta", "{delta}" }
                    }
                }
            }
        }
    }
}

#[component]
fn FinanceSurfaceContent(
    surface: FinanceSurface,
    locale: Locale,
    on_retry: Option<EventHandler<()>>,
) -> Element {
    let (title, desc) = match surface {
        FinanceSurface::Overview => (
            copy(locale, "Overview", "ภาพรวม"),
            copy(
                locale,
                "Trial balance, open payables/receivables, cash, and period health at a glance.",
                "ภาพรวมงบทดลอง เจ้าหนี้/ลูกหนี้ค้างจ่าย เงินสด และสถานะงวด",
            ),
        ),
        FinanceSurface::Chart => (
            copy(locale, "Chart of accounts", "ผังบัญชี"),
            copy(
                locale,
                "Hierarchical accounts by category and normal balance.",
                "บัญชีแบบลำดับชั้นตามหมวดและยอดดุลปกติ",
            ),
        ),
        FinanceSurface::Journals => (
            copy(locale, "Journals", "สมุดรายวัน"),
            copy(
                locale,
                "Draft → posted lifecycle with debit/credit totals.",
                "วงจรชีวิตแบบร่าง → ลงบัญชี พร้อมยอดเดบิต/เครดิต",
            ),
        ),
        FinanceSurface::Subledgers => (
            copy(locale, "Subledgers", "บัญชีย่อย"),
            copy(
                locale,
                "Open / partially settled / settled items.",
                "รายการค้าง / ชำระบางส่วน / ชำระแล้ว",
            ),
        ),
        FinanceSurface::AccountsPayable => (
            copy(locale, "Accounts payable", "เจ้าหนี้การค้า"),
            copy(
                locale,
                "Vendor invoices → 3-way match → payments.",
                "ใบแจ้งหนี้ผู้ขาย → ตรวจสามทาง → ชำระเงิน",
            ),
        ),
        FinanceSurface::AccountsReceivable => (
            copy(locale, "Accounts receivable", "ลูกหนี้การค้า"),
            copy(
                locale,
                "Customer invoices and revenue schedules.",
                "ใบแจ้งหนี้ลูกหนี้และตารางรับรู้รายได้",
            ),
        ),
        FinanceSurface::Cash => (
            copy(locale, "Cash", "เงินสด"),
            copy(
                locale,
                "Cash positions aggregated from AP/AR and bank.",
                "สถานะเงินสดรวมจาก AP/AR และธนาคาร",
            ),
        ),
        FinanceSurface::Banking => (
            copy(locale, "Banking", "ธนาคาร"),
            copy(
                locale,
                "Bank transactions + import job timeline with retry.",
                "ธุรกรรมธนาคาร + ไทม์ไลน์งานนำเข้าพร้อมลองใหม่",
            ),
        ),
        FinanceSurface::Close => (
            copy(locale, "Period close", "ปิดงวดบัญชี"),
            copy(
                locale,
                "Trial balance, statutory tax, and open/close/reopen.",
                "งบทดลอง ภาษีตามกฎหมาย และเปิด/ปิด/เปิดใหม่",
            ),
        ),
        FinanceSurface::Budgets => (
            copy(locale, "Budgets", "งบประมาณ"),
            copy(
                locale,
                "Plan vs actuals with variance and CSV export.",
                "แผน vs จริง พร้อมผลต่างและส่งออก CSV",
            ),
        ),
    };
    rsx! {
        section { class: "finance-surface", "data-surface": format!("{surface:?}"),
            h2 { "{title}" }
            p { class: "finance-surface__desc", "{desc}" }
            match surface {
                FinanceSurface::Overview => rsx! { OverviewPlaceholder { locale } },
                FinanceSurface::Chart => rsx! { ChartTreePlaceholder { locale } },
                FinanceSurface::Journals => rsx! { JournalsPlaceholder { locale } },
                FinanceSurface::Subledgers => rsx! { SubledgersPlaceholder { locale } },
                FinanceSurface::AccountsPayable => rsx! { PayablesPlaceholder { locale } },
                FinanceSurface::AccountsReceivable => rsx! { ReceivablesPlaceholder { locale } },
                FinanceSurface::Cash => rsx! { CashPlaceholder { locale } },
                FinanceSurface::Banking => rsx! { BankingJobsPlaceholder { locale, on_retry } },
                FinanceSurface::Close => rsx! { PeriodClosePlaceholder { locale } },
                FinanceSurface::Budgets => rsx! { BudgetVariancePlaceholder { locale } },
            }
        }
    }
}

#[component]
fn OverviewPlaceholder(locale: Locale) -> Element {
    let mut toast = use_signal(|| Option::<String>::None);
    rsx! {
        div { class: "finance-overview-grid",
            section { class: "finance-overview__panel",
                h3 { {copy(locale, "Period health", "สุขภาพงวด")} }
                p { class: "finance-overview__metric", "2026-08 · " {copy(locale, "open", "เปิด")} " · Δ THB 0.00" }
                p { class: "finance-overview__hint", {copy(locale, "Trial balance is balanced. 3 periods ready to close.", "งบทดลองสมดุล มี 3 งวดพร้อมปิด")} }
                div { class: "finance-overview__actions",
                    Button { label: copy(locale, "Go to Close", "ไปปิดงวด").to_owned(), tone: ButtonTone::Primary }
                    StatusBadge { label: copy(locale, "Fresh", "สด").to_owned(), tone: StatusTone::Success }
                }
                if let Some(msg) = toast.read().clone() {
                    p { class: "finance-overview__toast", role: "status", "{msg}" }
                }
            }
            section { class: "finance-overview__panel",
                h3 { {copy(locale, "Unposted queue", "รายการยังไม่ลงบัญชี")} }
                p { class: "finance-muted", style: "font-size:12px;margin-bottom:8px;", {copy(locale, "Via GetTrialBalance + ListJournals(limit 5) · JournalStatus Draft/Pending", "ผ่านงบทดลอง + สมุดรายวัน 5 รายการ")} }
                ul { class: "finance-overview__list",
                    li { span { class: "finance-mono", "J-2026-0881" } span { "THB 12,400" } StatusBadge { label: copy(locale, "draft", "แบบร่าง").to_owned(), tone: StatusTone::Warning } Button { label: copy(locale, "Post", "ลงบัญชี").to_owned(), tone: ButtonTone::Secondary } }
                    li { span { class: "finance-mono", "J-2026-0882" } span { "THB 8,200" } StatusBadge { label: copy(locale, "draft", "แบบร่าง").to_owned(), tone: StatusTone::Warning } Button { label: copy(locale, "Post", "ลงบัญชี").to_owned(), tone: ButtonTone::Secondary } }
                    li { span { class: "finance-mono", "J-2026-0883" } span { "THB 3,900" } StatusBadge { label: copy(locale, "pending", "รอดำเนินการ").to_owned(), tone: StatusTone::Neutral } Button { label: copy(locale, "View", "ดู").to_owned(), tone: ButtonTone::Secondary } }
                }
                p { class: "finance-table__footnote", {copy(locale, "Posting requires HumanDecisionProof · Closed period → InvalidPeriodTransition", "การลงบัญชีต้องมีหลักฐานอนุมัติ · งวดปิดลงไม่ได้")} }
            }
            section { class: "finance-overview__panel",
                h3 { {copy(locale, "Cash & exposure", "เงินสดและความเสี่ยง")} }
                div { class: "finance-overview__stats",
                    div { strong { "AP open" } span { class: "finance-mono", "THB 48,200" } span { class: "finance-muted", " 3 invoices" } }
                    div { strong { "AR open" } span { class: "finance-mono", "THB 62,400" } span { class: "finance-muted", " 5 invoices" } }
                    div { strong { "Bank" } span { class: "finance-mono", "THB 1,722,500" } StatusBadge { label: "CSV-only".to_owned(), tone: StatusTone::Neutral } }
                }
                p { class: "finance-muted", style: "font-size:11px;", {copy(locale, "From finance-bank-transactions + finance_subledger 5-bucket aging (Current/1-30/31-60/61-90/Over90)", "จากธุรกรรมธนาคาร + อายุหนี้ 5 ช่วง")} }
            }
            section { class: "finance-overview__panel finance-overview__panel--aging",
                h3 { {copy(locale, "Aging sparkline (AR)", "กราฟอายุหนี้ AR")} }
                div { class: "finance-aging__bars",
                    for (label, value, tone) in [("Current", "8,400", StatusTone::Success), ("1-30", "12,000", StatusTone::Neutral), ("31-60", "22,000", StatusTone::Warning), ("61-90", "9,000", StatusTone::Warning), ("Over90", "11,000", StatusTone::Danger)] {
                        div { class: "finance-aging__bucket",
                            span { class: "finance-aging__label", "{label}" }
                            span { class: "finance-mono", "{value}" }
                            StatusBadge { label: label.to_string(), tone: tone }
                        }
                    }
                }
                Button { label: copy(locale, "View BS/PL", "ดูงบดุล/กำไรขาดทุน").to_owned(), tone: ButtonTone::Secondary, on_press: move |_| toast.set(Some("BS assets 50,000 · liab -30,000 · net -60,000 · balanced false (demo)".into())) }
            }
            section { class: "finance-overview__panel finance-overview__panel--bank",
                h3 { {copy(locale, "Bank CSV pipeline", "ท่อธนาคาร CSV")} }
                p { style: "overflow-wrap:anywhere;line-height:1.5;", {copy(locale, "CSV → staging → SaveBankMatchProposal (system) → SaveBankMatchDecision(human) → SaveBankReconciliation(version)", "CSV → พัก → เสนอจับคู่ (ระบบ) → อนุมัติ (คน) → กระทบยอด (เวอร์ชัน)")} }
                div { class: "finance-bank-pipeline__badges",
                    StatusBadge { label: "FINANCE_BANK_PROVIDER_ENABLED=false".to_owned(), tone: StatusTone::Danger }
                    StatusBadge { label: "erpx_control OFF".to_owned(), tone: StatusTone::Neutral }
                    StatusBadge { label: "Idempotent CommandId".to_owned(), tone: StatusTone::Neutral }
                }
                p { class: "finance-muted", style: "font-size:11px;margin-top:6px;", {copy(locale, "Cutover gate: TB(legacy)==TB(erpx) && aging==aging @ closing date", "เกณฑ์โอน: งบทดลองเก่า==ใหม่ && อายุหนี้เก่า==ใหม่")} }
            }
        }
    }
}

#[component]
fn JournalsPlaceholder(locale: Locale) -> Element {
    let mut currency = use_signal(|| "THB".to_string());
    let mut fx_rate = use_signal(|| "".to_string());
    let mut debit = use_signal(|| "10000".to_string());
    let mut credit = use_signal(|| "10000".to_string());
    let mut status_msg: Signal<Option<String>> = use_signal(|| None);
    let journals: Signal<Vec<(String, String, String, String, String)>> = use_signal(|| {
        vec![
            (
                "J-2026-0001".into(),
                "2026-08-05".into(),
                "THB".into(),
                "—".into(),
                "posted".into(),
            ),
            (
                "J-2026-0002".into(),
                "2026-08-06".into(),
                "USD".into(),
                "35.0".into(),
                "draft".into(),
            ),
            (
                "J-OPEN-000".into(),
                "2026-08-01".into(),
                "THB".into(),
                "—".into(),
                "posted".into(),
            ),
        ]
    });
    rsx! {
        div { class: "finance-journals-root", style: "display:grid;gap:16px;",
            div { class: "finance-table-wrap finance-table-wrap--journals",
                div { class: "finance-table-placeholder finance-table--journals", role: "table", aria_label: "Journals",
                    div { class: "finance-table__head finance-table__head--journals",
                        span { {copy(locale, "Journal/Period", "สมุด/งวด")} }
                        span { {copy(locale, "Currency/FX", "สกุล/FX")} }
                        span { {copy(locale, "Amount", "ยอด")} }
                        span { {copy(locale, "Status", "สถานะ")} }
                        span { {copy(locale, "Actions", "จัดการ")} }
                    }
                    for (id, date, ccy, fx, st) in journals.read().clone() {
                        div { class: "finance-table__row finance-table__row--journals",
                            span { class: "finance-mono", "{id}" br {} span { class: "finance-muted", "{date}" } }
                            span { class: "finance-mono", "{ccy}" br {} span { class: "finance-muted", "{fx}" } }
                            span { class: "finance-mono", "10,000" }
                            StatusBadge { label: st.clone(), tone: if st=="posted" { StatusTone::Success } else { StatusTone::Warning } }
                            span {
                                if st=="draft" {
                                    Button { label: copy(locale, "Post", "ลงบัญชี").to_owned(), tone: ButtonTone::Primary, on_press: {
                                        let idc = id.clone();
                                        move |_| status_msg.set(Some(format!("PostJournal {} requires HumanDecisionProof finance.journal.post:{}:cmd · FxRateRequired if USD without 35_000_000", idc, idc)))
                                    } }
                                } else {
                                    Button { label: copy(locale, "View", "ดู").to_owned(), tone: ButtonTone::Secondary }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(msg) = status_msg.read().clone() {
                p { class: "finance-surface__desc", style: "color:var(--erpx-warning);margin-top:4px;", "{msg}" }
            }
            section { class: "finance-form", style: "border:1px solid var(--erpx-border);padding:12px;border-radius:10px;background:var(--erpx-surface);",
                h3 { {copy(locale, "Create Journal — balanced double-entry", "สร้างสมุด — เดบิต=เครดิต")} }
                p { class: "finance-muted", style: "font-size:11px;", {copy(locale, "2..10k lines · unique line_id · exactly one debit xor credit >0 · JournalCurrencyMismatch · validate_currency 3-letter · FxRate micros 1..1e12 overflow-checked", "2..10k บรรทัด · เดบิต xor เครดิต >0 · สกุลไม่ตรงล้มเหลว · FxRate 1..1e12")} }
                div { class: "finance-form-grid-4", style: "margin-top:8px;",
                    label { {copy(locale, "Currency", "สกุล")} input { value: "{currency}", oninput: move |e| currency.set(e.value()), placeholder: "THB / USD" } }
                    label { {copy(locale, "FxRate micros", "FxRate")} input { value: "{fx_rate}", oninput: move |e| fx_rate.set(e.value()), placeholder: "35000000 for 35.0" } }
                    label { {copy(locale, "Debit minor", "เดบิต")} input { value: "{debit}", oninput: move |e| debit.set(e.value()), placeholder: "10000" } }
                    label { {copy(locale, "Credit minor", "เครดิต")} input { value: "{credit}", oninput: move |e| credit.set(e.value()), placeholder: "10000" } }
                }
                div { style: "display:flex;gap:8px;margin-top:10px;flex-wrap:wrap;",
                    Button {
                        label: copy(locale, "Create (validate)", "สร้าง (ตรวจ)").to_owned(),
                        tone: ButtonTone::Primary,
                        on_press: move |_| {
                            let ccy = currency.read().to_string().to_uppercase();
                            let fx = fx_rate.read().to_string();
                            let d: i64 = debit.read().parse().unwrap_or(0);
                            let c: i64 = credit.read().parse().unwrap_or(0);
                            let msg = if ccy.len()!=3 { "validate_currency failed: 3-letter uppercase required".to_string() }
                            else if d!=c { "UnbalancedJournal: debit != credit".to_string() }
                            else if d<=0 { "Invalid amount: debit xor credit >0".to_string() }
                            else if ccy!="THB" && fx.is_empty() { "FxRateRequired: USD journal without fx_rate_micros".to_string() }
                            else if ccy=="THB" && !fx.is_empty() { "InvalidFxRate: THB journal must not have FxRate".to_string() }
                            else if ccy=="USD" && fx=="0" { "InvalidFxRate: micros 0 rejected".to_string() }
                            else {
                                let base = if ccy=="USD" { (d as i128 * fx.parse::<i128>().unwrap_or(35000000)/1_000_000) as i64 } else { d };
                                format!("Created draft J-NEW {} {} -> base {} THB · Post requires HumanDecisionProof + Closed gate check", ccy, d, base)
                            };
                            status_msg.set(Some(msg));
                        }
                    }
                    Button {
                        label: copy(locale, "Revalue (36.0)", "ตีค่าใหม่ 36.0").to_owned(),
                        tone: ButtonTone::Secondary,
                        on_press: move |_| status_msg.set(Some("FxSnapshot THB->USD 36.0 as_of 2026-08-10 → RevalueOpenSubledgers creates unrealized Journal (base_minor)".into()))
                    }
                    Button {
                        label: copy(locale, "Post with proof", "ลงบัญชีพร้อมหลักฐาน").to_owned(),
                        tone: ButtonTone::Secondary,
                        on_press: move |_| status_msg.set(Some("PostJournal requires decision_ref=finance.journal.post:{id}:{command_id} + HumanDecisionProof scope check · Closed period → InvalidPeriodTransition".into()))
                    }
                }
                p { class: "finance-table__footnote", style: "margin-top:8px;", {copy(locale, "JournalSource typed: procurement/sales/manual · idempotent via CommandId · outbox transactional", "แหล่งที่มาพิมพ์ชัด · idempotent ด้วย CommandId")} }
            }
        }
    }
}

#[component]
fn SubledgersPlaceholder(locale: Locale) -> Element {
    let mut tab = use_signal(|| "AR".to_string());
    let mut msg: Signal<Option<String>> = use_signal(|| None);
    let rows: Signal<Vec<(String, String, String, String, String)>> = use_signal(|| {
        vec![
            (
                "INV-AR-001".into(),
                "AR".into(),
                "10,000".into(),
                "THB".into(),
                "Open".into(),
            ),
            (
                "INV-AR-002".into(),
                "AR".into(),
                "6,000".into(),
                "THB".into(),
                "PartiallySettled".into(),
            ),
            (
                "INV-AP-001".into(),
                "AP".into(),
                "8,200".into(),
                "THB".into(),
                "Open".into(),
            ),
        ]
    });
    rsx! {
        div { class: "finance-subledger-placeholder",
            div { style: "display:flex;gap:8px;flex-wrap:wrap;margin-bottom:8px;align-items:center;",
                Button { label: "AR".to_owned(), tone: if tab.read().as_str()=="AR" { ButtonTone::Primary } else { ButtonTone::Secondary }, on_press: move |_| tab.set("AR".into()) }
                Button { label: "AP".to_owned(), tone: if tab.read().as_str()=="AP" { ButtonTone::Primary } else { ButtonTone::Secondary }, on_press: move |_| tab.set("AP".into()) }
                StatusBadge { label: "Hybrid: finance-direct + 3-way when PO exists".to_owned(), tone: StatusTone::Neutral }
                StatusBadge { label: "RevalueOpenSubledgers 36.0".to_owned(), tone: StatusTone::Neutral }
            }
            p { class: "finance-muted", style: "font-size:11px;margin-bottom:6px;", {copy(locale, "ListSubledger kind filter + cursor + receivable_only · ApplySubledgerSettlement amount >0 ≤ outstanding · InvalidSettlement guard", "กรองชนิด + ชำระ >0 ≤ คงค้าง")} }
            div { class: "finance-table-wrap finance-subledger-table-wrap",
                div { class: "finance-table-placeholder",
                    div { class: "finance-table__head",
                        span { "Invoice" } span { "Kind" } span { "Outstanding" } span { "Status" } span { "Actions" }
                    }
                    for (id,kind,amt,ccy,st) in rows.read().clone().into_iter().filter(|(_,k,_,_,_)| k==tab.read().as_str()) {
                        div { class: "finance-table__row",
                            span { class: "finance-mono", "{id}" }
                            span { "{kind}" }
                            span { class: "finance-mono", "{amt} {ccy}" }
                            StatusBadge { label: st.clone(), tone: if st=="Settled" { StatusTone::Success } else if st=="PartiallySettled" { StatusTone::Warning } else { StatusTone::Neutral } }
                            span {
                                if st!="Settled" {
                                    Button { label: copy(locale, "Settle 4,000", "ชำระ").to_owned(), tone: ButtonTone::Primary, on_press: {
                                        let idc=id.clone();
                                        move |_| msg.set(Some(format!("ApplySubledgerSettlement {} 4,000 → outstanding -4,000 · over-settle → InvalidSettlement · hybrid via PostingValidationSnapshot", idc)))
                                    } }
                                } else {
                                    span { class: "finance-muted", {copy(locale, "Settled", "ชำระแล้ว")} }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(m) = msg.read().clone() { p { style: "color:var(--erpx-warning);font-size:12px;margin-top:8px;", "{m}" } }
            p { class: "finance-table__footnote", {copy(locale, "Origin: ReleasedPurchaseOrderFact (procurement) for AP, CustomerInvoicePostedFact (sales) for AR · provenance badge shown", "ต้นทาง: PO สำหรับ AP, ใบแจ้งหนี้โพสต์สำหรับ AR")} }
        }
    }
}

#[component]
fn PayablesPlaceholder(locale: Locale) -> Element {
    let mut msg: Signal<Option<String>> = use_signal(|| None);
    let mut po_link = use_signal(|| "".to_string());
    rsx! {
        div { class: "finance-payables-placeholder",
            div { style: "display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:8px;",
                h3 { {copy(locale, "Vendor Invoices → Payables", "ใบผู้ขาย → เจ้าหนี้")} }
                Button { label: copy(locale, "Create Vendor Invoice", "สร้างใบผู้ขาย").to_owned(), tone: ButtonTone::Primary, on_press: move |_| msg.set(Some("CreateVendorInvoice: vendor + amount + currency · JournalCurrencyMismatch guard".into())) }
            }
            p { class: "finance-muted", style: "font-size:11px;", {copy(locale, "Hybrid: CreateVendorInvoice accepts optional ThreeWayMatchEvidence when source=PO; direct finance invoices bypass match but still enforce JournalCurrencyMismatch", "ไฮบริด: กรณีมี PO ต้องแนบตรวจสามทาง ไม่เช่นนั้นตรงได้แต่สกุลต้องตรง")} }
            div { class: "finance-table-wrap finance-payables-table-wrap",
                div { class: "finance-table-placeholder",
                    div { class: "finance-table__head",
                        span { "Invoice" } span { "Vendor" } span { "PO / Match" } span { "Status" }
                    }
                    div { class: "finance-table__row",
                        span { class: "finance-mono", "VI-2026-001" } span { "Acme Supplies" } span { if po_link.read().is_empty() { {copy(locale, "Direct (no PO)", "ตรง")} } else { "PO-2026-001 ✓" } } StatusBadge { label: "posted".to_owned(), tone: StatusTone::Success }
                    }
                    div { class: "finance-table__row",
                        span { class: "finance-mono", "VI-2026-002" } span { "Beta Corp" } span { class: "finance-muted", "PO-2026-014 · pending 3-way" } StatusBadge { label: "draft".to_owned(), tone: StatusTone::Warning }
                    }
                }
            }
            div { class: "finance-form", style: "margin-top:12px;border:1px solid var(--erpx-border);padding:12px;border-radius:10px;background:var(--erpx-surface);",
                h4 { {copy(locale, "Three-Way Match (optional)", "ตรวจสามทาง (เลือก)")} }
                p { class: "finance-muted", style: "font-size:11px;", "PO → Receipt → Invoice tolerance check via ThreeWayMatchLineEvidence" }
                div { style: "display:flex;gap:8px;margin-top:6px;flex-wrap:wrap;align-items:center;",
                    input { value: "{po_link}", oninput: move |e| po_link.set(e.value()), placeholder: "PO-2026-001 or empty for direct" }
                    Button { label: "Validate Match".to_owned(), tone: ButtonTone::Secondary, on_press: move |_| {
                        let v = po_link.read().to_string();
                        let m = if v.is_empty() { "Direct finance invoice: bypass match but JournalCurrencyMismatch still enforced".to_string() }
                        else if v.starts_with("PO-") { format!("ThreeWayMatchEvidence {} → tolerance pass · PostingValidationSnapshot wired via procurement", v) }
                        else { "Invalid PO ref: must be PO-xxx or empty".to_string() };
                        msg.set(Some(m));
                    } }
                }
            }
            if let Some(m) = msg.read().clone() { p { style: "color:var(--erpx-warning);font-size:12px;margin-top:8px;", "{m}" } }
        }
    }
}

#[component]
fn ReceivablesPlaceholder(locale: Locale) -> Element {
    let mut msg: Signal<Option<String>> = use_signal(|| None);
    rsx! {
        div { class: "finance-receivables-placeholder",
            div { style: "display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:8px;",
                h3 { {copy(locale, "Customer Invoices & Revenue Schedules", "ใบลูกหนี้ & ตารางรับรู้รายได้")} }
                div { style: "display:flex;gap:6px;flex-wrap:wrap;",
                    Button { label: copy(locale, "New Invoice", "สร้างใบแจ้ง").to_owned(), tone: ButtonTone::Primary, on_press: move |_| msg.set(Some("CreateCustomerInvoiceFromBilling → finance-direct".into())) }
                    Button { label: copy(locale, "Recognize Revenue", "รับรู้รายได้").to_owned(), tone: ButtonTone::Secondary, on_press: move |_| msg.set(Some("RecognizeRevenueFromFulfillment independent of invoice → dimensions bound to scope".into())) }
                }
            }
            p { class: "finance-muted", style: "font-size:11px;margin-bottom:6px;", {copy(locale, "PostCustomerInvoice requires HumanDecisionProof maker-checker prepared_by≠posted_by · InvoiceApprovalSeparation · CancelCustomerInvoice Draft only", "ลงบัญชีต้อง คนทำ≠คนอนุมัติ")} }
            div { class: "finance-table-wrap finance-receivables-table-wrap",
                div { class: "finance-table-placeholder",
                    div { class: "finance-table__head",
                        span { "Invoice" } span { "Customer" } span { "Amount / Status" } span { "Action" }
                    }
                    for (id,cust,amt,stat) in [("CI-2026-011","Siam Retail","THB 24,000","posted"),("CI-2026-012","Bangkok Co","THB 12,000","draft"),("CI-2026-013","Chiang Mai Ltd","USD 1,200","draft")] {
                        div { class: "finance-table__row",
                            span { class: "finance-mono", "{id}" } span { "{cust}" } span { class: "finance-mono", "{amt}" br{} StatusBadge { label: stat.to_owned(), tone: if stat=="posted" { StatusTone::Success } else { StatusTone::Warning } } }
                            span {
                                if stat=="draft" {
                                    Button { label: copy(locale, "Post", "ลงบัญชี").to_owned(), tone: ButtonTone::Primary, on_press: {
                                        let idc=id.to_string();
                                        move |_| msg.set(Some(format!("PostCustomerInvoice {} requires HumanDecisionProof · prepared_by≠posted_by · publishes CustomerInvoicePostedFact before AR", idc)))
                                    } }
                                } else {
                                    Button { label: "View".to_owned(), tone: ButtonTone::Secondary }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "finance-form", style: "margin-top:12px;border:1px solid var(--erpx-border);padding:12px;border-radius:10px;background:var(--erpx-surface);",
                h4 { "Revenue Schedule" }
                p { class: "finance-muted", style: "font-size:11px;", "Arrangement → fulfillment → RecognizeRevenueFromFulfillment (independent of invoice) · dimensions bound to scope" }
                div { style: "display:grid;grid-template-columns:1fr 1fr 1fr;gap:8px;margin-top:6px;",
                    div { class: "finance-table__head", "Schedule" } div { class: "finance-table__head", "Fulfillment" } div { class: "finance-table__head", "Recognized" }
                }
                div { class: "finance-table__row", style: "padding:8px 0;",
                    span { class: "finance-mono", "RS-2026-001" } span { "80% fulfilled" } StatusBadge { label: "partial".to_owned(), tone: StatusTone::Warning }
                }
            }
            if let Some(m) = msg.read().clone() { p { style: "color:var(--erpx-warning);font-size:12px;margin-top:8px;", "{m}" } }
        }
    }
}

#[component]
fn CashPlaceholder(locale: Locale) -> Element {
    rsx! {
        div { class: "finance-cash-placeholder",
            div { class: "finance-cash__grid",
                for (acct, pos, delta) in [
                    ("SCB Main · THB", "THB 1,240,500", "+12,400"),
                    ("KBank Ops · THB", "THB 482,000", "—"),
                    ("USD Holding · USD", "USD 12,400", "+320"),
                ] {
                    article { class: "finance-cash__card", style: "border:1px solid var(--erpx-border);padding:10px;border-radius:8px;",
                        h4 { "{acct}" }
                        p { class: "finance-mono finance-cash__value", style: "font-size:16px;font-weight:600;", "{pos}" }
                        p { class: "finance-muted", style: "font-size:11px;", "Δ {delta} · Updated just now · via ListBankTransactions + TrialBalance cash" }
                    }
                }
            }
            div { class: "finance-cash__summary", style: "margin-top:12px;border:1px solid var(--erpx-border);padding:10px;border-radius:8px;display:flex;justify-content:space-between;align-items:center;",
                div {
                    p { {copy(locale, "Cash aggregated from AP/AR settlements and bank reconciliation. No import here — use Banking.", "เงินสดรวมจากการชำระ AP/AR และการกระทบยอด")} }
                    p { class: "finance-muted", style: "font-size:11px;", {copy(locale, "Read-only rollup · scope-fenced by Branch · Reconciliation status links to Banking", "สรุปอย่างเดียว · ตาม Branch")} }
                }
                Button { label: copy(locale, "Go to Banking", "ไปธนาคาร").to_owned(), tone: ButtonTone::Primary }
            }
            div { style: "margin-top:12px;",
                h4 { {copy(locale, "Reconciliation status", "สถานะกระทบยอด")} }
                p { class: "finance-muted", style: "font-size:11px;", {copy(locale, "Bank → ListBankTransactions by posted_from/through + cursor · Reconciliation = SaveBankReconciliation(expected_version) with Conflicted on mismatch", "รายการธนาคารตามช่วง + เคอร์เซอร์")} }
                div { style: "display:flex;gap:8px;margin-top:6px;",
                    StatusBadge { label: "Reconciled 2026-08-10".to_owned(), tone: StatusTone::Success }
                    StatusBadge { label: "3 unmatched".to_owned(), tone: StatusTone::Warning }
                    StatusBadge { label: "Branch scope".to_owned(), tone: StatusTone::Neutral }
                }
            }
        }
    }
}

#[component]
fn ChartTreePlaceholder(locale: Locale) -> Element {
    let mut csv_text = use_signal(|| {
        "code,name,category,parent\n1000,Assets,Asset,\n1100,Cash,Asset,1000\n".to_string()
    });
    let mut msg: Signal<Option<String>> = use_signal(|| None);
    let mut accounts: Signal<Vec<(String, String, String, String)>> = use_signal(|| {
        vec![
            ("1000".into(), "Assets".into(), "Asset".into(), "—".into()),
            (
                "1100".into(),
                "Cash on hand".into(),
                "Asset".into(),
                "1000".into(),
            ),
            (
                "2100".into(),
                "Payables".into(),
                "Liability".into(),
                "—".into(),
            ),
            ("4100".into(), "Sales".into(), "Revenue".into(), "—".into()),
            (
                "5100".into(),
                "Expenses".into(),
                "Expense".into(),
                "—".into(),
            ),
        ]
    });
    rsx! {
        div { class: "finance-chart-layout",
            div { style: "flex:1;",
                div { style: "display:flex;gap:8px;margin-bottom:8px;align-items:center;",
                    h3 { {copy(locale, "Chart MAIN · THB · Company scope", "ผัง MAIN · THB")} }
                    StatusBadge { label: "Holding isolation".to_owned(), tone: StatusTone::Success }
                    StatusBadge { label: "1..4096 accounts".to_owned(), tone: StatusTone::Neutral }
                }
                ul { class: "finance-chart-tree",
                    for (code, name, cat, parent) in accounts.read().clone() {
                        li { class: "finance-chart-tree__node",
                            div { class: "finance-chart-tree__row",
                                span { class: "finance-mono finance-chart-tree__name", "{code} · {name}" }
                                span { class: "finance-muted", "{cat} · parent {parent}" }
                                StatusBadge { label: "Active".to_owned(), tone: StatusTone::Success }
                                Button { label: copy(locale, "Inactive", "พัก").to_owned(), tone: ButtonTone::Secondary, on_press: {
                                    let c = code.clone();
                                    move |_| msg.set(Some(format!("ChangeAccountStatus {} -> Inactive (Active↔Inactive) · validate parent cycle guard", c)))
                                } }
                            }
                        }
                    }
                }
                if let Some(m) = msg.read().clone() {
                    p { style: "color:var(--erpx-warning);margin-top:8px;font-size:12px;", "{m}" }
                }
            }
            section { class: "finance-chart__detail",
                h3 { {copy(locale, "Import / Create", "นำเข้า / สร้าง")} }
                p { class: "finance-muted", style: "font-size:11px;", {copy(locale, "ImportChartCsv 2-phase code→id (out-of-order parents) · duplicate code → DuplicateAccount · parent cycle guard · validate_currency", "นำเข้า 2 เฟส · โค้ดซ้ำล้มเหลว")} }
                textarea { style: "width:100%;height:80px;font-family:monospace;font-size:11px;margin-top:6px;", value: "{csv_text}", oninput: move |e| csv_text.set(e.value()) }
                div { style: "display:flex;gap:6px;margin-top:6px;",
                    Button { label: copy(locale, "Validate CSV", "ตรวจ CSV").to_owned(), tone: ButtonTone::Primary, on_press: move |_| {
                        let t = csv_text.read().to_string();
                        let lines: Vec<&str> = t.lines().collect();
                        let m = if lines.len()<2 { "CSV needs header + rows".to_string() }
                        else if t.contains("1100") && t.matches("1100").count()>1 { "DuplicateAccount: code 1100 appears twice".to_string() }
                        else { format!("validate_chart_csv_rows: {} rows ok · hierarchy 1001→1000 validated · idempotent via CommandId", lines.len()-1) };
                        msg.set(Some(m));
                    } }
                    Button { label: copy(locale, "Add Account 5101", "เพิ่มบัญชี").to_owned(), tone: ButtonTone::Secondary, on_press: move |_| {
                        let mut v = accounts.read().clone();
                        if v.iter().any(|(c,_,_,_)| c=="5101") { msg.set(Some("DuplicateAccount: 5101 already exists".into())); }
                        else { v.push(("5101".into(),"Office Exp".into(),"Expense".into(),"—".into())); accounts.set(v); msg.set(Some("AddAccount 5101 Expense → total persists via finance-sqlx · Chart::create validates 1..4096".into())); }
                    } }
                }
                dl { class: "finance-chart__facts", style: "margin-top:12px;",
                    div { dt { "Base CCY" } dd { class: "finance-mono", "THB (validate_currency)" } }
                    div { dt { "Scope" } dd { "Company holding/company exact" } }
                    div { dt { "Idempotent" } dd { "CommandId" } }
                }
            }
        }
    }
}

#[component]
fn BankingJobsPlaceholder(locale: Locale, on_retry: Option<EventHandler<()>>) -> Element {
    let mut csv = use_signal(|| {
        "date,amount,currency,ref\n2026-08-01,12400,THB,TR-001\n2026-08-02,8200,THB,TR-002\n"
            .to_string()
    });
    let mut msg: Signal<Option<String>> = use_signal(|| None);
    let jobs: Signal<Vec<(String, String, String)>> = use_signal(|| {
        vec![
            (
                "bank-2026-08-29.csv".into(),
                "succeeded".into(),
                "42 matched".into(),
            ),
            (
                "bank-2026-08-28.csv".into(),
                "failed".into(),
                "CSV error".into(),
            ),
        ]
    });
    rsx! {
        section { class: "finance-banking-jobs",
            h3 { {copy(locale, "Banking — CSV-only v1", "ธนาคาร — CSV เท่านั้น")} }
            div { class: "finance-bank-pipeline__badges",
                StatusBadge { label: "FINANCE_BANK_PROVIDER_ENABLED=false".to_owned(), tone: StatusTone::Danger }
                StatusBadge { label: "erpx_control.finance_bank_provider_import_jobs OFF".to_owned(), tone: StatusTone::Neutral }
                StatusBadge { label: "Idempotent CommandId".to_owned(), tone: StatusTone::Neutral }
            }
            p { class: "finance-muted", style: "font-size:11px;overflow-wrap:anywhere;", {copy(locale, "CSV → staging finance_bank_transactions → SaveBankMatchProposal (system) → SaveBankMatchDecision(decision_ref human) → SaveBankReconciliation(expected_version) idempotent · Conflicted on version mismatch", "CSV → พัก → เสนอ → อนุมัติ → กระทบยอด")} }
            section { class: "finance-form", style: "margin-top:12px;",
                h4 { {copy(locale, "Import CSV", "นำเข้า CSV")} }
                textarea { value: "{csv}", oninput: move |e| csv.set(e.value()), placeholder: "date,amount,currency,ref" }
                div { style: "display:flex;gap:8px;margin-top:8px;flex-wrap:wrap;",
                    Button { label: copy(locale, "Import & Stage", "นำเข้า").to_owned(), tone: ButtonTone::Primary, on_press: move |_| msg.set(Some("ImportBankStatement CSV staging idempotent via CommandId/idempotency_key → finance_bank_transactions".into())) }
                    Button { label: copy(locale, "Propose Match (system)", "เสนอจับคู่").to_owned(), tone: ButtonTone::Secondary, on_press: move |_| msg.set(Some("SaveBankMatchProposal (system) → match candidates".into())) }
                    Button { label: copy(locale, "Decide (human)", "อนุมัติ").to_owned(), tone: ButtonTone::Secondary, on_press: move |_| msg.set(Some("SaveBankMatchDecision(decision_ref) human → HumanDecisionProof".into())) }
                    Button { label: copy(locale, "Reconcile (version)", "กระทบยอด").to_owned(), tone: ButtonTone::Secondary, on_press: move |_| msg.set(Some("SaveBankReconciliation(expected_version) idempotent → Conflicted on mismatch".into())) }
                }
            }
            section { style: "margin-top:12px;",
                h4 { {copy(locale, "Staging & Proposals", "พัก & เสนอ")} }
                div { class: "finance-table-wrap finance-banking-table-wrap",
                    div { class: "finance-table-placeholder",
                        div { class: "finance-table__head",
                            span { "File" } span { "Status" } span { "Detail" }
                        }
                        for (file, st, detail) in jobs.read().clone() {
                            div { class: "finance-table__row",
                                span { class: "finance-mono", "{file}" } StatusBadge { label: st.clone(), tone: if st=="succeeded" { StatusTone::Success } else { StatusTone::Danger } } span { class: "finance-muted", "{detail}" }
                            }
                        }
                    }
                }
            }
            section { class: "finance-form", style: "margin-top:12px;",
                h4 { "Reconciliation" }
                p { class: "finance-muted", "42 transactions matched automatically. 3 need review. ListBankTransactions by posted_from/through + cursor." }
                div { style: "display:flex;gap:8px;margin-top:8px;flex-wrap:wrap;",
                    Button { label: "Review matches".to_owned(), tone: ButtonTone::Primary }
                    Button { label: "Retry".to_owned(), tone: ButtonTone::Secondary, on_press: on_retry }
                }
            }
            if let Some(m) = msg.read().clone() { p { style: "color:var(--erpx-warning);font-size:12px;margin-top:8px;", "{m}" } }
        }
    }
}

#[component]
fn PeriodClosePlaceholder(locale: Locale) -> Element {
    let mut period_status: Signal<String> = use_signal(|| "open".to_string());
    let mut msg: Signal<Option<String>> = use_signal(|| None);
    let mut tried_post_while_closed = use_signal(|| false);
    rsx! {
        section { class: "finance-period-close",
            div { class: "finance-bank-pipeline__badges",
                StatusBadge { label: format!("Period 2026-08 · {}", period_status.read().to_string()).to_owned(), tone: if period_status.read().as_str()=="closed" { StatusTone::Danger } else if period_status.read().as_str()=="closing" { StatusTone::Warning } else { StatusTone::Success } }
                StatusBadge { label: "per-Company AuthorityScope".to_owned(), tone: StatusTone::Neutral }
                StatusBadge { label: "HumanDecisionProof required".to_owned(), tone: StatusTone::Neutral }
            }
            p { class: "finance-muted", style: "font-size:11px;overflow-wrap:anywhere;", {copy(locale, "CreatePeriod + Get/ListPeriods period_id fence · BeginPeriodClose → ClosePeriod(decision) → ReopenPeriod(decision) · CloseApprovalSeparation alice≠bob, carol≠bob tied to CommandId+at+scope · Idempotent via CommandId", "สร้าง/ดูงวด · เริ่มปิด → ปิด (ตัดสินใจ) → เปิดใหม่ · คนปิด≠คนอนุมัติ")} }
            div { class: "finance-table-wrap finance-period-table-wrap",
                div { class: "finance-table-placeholder",
                    div { class: "finance-table__head",
                        span { "Period" } span { "Status" } span { "Trial Δ" } span { "Actions" }
                    }
                    div { class: "finance-table__row",
                        span { class: "finance-mono", "2026-08" }
                        StatusBadge { label: period_status.read().to_string(), tone: if period_status.read().as_str()=="closed" { StatusTone::Danger } else { StatusTone::Success } }
                        span { class: "finance-mono", "THB 0.00 Δ" }
                        span { class: "finance-chip-row",
                            if period_status.read().as_str()=="open" {
                                Button { label: copy(locale, "Begin Close (alice)", "เริ่มปิด (alice)").to_owned(), tone: ButtonTone::Secondary, on_press: move |_| { period_status.set("closing".into()); msg.set(Some("BeginPeriodClose alice → status Closing".into())); } }
                            } else if period_status.read().as_str()=="closing" {
                                Button { label: copy(locale, "Close (bob + proof)", "ปิด (bob)").to_owned(), tone: ButtonTone::Primary, on_press: move |_| { period_status.set("closed".into()); msg.set(Some("ClosePeriod bob decision_ref=finance.period.close:2026-08:cmd → Closed".into())); } }
                            } else {
                                Button { label: copy(locale, "Reopen (carol)", "เปิดใหม่ (carol)").to_owned(), tone: ButtonTone::Primary, on_press: move |_| { period_status.set("open".into()); msg.set(Some("ReopenPeriod carol decision_ref=finance.period.reopen:2026-08:cmd".into())); } }
                            }
                        }
                    }
                    div { class: "finance-table__row",
                        span { class: "finance-mono", "2026-07" }
                        StatusBadge { label: "closed".to_owned(), tone: StatusTone::Danger }
                        span { class: "finance-mono", "THB 0.00 Δ" }
                        span { StatusBadge { label: "locked".to_owned(), tone: StatusTone::Neutral } }
                    }
                }
            }
            div { class: "finance-period-close__gate",
                h4 { {copy(locale, "Hard Gate Demo", "สาธิตประตูล็อก")} }
                p { {copy(locale, "When period is Closed, PostJournal / ApplySubledgerSettlement / RecognizeRevenue with entry_date in period must be rejected → InvalidPeriodTransition. TrialBalance/SubledgerList filters by period_id.", "เมื่องวดปิด การลงบัญชีต้องถูกปฏิเสธ")} }
                div { style: "margin-top:10px;display:flex;gap:8px;flex-wrap:wrap;",
                    Button { label: copy(locale, "Try Post in Closed", "ลองลงบัญชีตอนปิด").to_owned(), tone: ButtonTone::Secondary, on_press: move |_| { tried_post_while_closed.set(true); msg.set(Some("PostJournal in Closed → InvalidPeriodTransition (hard gate)".into())); } }
                    if *tried_post_while_closed.read() && period_status.read().as_str()=="closed" {
                        StatusBadge { label: "InvalidPeriodTransition".to_owned(), tone: StatusTone::Danger }
                    }
                }
            }
            if let Some(m) = msg.read().clone() { p { style: "color:var(--erpx-warning);font-size:12px;margin-top:8px;overflow-wrap:anywhere;", "{m}" } }
            p { class: "finance-table__footnote", {copy(locale, "Closing locks postings for the period. Reopen requires approval. TrialBalance/SubledgerList filtered by period_id.", "การปิดล็อกการลงบัญชี การเปิดใหม่ต้องอนุมัติ")} }
        }
    }
}

#[component]
fn BudgetVariancePlaceholder(locale: Locale) -> Element {
    rsx! {
        section { class: "finance-budgets",
            div { class: "finance-bank-pipeline__badges",
                StatusBadge { label: "Deferred in v1".to_owned(), tone: StatusTone::Warning }
                StatusBadge { label: "Route kept for nav stability".to_owned(), tone: StatusTone::Neutral }
            }
            p { {copy(locale, "Variance = Actual − Plan. Export CSV available.", "ผลต่าง = จริง − แผน พร้อมส่งออก CSV")} }
            p { class: "finance-muted", style: "font-size:11px;margin-bottom:8px;", {copy(locale, "Budget vs actual, fixed-asset depreciation, inventory costing/variance, intercompany eliminations are explicitly deferred. GetStatutoryTaxReport returns empty/generic (intl-only).", "งบประมาณ เทียบ จริง ค่าเสื่อม ต้นทุนสต๊อก ตัดระหว่างบริษัท เลื่อนออกไป")} }
            div { class: "finance-budgets__grid",
                div { class: "finance-budgets__row",
                    span { "Marketing" } span { class: "finance-mono", "THB 420k / 600k" }
                    div { class: "finance-budgets__bar-wrap",
                        div { class: "finance-budgets__bar-track", div { class: "finance-budgets__bar", style: "width:70%;background:var(--erpx-success);" } }
                        span { class: "finance-budgets__bar-label", "70% · within plan" }
                    }
                }
                div { class: "finance-budgets__row",
                    span { "Operations" } span { class: "finance-mono", "THB 1.1M / 1.0M" }
                    div { class: "finance-budgets__bar-wrap",
                        div { class: "finance-budgets__bar-track", div { class: "finance-budgets__bar", style: "width:110%;background:var(--erpx-danger);" } }
                        span { class: "finance-budgets__bar-label", "110% · over budget" }
                    }
                }
                div { class: "finance-budgets__row",
                    span { "Finance" } span { class: "finance-mono", "THB 88k / 120k" }
                    div { class: "finance-budgets__bar-wrap",
                        div { class: "finance-budgets__bar-track", div { class: "finance-budgets__bar", style: "width:73%;background:var(--erpx-warning);" } }
                        span { class: "finance-budgets__bar-label", "73% · under" }
                    }
                }
            }
            div { style: "margin-top:12px;display:flex;gap:8px;flex-wrap:wrap;",
                Button { label: copy(locale, "Export CSV (deferred)", "ส่งออก CSV (เลื่อน)").to_owned(), tone: ButtonTone::Secondary }
                Button { label: copy(locale, "View ADR", "ดู ADR").to_owned(), tone: ButtonTone::Secondary }
            }
            p { class: "finance-table__footnote", "FINANCE_ROUTES completeness kept · audits deferred to v2" }
        }
    }
}

#[component]
fn FinanceMentionComposer(locale: Locale) -> Element {
    let mut draft = use_signal(String::new);
    let resolved: Signal<Vec<TypedToken>> = use_signal(Vec::new);
    let mut autocomplete_query: Signal<Option<String>> = use_signal(|| None);
    let suggestions: Vec<(&'static str, &'static str, &'static str, &'static str)> = vec![
        (
            "@record:00000000-0000-4000-8000-000000000001",
            "PO-2026-001",
            "Procurement · PO-2026-001 · Item Directory",
            "record",
        ),
        (
            "@record:00000000-0000-4000-8000-000000000002",
            "SO-2026-114",
            "Sales · SO-2026-114 · deduped edge",
            "record",
        ),
        (
            "@record:00000000-0000-4000-8000-000000000003",
            "RCV-2026-09",
            "Inventory · RCV-2026-09 · max 50/host ordered",
            "record",
        ),
        (
            "@user:00000000-0000-4000-8000-000000000004",
            "A. Finance",
            "People · approver · @user",
            "user",
        ),
        (
            "@group:00000000-0000-4000-8000-000000000005:v1",
            "Finance Approvers",
            "Group · Finance Approvers · @group:v1",
            "group",
        ),
        (
            "@field:00000000-0000-4000-8000-000000000010:amount@v1",
            "Journal amount",
            "Field · amount@v1 · @field",
            "field",
        ),
    ];
    rsx! {
        section { class: "finance-mention-composer", aria_label: copy(locale, "Mention composer", "กล่องเมนชัน"),
            h3 { {copy(locale, "Add a note with @ mentions", "เพิ่มบันทึกพร้อม @ เมนชัน")} }
            p { class: "finance-mention-composer__hint", {copy(locale, "Type @ to reference people, agents, or any record: PO, SO, receipt, cost center, journal, invoice… References persist as ordered deduped edges (max 50/host) and appear bidirectionally via Item Directory.", "พิมพ์ @ เพื่ออ้างอิงบุคคล เอเจนต์ หรือรายการใด ๆ: PO, SO, ใบรับของ, cost center, สมุดรายวัน, ใบแจ้งหนี้… การอ้างอิงจะคงอยู่เป็น edge แบบเรียงและไม่ซ้ำ (สูงสุด 50/host) และปรากฏทั้งสองทิศทางผ่าน Item Directory")} }
            div { class: "finance-mention-composer__field",
                textarea {
                    class: "finance-mention-composer__input",
                    placeholder: copy(locale, "Type @ to mention… e.g. @record:… or @user:…", "พิมพ์ @ เพื่อเมนชัน… เช่น @record:… หรือ @user:…"),
                    value: "{draft}",
                    oninput: move |e: FormEvent| {
                        let v = e.value();
                        if let Some(at) = v.rfind('@') {
                            let after = &v[at + 1..];
                            let q = after
                                .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == ')' || c == '(' || c == '"' || c == '\'')
                                .next()
                                .unwrap_or("")
                                .to_string();
                            if q.is_empty() {
                                autocomplete_query.set(Some(String::new()));
                            } else {
                                autocomplete_query.set(Some(q));
                            }
                        } else {
                            autocomplete_query.set(None);
                        }
                        draft.set(v);
                    },
                }
                if let Some(q) = autocomplete_query.read().as_ref().cloned() {
                    {
                        let q_lower = q.to_ascii_lowercase();
                        let filtered: Vec<(&'static str, &'static str, &'static str, &'static str)> = suggestions
                            .iter()
                            .copied()
                            .filter(|(tok, name, sub, badge)| {
                                if q.is_empty() {
                                    return true;
                                }
                                let ql = q_lower.as_str();
                                tok.to_ascii_lowercase().contains(ql)
                                    || name.to_ascii_lowercase().contains(ql)
                                    || sub.to_ascii_lowercase().contains(ql)
                                    || badge.to_ascii_lowercase().contains(ql)
                            })
                            .collect();
                        rsx! {
                            div { class: "feed-autocomplete",
                                div { class: "feed-autocomplete__list", role: "listbox", aria_label: "Mention suggestions",
                                    if filtered.is_empty() {
                                        div { class: "feed-autocomplete__empty", {copy(locale, "No matches — try @record, @user, @group, @field", "ไม่พบ — ลอง @record, @user, @group, @field")} }
                                    } else {
                                        for (token, name, sub, badge) in filtered {
                                            {
                                                let tok = token.to_string();
                                                let nm = name.to_string();
                                                let sb = sub.to_string();
                                                let bd = badge.to_string();
                                                let initial = nm.chars().next().unwrap_or('•').to_ascii_uppercase().to_string();
                                                rsx! {
                                                    button {
                                                        class: "feed-autocomplete__item",
                                                        role: "option",
                                                        onclick: {
                                                            let tok_clone = tok.clone();
                                                            let mut d = draft;
                                                            let mut aq = autocomplete_query;
                                                            move |_| {
                                                                if parse_pasted_typed_token(&tok_clone).is_some() {
                                                                    let mut t = d.read().clone();
                                                                    if let Some(pos) = t.rfind('@') {
                                                                        let before = t[..pos].to_string();
                                                                        let after_q = &t[pos + 1..];
                                                                        let q_end = after_q
                                                                            .find(|c: char| c.is_whitespace())
                                                                            .unwrap_or(after_q.len());
                                                                        let rest = if q_end < after_q.len() { &after_q[q_end..] } else { "" };
                                                                        t = format!("{}{}{}", before, tok_clone, rest);
                                                                        if !t.ends_with(' ') {
                                                                            t.push(' ');
                                                                        }
                                                                        d.set(t);
                                                                    } else {
                                                                        let mut cur = d.read().clone();
                                                                        if !cur.is_empty() && !cur.ends_with(' ') {
                                                                            cur.push(' ');
                                                                        }
                                                                        cur.push_str(&tok_clone);
                                                                        cur.push(' ');
                                                                        d.set(cur);
                                                                    }
                                                                } else {
                                                                    let mut cur = d.read().clone();
                                                                    if !cur.is_empty() && !cur.ends_with(' ') {
                                                                        cur.push(' ');
                                                                    }
                                                                    cur.push_str(&tok_clone);
                                                                    cur.push(' ');
                                                                    d.set(cur);
                                                                }
                                                                aq.set(None);
                                                            }
                                                        },
                                                        div { class: "feed-autocomplete__item-avatar", "{initial}" }
                                                        div { class: "feed-autocomplete__item-main",
                                                            div { class: "feed-autocomplete__item-name", "{tok}" }
                                                            div { class: "feed-autocomplete__item-sub", "{sb}" }
                                                        }
                                                        span { class: "feed-autocomplete__item-badge", "{bd}" }
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
            }
            div { class: "finance-mention-composer__resolved",
                for tok in resolved.read().iter().cloned() {
                    FinanceMentionPill { token: tok.clone() }
                }
            }
            div { class: "finance-mention-composer__chips",
                for kind in FinanceMentionKind::ALL {
                    span { class: "finance-mention-chip", title: kind.record_type(), "{kind.label(locale)} · {kind.record_type()}" }
                }
            }
        }
    }
}

#[component]
fn FinanceMentionPill(token: TypedToken) -> Element {
    let label = token.display_label.clone();
    let clip = token
        .clipboard_token()
        .unwrap_or_else(|| "Restricted item".to_string());
    let restricted = token.restricted;
    rsx! {
        span { class: if restricted { "finance-mention-pill finance-mention-pill--restricted" } else { "finance-mention-pill" }, title: clip.clone(),
            if restricted { "Restricted item" } else { "{label}" }
            span { class: "finance-mention-pill__token", "{clip}" }
        }
    }
}

#[component]
fn FinanceCrossDeptPalette(locale: Locale) -> Element {
    rsx! {
        section { class: "finance-cross-palette",
            h3 { {copy(locale, "This workspace as a mention target", "พื้นที่นี้เป็นเป้าหมายเมนชัน")} }
            p { {copy(locale, "Any finance record can be @mentioned elsewhere as @record:<uuid> and its fields as @field:<uuid>:<field>@v<ver>.", "ทุกรายการการเงินสามารถถูก @เมนชันจากที่อื่นเป็น @record:<uuid> และฟิลด์เป็น @field:<uuid>:<field>@v<ver>")} }
            ul {
                li { code { "@record:<journal-uuid>" } " — " {copy(locale, "Journal", "สมุดรายวัน")} }
                li { code { "@field:<journal-uuid>:amount@v1" } " — " {copy(locale, "Amount field", "ฟิลด์ยอดเงิน")} }
                li { code { "@record:<invoice-uuid>" } " — " {copy(locale, "Customer / Vendor invoice", "ใบแจ้งหนี้ลูกหนี้/ผู้ขาย")} }
            }
        }
    }
}

#[component]
fn FinanceActivityRail(locale: Locale) -> Element {
    rsx! {
        section { class: "finance-activity-rail",
            h3 { {copy(locale, "Activity & references", "กิจกรรมและการอ้างอิง")} }
            p { {copy(locale, "Record-anchored threads and backlinks appear here. @ mentions create bidirectional references via the universal reference graph (cap 50/host, deduped, ordered).", "เธรดที่ผูกกับรายการและการอ้างอิงย้อนกลับจะแสดงที่นี่ @ เมนชันสร้างการอ้างอิงสองทิศทางผ่านกราฟอ้างอิงสากล (จำกัด 50/โฮสต์)")} }
            div { class: "finance-activity-rail__placeholder", role: "status",
                p { {copy(locale, "No discussion yet — start a thread with @ mentions. Finance records are resolvable as @record:<uuid> and fields as @field:<uuid>:<field>@v<ver>.", "ยังไม่มีการสนทนา — เริ่มเธรดพร้อม @ เมนชัน รายการการเงินอ้างอิงได้เป็น @record:<uuid>")} }
            }
            p { class: "finance-activity-rail__hint", {copy(locale, "Detail pages (/finance/journal/<id>, /finance/vendor-invoice/<id>, etc.) show the live collaboration rail with references and threads.", "หน้ารายละเอียดแสดงรางการทำงานร่วมกันแบบสด")} }
        }
    }
}

/// Finance record detail with mention rail (simplified stub - full collaboration rail wired via record_collaboration_delivery).
#[component]
pub fn FinanceRecordDetailWithMentions(locale: Locale) -> Element {
    rsx! {
        section { class: "finance-record-detail",
            p { {copy(locale, "Detail with mentions — resolved via Item Directory", "รายละเอียดพร้อมเมนชัน — แก้ไขผ่าน Item Directory")} }
            FinanceMentionComposer { locale }
        }
    }
}

/// Autocomplete composer stub using collaboration typed tokens.
#[component]
pub fn FinanceCollaborationComposer(
    locale: Locale,
    state: erpx_ui_ai_collaboration::AutocompleteState,
) -> Element {
    rsx! {
        section { class: "finance-collab-composer",
            p { {copy(locale, "Composer — type @ to see cross-department candidates", "กล่องสนทนา — พิมพ์ @ เพื่อดูคำแนะนำข้ามแผนก")} }
            match &state {
                erpx_ui_ai_collaboration::AutocompleteState::Offline => rsx!{ p { "offline" } },
                _ => rsx!{ p { "ready" } },
            }
        }
    }
}
