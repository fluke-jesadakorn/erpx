//! Bounded renderer-neutral acceptance surface for the Dioxus native renderer.
//!
//! This module deliberately owns no transport, storage, browser, or operating-system API. The
//! launcher can mount it below the normal locale/theme providers on Web and macOS to exercise the
//! same RSX and event path on both renderers.

use std::rc::Rc;

use dioxus::prelude::*;
use erpx_ui_kit::{
    Dialog, DialogSize, Drawer, SelectControl, Skeleton, Toast, ToastTone,
    button::{Button, ButtonSize, ButtonVariant},
    input::Input,
    textarea::Textarea,
};

const ROW_COUNT: usize = 10_000;
const VISIBLE_ROWS: usize = 32;

const fn copy(thai: bool, english: &'static str, thai_text: &'static str) -> &'static str {
    if thai { thai_text } else { english }
}

/// A real interactive surface used by the macOS native launcher and renderer certification.
#[component]
pub fn NativeSpike() -> Element {
    let mut thai = use_signal(|| false);
    let mut dark = use_signal(|| true);
    let mut sidebar_open = use_signal(|| true);
    let mut dialog_open = use_signal(|| false);
    let mut drawer_open = use_signal(|| false);
    let mut toast_visible = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut retry_count = use_signal(|| 0_u32);
    let mut row_start = use_signal(|| 0_usize);
    let mut email = use_signal(String::new);
    let mut department = use_signal(|| "finance".to_owned());
    let mut notes = use_signal(String::new);
    let mut dialog_trigger = use_signal(|| None::<Rc<MountedData>>);

    let is_thai = thai();
    let max_start = ROW_COUNT.saturating_sub(VISIBLE_ROWS);
    let visible_start = row_start().min(max_start);
    let visible_end = (visible_start + VISIBLE_ROWS).min(ROW_COUNT);
    let theme = if dark() { "dark" } else { "light" };
    let dialog_restore_target: ReadSignal<Option<Rc<MountedData>>> = dialog_trigger.into();

    rsx! {
        main {
            class: "native-spike",
            "data-erpx-native-spike": "true",
            "data-theme": theme,
            "data-locale": if is_thai { "th" } else { "en" },
            header { class: "native-spike__topbar",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::IconSm,
                    aria_label: copy(is_thai, "Toggle sidebar", "เปิดหรือปิดแถบข้าง"),
                    onclick: move |_| sidebar_open.toggle(),
                    "☰"
                }
                strong { "ERPX Native" }
                nav { aria_label: copy(is_thai, "Display settings", "การตั้งค่าการแสดงผล"),
                    Button {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        onclick: move |_| thai.toggle(),
                        if is_thai { "English" } else { "ไทย" }
                    }
                    Button {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        onclick: move |_| dark.toggle(),
                        {copy(is_thai, "Light / dark", "สว่าง / มืด")}
                    }
                }
            }

            div { class: "native-spike__shell", "data-sidebar-open": sidebar_open().to_string(),
                aside { class: "native-spike__sidebar", aria_label: copy(is_thai, "Workspace", "พื้นที่ทำงาน"),
                    Button { variant: ButtonVariant::Ghost, "aria-current": "page", {copy(is_thai, "Overview", "ภาพรวม")} }
                    Button { variant: ButtonVariant::Ghost, {copy(is_thai, "Customers", "ลูกค้า")} }
                    Button { variant: ButtonVariant::Ghost, {copy(is_thai, "Finance", "การเงิน")} }
                }

                section { class: "native-spike__content",
                    h1 { {copy(is_thai, "Native renderer acceptance", "ตรวจรับตัวเรนเดอร์เนทีฟ")} }
                    p { {copy(is_thai,
                        "The same typed RSX events run on Web DOM and Blitz/WGPU.",
                        "เหตุการณ์ RSX แบบระบุชนิดชุดเดียวกันทำงานบน Web DOM และ Blitz/WGPU"
                    )} }

                    form {
                        class: "native-spike__form",
                        onsubmit: move |event| {
                            event.prevent_default();
                            loading.set(true);
                            toast_visible.set(true);
                        },
                        label {
                            {copy(is_thai, "Work email", "อีเมลงาน")}
                            Input {
                                r#type: "email",
                                autocomplete: "username",
                                value: email,
                                oninput: move |event: FormEvent| email.set(event.value()),
                            }
                        }
                        label {
                            {copy(is_thai, "Department", "แผนก")}
                            SelectControl {
                                value: department,
                                onchange: move |event: FormEvent| department.set(event.value()),
                                option { value: "finance", {copy(is_thai, "Finance", "การเงิน")} }
                                option { value: "operations", {copy(is_thai, "Operations", "ปฏิบัติการ")} }
                            }
                        }
                        label {
                            {copy(is_thai, "Notes", "หมายเหตุ")}
                            Textarea {
                                value: notes,
                                oninput: move |event: FormEvent| notes.set(event.value()),
                            }
                        }
                        Button { variant: ButtonVariant::Primary, r#type: "submit", {copy(is_thai, "Sign in", "เข้าสู่ระบบ")} }
                    }

                    div { class: "native-spike__actions",
                        Button {
                            variant: ButtonVariant::Secondary,
                            onmounted: move |event: MountedEvent| dialog_trigger.set(Some(event.data())),
                            onclick: move |_| dialog_open.set(true),
                            {copy(is_thai, "Open dialog", "เปิดกล่องโต้ตอบ")}
                        }
                        Button { variant: ButtonVariant::Secondary, onclick: move |_| drawer_open.set(true), {copy(is_thai, "Open drawer", "เปิดแผงข้าง")} }
                        Button {
                            variant: ButtonVariant::Outline,
                            onclick: move |_| {
                                retry_count += 1;
                                loading.set(false);
                            },
                            {copy(is_thai, "Retry", "ลองอีกครั้ง")} " ({retry_count})"
                        }
                    }

                    if loading() {
                        section { aria_live: "polite", Skeleton { lines: 4 } }
                    }

                    section { class: "native-spike__svg", aria_label: copy(is_thai, "SVG rendering", "การแสดงผล SVG"),
                        svg { view_box: "0 0 120 36", role: "img",
                            title { {copy(is_thai, "Three-point trend", "แนวโน้มสามจุด")} }
                            polyline { points: "4,30 44,17 78,23 116,5", fill: "none", stroke: "currentColor", stroke_width: "3" }
                        }
                    }

                    section { class: "native-spike__virtual-table", aria_label: copy(is_thai, "Virtualized ten thousand rows", "ตารางเสมือนหนึ่งหมื่นแถว"),
                        div { class: "native-spike__table-controls",
                            Button {
                                size: ButtonSize::Sm,
                                disabled: visible_start == 0,
                                onclick: move |_| row_start.set(visible_start.saturating_sub(VISIBLE_ROWS)),
                                {copy(is_thai, "Previous rows", "แถวก่อนหน้า")}
                            }
                            output { "{visible_start + 1}–{visible_end} / {ROW_COUNT}" }
                            Button {
                                size: ButtonSize::Sm,
                                disabled: visible_end == ROW_COUNT,
                                onclick: move |_| row_start.set((visible_start + VISIBLE_ROWS).min(max_start)),
                                {copy(is_thai, "Next rows", "แถวถัดไป")}
                            }
                        }
                        div { class: "native-spike__scroll-region", role: "table", tabindex: 0,
                            div { role: "row", strong { role: "columnheader", {copy(is_thai, "Record", "รายการ")} } strong { role: "columnheader", {copy(is_thai, "Status", "สถานะ")} } }
                            for row in visible_start..visible_end {
                                div { role: "row", "aria-rowindex": row + 2,
                                    span { role: "cell", "ERPX-{row:05}" }
                                    span { role: "cell", {copy(is_thai, "Ready", "พร้อม")} }
                                }
                            }
                        }
                    }
                }
            }

            Dialog {
                open: dialog_open(),
                title: copy(is_thai, "Confirm native action", "ยืนยันการทำงานเนทีฟ"),
                size: DialogSize::Sm,
                restore_focus: dialog_restore_target,
                on_close: move |_| dialog_open.set(false),
                p { {copy(is_thai, "Closing restores focus to the typed trigger.", "เมื่อปิด โฟกัสจะกลับไปยังปุ่มเดิม")} }
            }
            Drawer {
                open: drawer_open(),
                title: copy(is_thai, "Record details", "รายละเอียดรายการ"),
                on_close: move |_| drawer_open.set(false),
                p { {copy(is_thai, "Scrollable native drawer content.", "เนื้อหาแผงเนทีฟที่เลื่อนได้")} }
            }
            if toast_visible() {
                Toast {
                    title: copy(is_thai, "Request accepted", "รับคำขอแล้ว"),
                    message: copy(is_thai, "The renderer remains responsive.", "ตัวเรนเดอร์ยังตอบสนองได้"),
                    tone: ToastTone::Success,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_window_is_bounded() {
        assert!(VISIBLE_ROWS < ROW_COUNT);
        assert_eq!(ROW_COUNT, 10_000);
    }
}
