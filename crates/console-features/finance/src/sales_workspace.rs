use dioxus::prelude::*;
use erpx_ui_core::Locale;
use erpx_ui_kit::{Button, ButtonTone, EmptyState, PageHero, SectionCard, StatusBadge, StatusTone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceSalesProjectionState {
    Current,
    Lagging,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceApprovalWorkItem {
    pub key: String,
    pub business_reference: String,
    pub stage: String,
    pub amount: String,
    pub preparer: String,
    pub source_version: u64,
    pub tax_status: String,
    pub accounting_status: String,
    pub anomaly_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesFinanceReconciliationRow {
    pub key: String,
    pub quote_reference: String,
    pub approval_status: String,
    pub order_reference: String,
    pub fulfillment_status: String,
    pub invoice_reference: String,
    pub ar_status: String,
    pub journal_reference: String,
    pub revenue_status: String,
    pub variance: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSalesWorkspaceModel {
    pub scope_label: String,
    pub projection: FinanceSalesProjectionState,
    pub work_items: Vec<FinanceApprovalWorkItem>,
    pub reconciliation: Vec<SalesFinanceReconciliationRow>,
    pub fixture_review_ready: bool,
}

#[component]
pub fn FinanceSalesApprovalWorkspace(
    model: FinanceSalesWorkspaceModel,
    locale: Locale,
    #[props(default)] busy: bool,
    #[props(default)] on_refresh: Option<EventHandler<()>>,
    #[props(default)] on_suggest_review: Option<EventHandler<String>>,
    #[props(default)] on_approve_plan: Option<EventHandler<String>>,
    #[props(default)] on_reject_plan: Option<EventHandler<String>>,
    #[props(default)] on_final_approve: Option<EventHandler<String>>,
) -> Element {
    let (projection_label, projection_tone) = match model.projection {
        FinanceSalesProjectionState::Current => (
            copy(locale, "Projection current", "ข้อมูลเป็นปัจจุบัน"),
            StatusTone::Success,
        ),
        FinanceSalesProjectionState::Lagging => (
            copy(
                locale,
                "Projection lagging · retry available",
                "ข้อมูลล่าช้า · ลองใหม่ได้",
            ),
            StatusTone::Warning,
        ),
        FinanceSalesProjectionState::Failed => (
            copy(
                locale,
                "Worker failed · safe retry available",
                "worker ล้มเหลว · ลองใหม่ได้อย่างปลอดภัย",
            ),
            StatusTone::Danger,
        ),
    };
    rsx! {
        section { class: "erpx-finance-sales-approval-workspace", "data-workspace": "finance-sales-approval",
            PageHero {
                eyebrow: copy(locale, "FINANCE · SECOND CONTROL", "การเงิน · จุดควบคุมรอบสอง").to_owned(),
                title: copy(locale, "Sales accounting and posting approval", "อนุมัติแผนบัญชีและการลงบัญชีจากการขาย").to_owned(),
                description: copy(locale, "Approve the version-pinned accounting plan first, then approve actual billing and fulfillment again before Invoice, AR, GL, or Revenue is posted.", "อนุมัติแผนบัญชีที่ตรึงเวอร์ชันก่อน แล้วอนุมัติยอดวางบิลและส่งมอบจริงอีกครั้งก่อนสร้าง Invoice, AR, GL หรือ Revenue").to_owned(),
                meta: Some(model.scope_label.clone()),
                actions: rsx! { Button { label: copy(locale, "Refresh", "รีเฟรช").to_owned(), tone: ButtonTone::Secondary, disabled: on_refresh.is_none(), on_press: on_refresh } },
            }
            div { class: "erpx-workspace-status-strip", role: "status",
                StatusBadge { label: projection_label.to_owned(), tone: projection_tone }
                StatusBadge { label: "Fixture AI".to_owned(), tone: StatusTone::Info }
                StatusBadge { label: copy(locale, "Suggestion only", "เป็นเพียงคำแนะนำ").to_owned(), tone: StatusTone::Neutral }
                if model.fixture_review_ready {
                    StatusBadge { label: copy(locale, "Review suggestion ready", "คำแนะนำตรวจสอบพร้อมแล้ว").to_owned(), tone: StatusTone::Info }
                } else {
                    StatusBadge { label: copy(locale, "Manual review available", "ตรวจสอบด้วยตนเองได้").to_owned(), tone: StatusTone::Neutral }
                }
                StatusBadge { label: copy(locale, "Not posted", "ยังไม่ลงบัญชี").to_owned(), tone: StatusTone::Warning }
            }
            SectionCard {
                title: copy(locale, "My approval work", "งานอนุมัติของฉัน").to_owned(),
                description: Some(copy(locale, "The preparer cannot approve. The Finance reviewer must differ from the Sales approver. Final posting approval is always explicit.", "ผู้จัดทำอนุมัติเองไม่ได้ ผู้ตรวจการเงินต้องเป็นคนละคนกับผู้อนุมัติฝ่ายขาย และการอนุมัติลงบัญชีขั้นสุดท้ายต้องกดอย่างชัดเจนเสมอ").to_owned()),
                if model.work_items.is_empty() {
                    EmptyState { title: copy(locale, "No Finance approval work", "ไม่มีงานอนุมัติการเงิน").to_owned(), message: copy(locale, "Items appear after version-pinned Sales facts reach Finance through the outbox.", "รายการจะแสดงเมื่อ facts ของ Sales ที่ตรึงเวอร์ชันมาถึง Finance ผ่าน outbox").to_owned() }
                }
                for item in model.work_items {
                    {
                        let key = item.key.clone();
                        let suggest_key = key.clone();
                        let approve_key = key.clone();
                        let reject_key = key.clone();
                        let final_key = key.clone();
                        let suggest_handler = on_suggest_review;
                        let approve_handler = on_approve_plan;
                        let reject_handler = on_reject_plan;
                        let final_handler = on_final_approve;
                        let final_stage = item.stage == "posting_packet";
                        let approval_label = if final_stage {
                            copy(locale, "Final approve and post", "อนุมัติสุดท้ายและลงบัญชี")
                        } else {
                            copy(locale, "Approve accounting plan", "อนุมัติแผนบัญชี")
                        }.to_owned();
                        let approval_disabled = if final_stage {
                            final_handler.is_none()
                        } else {
                            approve_handler.is_none()
                        };
                        rsx! {
                            article { class: "erpx-finance-approval-card", "data-work-key": key,
                                header { h4 { {item.business_reference} } StatusBadge { label: item.stage.clone(), tone: StatusTone::Warning } }
                                dl {
                                    dt { {copy(locale, "Amount", "ยอดเงิน")} } dd { {item.amount} }
                                    dt { {copy(locale, "Prepared by", "จัดทำโดย")} } dd { {item.preparer} }
                                    dt { {copy(locale, "Source version", "เวอร์ชันต้นทาง")} } dd { {item.source_version.to_string()} }
                                    dt { {copy(locale, "Tax profile", "โปรไฟล์ภาษี")} } dd { {item.tax_status} }
                                    dt { {copy(locale, "Accounting", "บัญชี")} } dd { {item.accounting_status} }
                                    dt { {copy(locale, "Anomaly review", "ตรวจความผิดปกติ")} } dd { {item.anomaly_status} }
                                }
                                div { class: "erpx-finance-approval-actions",
                                    Button { label: copy(locale, "Suggest review", "ขอคำแนะนำตรวจสอบ").to_owned(), tone: ButtonTone::Secondary, disabled: suggest_handler.is_none(), on_press: move |_| if let Some(handler) = suggest_handler.as_ref() { handler.call(suggest_key.clone()) } }
                                    Button { label: approval_label, tone: ButtonTone::Primary, busy, disabled: approval_disabled, on_press: move |_| if final_stage { if let Some(handler) = final_handler.as_ref() { handler.call(final_key.clone()) } } else if let Some(handler) = approve_handler.as_ref() { handler.call(approve_key.clone()) } }
                                    Button { label: copy(locale, "Reject / revise", "ปฏิเสธ / ให้แก้ไข").to_owned(), tone: ButtonTone::Danger, disabled: reject_handler.is_none(), on_press: move |_| if let Some(handler) = reject_handler.as_ref() { handler.call(reject_key.clone()) } }
                                }
                                p { class: "erpx-posting-warning", role: "status", {copy(locale, "No posting occurs until final Finance approval passes open-period, active-account, balance, tax-drift, and plan-actual checks.", "จะไม่ลงบัญชีจนกว่าการอนุมัติสุดท้ายจะผ่านการตรวจงวดเปิด บัญชีใช้งาน สมดุลรายการ ภาษีเปลี่ยน และยอดจริงตรงกับแผน")} }
                            }
                        }
                    }
                }
            }
            SectionCard {
                title: copy(locale, "Quote-to-revenue reconciliation", "กระทบยอดจากใบเสนอราคาถึงรายได้").to_owned(),
                description: Some(copy(locale, "Every row preserves source backlinks and makes amount differences visible.", "ทุกรายการเก็บลิงก์กลับต้นทางและแสดงผลต่างของยอดอย่างชัดเจน").to_owned()),
                if model.reconciliation.is_empty() {
                    EmptyState { title: copy(locale, "No completed chain yet", "ยังไม่มีสายงานที่เสร็จ").to_owned(), message: copy(locale, "Approved facts will link Quote → approvals → Order → fulfillment → Invoice → AR → Journals → Revenue.", "facts ที่อนุมัติแล้วจะเชื่อม Quote → approvals → Order → fulfillment → Invoice → AR → Journals → Revenue").to_owned() }
                }
                for row in model.reconciliation {
                    article { class: "erpx-reconciliation-row", "data-reconciliation-key": row.key,
                        header { h4 { {row.quote_reference} } StatusBadge { label: row.variance.clone(), tone: if row.variance == "0.00" { StatusTone::Success } else { StatusTone::Danger } } }
                        ol {
                            li { {row.approval_status} }
                            li { {row.order_reference} }
                            li { {row.fulfillment_status} }
                            li { {row.invoice_reference} }
                            li { {row.ar_status} }
                            li { {row.journal_reference} }
                            li { {row.revenue_status} }
                        }
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
    fn posting_surface_is_explicit_and_business_facing() {
        let source = include_str!("sales_workspace.rs");
        assert!(!source.contains(concat!("U", "UID")));
        assert!(source.contains("Final approve and post"));
        assert!(source.contains("Not posted"));
        assert!(source.contains("Fixture AI"));
    }
}
