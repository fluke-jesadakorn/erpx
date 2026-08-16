use dioxus::prelude::*;
use erpx_ui_ai_collaboration::{
    ActivityRailState, ActivityReferenceGate, AiWorkspaceState, AnchorFreshness, AutocompleteState,
    CommentCard, CommentEditorDocument, CommentVisibility, ConnectivityState,
    FloatingLauncherState, LAUNCHER_ACCESSIBILITY, LauncherPresentation, MutationAvailability,
    PrivacyState, RealtimeState, ThreadCard, ThreadLifecycle, ThreadRealm, TypedActionProposal,
    WorkspaceSection, activity_accessibility,
};
use erpx_ui_core::Locale;
use erpx_ui_kit::{
    Button, ButtonTone, Disclosure, EmptyState, Skeleton, StatusBadge, StatusTone,
    kit::{TextArea, button::Button as KitButton},
};

pub const AI_COLLABORATION_STYLES: &str = include_str!("../assets/ai-collaboration.css");

const fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

#[component]
pub fn RecordActivityRail(
    state: ActivityRailState,
    locale: Locale,
    #[props(default)] reduced_motion: bool,
    #[props(default)] on_retry: Option<EventHandler<()>>,
) -> Element {
    let accessibility = activity_accessibility(reduced_motion);
    rsx! {
        aside {
            class: "erpx-activity-rail",
            role: accessibility.role,
            aria_label: copy(locale, "Record activity and discussion", "กิจกรรมและการสนทนาของรายการ"),
            match state {
                ActivityRailState::Loading => rsx! { Skeleton { lines: 4 } },
                ActivityRailState::Empty => rsx! {
                    EmptyState {
                        title: copy(locale, "No discussion yet", "ยังไม่มีการสนทนา").to_owned(),
                        message: copy(locale, "Start a record-anchored thread when input is needed.", "เริ่มเธรดที่ผูกกับรายการเมื่อต้องการข้อมูล").to_owned(),
                    }
                },
                ActivityRailState::Denied => state_notice(locale, "denied", copy(locale, "You do not have access to this discussion.", "คุณไม่มีสิทธิ์เข้าถึงการสนทนานี้").to_owned()),
                ActivityRailState::Restricted(card) => rsx! {
                    div { class: "erpx-restricted", role: "status", {card.neutral_label} }
                },
                ActivityRailState::Offline { cached_threads } => rsx! {
                    {state_notice(locale, "offline", copy(locale, "Offline: comments, mentions, references, and AI actions are disabled. Changes are not queued.", "ออฟไลน์: ปิดการแสดงความคิดเห็น การเมนชัน การอ้างอิง และ AI ระบบจะไม่เข้าคิวการเปลี่ยนแปลง").to_owned())}
                    for thread in cached_threads { {render_thread(thread, locale)} }
                },
                ActivityRailState::Stale { threads, reason } => rsx! {
                    {state_notice(locale, "stale", reason)}
                    for thread in threads { {render_thread(thread, locale)} }
                },
                ActivityRailState::Degraded { threads, reason, retryable } => rsx! {
                    {state_notice(locale, "degraded", reason)}
                    if retryable {
                        Button { label: copy(locale, "Retry", "ลองใหม่").to_owned(), tone: ButtonTone::Secondary, on_press: on_retry }
                    }
                    for thread in threads { {render_thread(thread, locale)} }
                },
                ActivityRailState::Content { threads, realtime } => rsx! {
                    {realtime_status(realtime, locale)}
                    for thread in threads { {render_thread(thread, locale)} }
                },
            }
        }
    }
}

/// Fail-closed activity-rail mount. A canonical URL, route parameter, cached
/// descriptor, or failed resolver response can never create an activity
/// request. The rail appears only after the Reference service returned an
/// authorized, current descriptor for the exact canonical route.
#[component]
pub fn AuthorizedRecordActivityRail(
    gate: ActivityReferenceGate,
    current_route: String,
    state: ActivityRailState,
    locale: Locale,
    #[props(default)] reduced_motion: bool,
    #[props(default)] on_retry: Option<EventHandler<()>>,
) -> Element {
    if gate.authorized_record(&current_route).is_some() {
        return rsx! {
            RecordActivityRail { state, locale, reduced_motion, on_retry }
        };
    }
    match gate {
        ActivityReferenceGate::NotRecordRoute | ActivityReferenceGate::Neutral => rsx! {},
        ActivityReferenceGate::Resolving => rsx! {
            div { class: "erpx-activity-gate", role: "status",
                {copy(locale, "Authorizing record activity…", "กำลังตรวจสอบสิทธิ์กิจกรรมของรายการ…")}
            }
        },
        ActivityReferenceGate::Stale => state_notice(
            locale,
            "stale",
            copy(
                locale,
                "Record activity is hidden until the directory projection is current.",
                "ซ่อนกิจกรรมของรายการจนกว่าไดเรกทอรีโพรเจกชันจะเป็นปัจจุบัน",
            )
            .to_owned(),
        ),
        ActivityReferenceGate::Degraded => state_notice(
            locale,
            "degraded",
            copy(
                locale,
                "Record activity is unavailable; no cached descriptor was used.",
                "กิจกรรมของรายการไม่พร้อมใช้งาน และไม่ได้ใช้ตัวอธิบายจากแคช",
            )
            .to_owned(),
        ),
        ActivityReferenceGate::Offline => state_notice(
            locale,
            "offline",
            copy(
                locale,
                "Record activity requires an online authorization check.",
                "กิจกรรมของรายการต้องตรวจสอบสิทธิ์ขณะออนไลน์",
            )
            .to_owned(),
        ),
        ActivityReferenceGate::Authorized { .. } => rsx! {},
    }
}

fn state_notice(locale: Locale, kind: &'static str, message: String) -> Element {
    rsx! {
        div {
            class: "erpx-state-notice",
            role: if kind == "denied" { "alert" } else { "status" },
            aria_live: if kind == "denied" { "assertive" } else { "polite" },
            "data-state": kind,
            strong { {copy(locale, "Status", "สถานะ")} }
            p { {message} }
        }
    }
}

fn realtime_status(state: RealtimeState, locale: Locale) -> Element {
    let tone = match state {
        RealtimeState::Connected => StatusTone::Success,
        RealtimeState::Reconnecting | RealtimeState::Degraded => StatusTone::Warning,
        RealtimeState::Unavailable => StatusTone::Danger,
    };
    rsx! {
        div { class: "erpx-live-status", role: "status", aria_live: "polite",
            StatusBadge { label: state.announcement(locale).to_owned(), tone }
        }
    }
}

fn render_thread(thread: ThreadCard, locale: Locale) -> Element {
    let realm = match &thread.realm {
        ThreadRealm::Internal => copy(locale, "Internal discussion", "การสนทนาภายใน").to_owned(),
        ThreadRealm::ExternalPortal { portal_label } => format!(
            "{} · {portal_label}",
            copy(locale, "External shared thread", "เธรดที่แชร์ภายนอก")
        ),
    };
    let freshness = match thread.anchor.freshness {
        AnchorFreshness::Current => None,
        AnchorFreshness::Stale => Some(copy(
            locale,
            "Anchor is stale; verify the source revision.",
            "จุดอ้างอิงล้าสมัย โปรดตรวจสอบรุ่นต้นทาง",
        )),
        AnchorFreshness::Orphaned => Some(copy(
            locale,
            "Anchor no longer resolves; it was not moved silently.",
            "ไม่พบจุดอ้างอิงและระบบไม่ได้ย้ายโดยอัตโนมัติ",
        )),
    };
    let transition_label = if thread.lifecycle == ThreadLifecycle::Resolved {
        copy(locale, "Reopen", "เปิดอีกครั้ง")
    } else {
        copy(locale, "Resolve", "ทำเครื่องหมายว่าแก้ไขแล้ว")
    };
    let transition_disabled = if thread.lifecycle == ThreadLifecycle::Resolved {
        !thread.capabilities.reopen
    } else {
        !thread.capabilities.resolve
    };
    rsx! {
        article {
            class: "erpx-thread-card",
            "data-thread-state": if thread.lifecycle == ThreadLifecycle::Resolved { "resolved" } else { "open" },
            header {
                div { class: "erpx-thread-meta",
                    StatusBadge { label: realm, tone: StatusTone::Info }
                    if thread.pinned { StatusBadge { label: copy(locale, "Pinned", "ปักหมุด").to_owned(), tone: StatusTone::Neutral } }
                    if thread.following { StatusBadge { label: copy(locale, "Following", "กำลังติดตาม").to_owned(), tone: StatusTone::Success } }
                }
                if let Some(message) = freshness { p { role: "status", {message} } }
            }
            for comment in thread.comments { {render_comment(comment, locale)} }
            footer { class: "erpx-comment-actions",
                Button { label: transition_label.to_owned(), disabled: transition_disabled }
                Button { label: copy(locale, "Follow", "ติดตาม").to_owned(), disabled: !thread.capabilities.follow }
                Button { label: copy(locale, "Promote to work item", "สร้างเป็นรายการงาน").to_owned(), disabled: !thread.capabilities.promote }
            }
        }
    }
}

fn render_comment(comment: CommentCard, locale: Locale) -> Element {
    let status = match comment.visibility {
        CommentVisibility::Visible => None,
        CommentVisibility::Tombstone => Some(copy(locale, "Comment deleted", "ความคิดเห็นถูกลบ")),
        CommentVisibility::Quarantined => Some(copy(
            locale,
            "Comment is under moderation",
            "ความคิดเห็นอยู่ระหว่างการตรวจสอบ",
        )),
        CommentVisibility::Redacted => Some(copy(
            locale,
            "Comment redacted by policy",
            "ความคิดเห็นถูกปกปิดตามนโยบาย",
        )),
    };
    rsx! {
        article { class: "erpx-comment-card",
            header {
                strong { {comment.author_label} }
                if comment.author_is_agent { StatusBadge { label: copy(locale, "AI agent", "เอเจนต์ AI").to_owned(), tone: StatusTone::Info } }
                if let Some(publisher) = comment.published_by_label {
                    small { {format!("{} {publisher}", copy(locale, "Published by", "เผยแพร่โดย"))} }
                }
            }
            if let Some(message) = status {
                p { class: "erpx-comment-tombstone", role: "status", {message} }
            } else if let Some(document) = comment.document {
                p { {document.accessible_plain_text} }
            }
            footer { class: "erpx-comment-actions",
                for reaction in comment.reactions {
                    KitButton { r#type: "button", aria_pressed: reaction.reacted_by_current_user.to_string(), "{reaction.emoji} {reaction.count}" }
                }
                Disclosure {
                    summary { {copy(locale, "Revision history", "ประวัติการแก้ไข")} }
                    ul { for revision in comment.revisions { li { "{revision.revision} · {revision.editor_label}" } } }
                }
            }
        }
    }
}

#[component]
pub fn CommentEditorSurface(
    document: CommentEditorDocument,
    autocomplete: AutocompleteState,
    availability: MutationAvailability,
    locale: Locale,
    #[props(default)] submitting: bool,
) -> Element {
    let disabled = !availability.enabled || submitting;
    rsx! {
        section { class: "erpx-comment-editor", aria_label: copy(locale, "Comment editor", "ตัวแก้ไขความคิดเห็น"),
            div { class: "erpx-editor-toolbar", role: "toolbar", aria_label: copy(locale, "Formatting and typed references", "การจัดรูปแบบและการอ้างอิงแบบระบุชนิด"),
                KitButton { r#type: "button", disabled, aria_label: copy(locale, "Mention a person, group, or agent", "เมนชันบุคคล กลุ่ม หรือเอเจนต์"), "@" }
                KitButton { r#type: "button", disabled, aria_label: copy(locale, "Reference an authorized record", "อ้างอิงรายการที่ได้รับอนุญาต"), "#" }
                KitButton { r#type: "button", disabled, {copy(locale, "Attach", "แนบไฟล์")} }
            }
            TextArea { value: document.accessible_plain_text, disabled, aria_label: copy(locale, "Accessible comment text", "ข้อความความคิดเห็นสำหรับการช่วยการเข้าถึง").to_owned() }
            {render_autocomplete(autocomplete, locale)}
            if !availability.enabled {
                p { class: "erpx-offline-lock", role: "status", {copy(locale, "Editing is unavailable in the current authorization, anchor, realm, or connectivity state.", "ไม่สามารถแก้ไขได้เนื่องจากสิทธิ์ จุดอ้างอิง ขอบเขต หรือสถานะการเชื่อมต่อปัจจุบัน")} }
            }
            Button { label: copy(locale, "Post comment", "ส่งความคิดเห็น").to_owned(), tone: ButtonTone::Primary, disabled, busy: submitting }
        }
    }
}

fn render_autocomplete(state: AutocompleteState, locale: Locale) -> Element {
    rsx! {
        div { class: "erpx-autocomplete", role: "listbox", aria_label: copy(locale, "Mention and reference suggestions", "คำแนะนำเมนชันและการอ้างอิง"),
            match state {
                AutocompleteState::Idle => rsx! {},
                AutocompleteState::Searching { .. } => rsx! { p { role: "status", {copy(locale, "Searching…", "กำลังค้นหา…")} } },
                AutocompleteState::Results { candidates, .. } => rsx! { for candidate in candidates { KitButton { r#type: "button", role: "option", disabled: candidate.restricted, "{candidate.display_label}" } } },
                AutocompleteState::Empty { .. } => rsx! { p { {copy(locale, "No authorized matches", "ไม่พบรายการที่ได้รับอนุญาต")} } },
                AutocompleteState::Denied => rsx! { p { {copy(locale, "Suggestions unavailable", "ไม่สามารถแสดงคำแนะนำ")} } },
                AutocompleteState::Offline => rsx! { p { {copy(locale, "Suggestions require an online connection", "คำแนะนำต้องเชื่อมต่อออนไลน์")} } },
                AutocompleteState::Failed { retryable } => rsx! { p { {if retryable { copy(locale, "Suggestions failed; retry", "คำแนะนำล้มเหลว โปรดลองใหม่") } else { copy(locale, "Suggestions unavailable", "คำแนะนำไม่พร้อมใช้งาน") }} } },
            }
        }
    }
}

#[component]
pub fn AiWorkspaceView(state: AiWorkspaceState, locale: Locale) -> Element {
    rsx! {
        main { class: "erpx-ai-workspace",
            header {
                h1 { {copy(locale, "AI & Collaboration", "AI และการทำงานร่วมกัน")} }
                nav { class: "erpx-ai-tabs", aria_label: copy(locale, "AI workspace sections", "ส่วนพื้นที่ทำงาน AI"),
                    for section in WorkspaceSection::ALL { KitButton { r#type: "button", {section.label(locale)} } }
                }
            }
            {render_workspace_state(state, locale)}
        }
    }
}

fn render_workspace_state(state: AiWorkspaceState, locale: Locale) -> Element {
    match state {
        AiWorkspaceState::Loading => rsx! { Skeleton { lines: 6 } },
        AiWorkspaceState::Empty { section } => {
            rsx! { EmptyState { title: section.label(locale).to_owned(), message: copy(locale, "No authorized items in this section.", "ไม่มีรายการที่ได้รับอนุญาตในส่วนนี้").to_owned() } }
        }
        AiWorkspaceState::Denied => state_notice(
            locale,
            "denied",
            copy(
                locale,
                "You do not have access to the AI workspace.",
                "คุณไม่มีสิทธิ์เข้าถึงพื้นที่ทำงาน AI",
            )
            .to_owned(),
        ),
        AiWorkspaceState::Restricted => {
            rsx! { p { class: "erpx-restricted", role: "status", "Restricted item" } }
        }
        AiWorkspaceState::Stale { reason, .. } => state_notice(locale, "stale", reason),
        AiWorkspaceState::Degraded { reason, .. } => state_notice(locale, "degraded", reason),
        AiWorkspaceState::Offline { .. } => state_notice(
            locale,
            "offline",
            copy(
                locale,
                "Offline: workspace data is read-only and AI invocation is disabled.",
                "ออฟไลน์: ข้อมูลดูได้อย่างเดียวและปิดการเรียกใช้ AI",
            )
            .to_owned(),
        ),
        AiWorkspaceState::Content {
            assignments,
            registry,
            pools,
            budgets,
            provenance,
            ..
        } => rsx! {
            section { aria_label: copy(locale, "Agent assignments", "งานของเอเจนต์"),
                for item in assignments { article { h3 { {item.agent_label} } StatusBadge { label: format!("{:?}", item.assignment_state), tone: StatusTone::Info } small { {format!("v{}", item.pinned_agent_version)} } } }
            }
            section { aria_label: copy(locale, "Agent registry", "ทะเบียนเอเจนต์"), for agent in registry { article { strong { {agent.label} } small { {format!("v{}", agent.version)} } } } }
            section { aria_label: copy(locale, "Agent pools", "กลุ่มเอเจนต์"), for pool in pools { p { "{pool.label} · {pool.routing_summary}" } } }
            section { aria_label: copy(locale, "Budgets", "งบประมาณ"), for budget in budgets { p { "{budget.layer}: {budget.used_minor} / {budget.limit_minor} {budget.currency}" } } }
            section { aria_label: copy(locale, "Provenance", "ที่มาและหลักฐาน"), for row in provenance { p { "{row.initiating_principal_label} · {row.effective_agent_label} v{row.agent_version} · {row.model_label}" } } }
        },
    }
}

#[component]
pub fn FloatingAiLauncher(
    state: FloatingLauncherState,
    locale: Locale,
    #[props(default)] on_open: Option<EventHandler<()>>,
    #[props(default)] on_close: Option<EventHandler<()>>,
    #[props(default)] on_confirm_context: Option<EventHandler<()>>,
) -> Element {
    if state.presentation == LauncherPresentation::Hidden {
        return rsx! {};
    }
    let total = state.badge.clone().total();
    if !state.preference.open {
        return rsx! {
            div { class: "erpx-ai-launcher",
                KitButton { r#type: "button", aria_label: LAUNCHER_ACCESSIBILITY.trigger_label,
                    onclick: move |_| if let Some(handler) = on_open.as_ref() { handler.call(()) },
                    "AI"
                    if total > 0 { span { class: "erpx-ai-badge", role: "status", aria_live: LAUNCHER_ACCESSIBILITY.badge_live, "{total}" } }
                }
            }
        };
    }
    let presentation = if state.presentation == LauncherPresentation::DockedResizablePanel {
        "docked"
    } else {
        "bottom-sheet"
    };
    rsx! {
        div { class: "erpx-ai-launcher",
            aside { class: "erpx-ai-panel", role: LAUNCHER_ACCESSIBILITY.dialog_role, aria_modal: "false", aria_label: copy(locale, "AI assistants", "ผู้ช่วย AI"), "data-presentation": presentation, style: "--ai-panel-width: {state.preference.panel_width}px",
                header { h2 { {copy(locale, "AI assistants", "ผู้ช่วย AI")} } KitButton { r#type: "button", aria_label: copy(locale, "Close AI panel", "ปิดแผง AI"), onclick: move |_| if let Some(handler) = on_close.as_ref() { handler.call(()) }, "×" } }
                if state.connectivity == ConnectivityState::Offline { p { class: "erpx-offline-lock", role: "status", {copy(locale, "Offline: AI invocation is disabled and will not be queued.", "ออฟไลน์: ปิดการเรียกใช้ AI และระบบจะไม่เข้าคิว")} } }
                if let Some(session) = state.active_session {
                    article { h3 { {session.agent_label} } p { {format!("{} {}", copy(locale, "Pinned version", "รุ่นที่ตรึง"), session.pinned_agent_version)} } StatusBadge { label: match session.privacy { PrivacyState::Private => copy(locale, "Private", "ส่วนตัว"), PrivacyState::Published => copy(locale, "Published", "เผยแพร่แล้ว") }.to_owned(), tone: StatusTone::Info } p { {format!("{:?}", session.assignment_state)} } }
                }
                if state.context_change_pending.is_some() {
                    div { role: "alertdialog", aria_label: copy(locale, "Confirm context boundary", "ยืนยันขอบเขตบริบท"),
                        p { {copy(locale, "This record will start a new context. The current session will not be rebound silently.", "รายการนี้จะเริ่มบริบทใหม่ ระบบจะไม่เปลี่ยนบริบทของเซสชันปัจจุบันโดยอัตโนมัติ")} }
                        Button { label: copy(locale, "Start new context", "เริ่มบริบทใหม่").to_owned(), tone: ButtonTone::Primary, on_press: on_confirm_context }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TypedActionCard(
    proposal: TypedActionProposal,
    connectivity: ConnectivityState,
    locale: Locale,
    #[props(default)] on_confirm: Option<EventHandler<()>>,
) -> Element {
    let disabled =
        connectivity == ConnectivityState::Offline || !proposal.validate() || proposal.confirmed;
    rsx! {
        article { class: "erpx-action-card",
            h3 { {proposal.summary} }
            p { "{proposal.owner_context} · {proposal.command_type}" }
            p { {copy(locale, "This proposal has no business effect until an authorized owner-context command is confirmed.", "ข้อเสนอนี้ไม่มีผลต่อธุรกิจจนกว่าจะยืนยันคำสั่งของโดเมนเจ้าของโดยผู้มีอำนาจ")} }
            Button { label: copy(locale, "Review and confirm", "ตรวจสอบและยืนยัน").to_owned(), tone: ButtonTone::Primary, disabled, on_press: on_confirm }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn css_defines_docked_resizable_mobile_sheet_and_reduced_motion() {
        assert!(AI_COLLABORATION_STYLES.contains("position: fixed"));
        assert!(AI_COLLABORATION_STYLES.contains("resize: horizontal"));
        assert!(AI_COLLABORATION_STYLES.contains("bottom-sheet"));
        assert!(AI_COLLABORATION_STYLES.contains("prefers-reduced-motion"));
    }
    #[test]
    fn rendering_source_has_no_focus_stealing_or_implicit_mutation() {
        let source = include_str!("view.rs");
        assert!(!source.contains(concat!("auto", "focus")));
        assert!(!source.contains(concat!("Natural", "Language")));
        assert!(source.contains("TypedActionCard"));
        assert!(source.contains("Offline: AI invocation is disabled"));
    }

    #[test]
    fn activity_rail_is_guarded_by_an_authorized_canonical_reference() {
        let source = include_str!("view.rs");
        let gate = source
            .find("gate.authorized_record(&current_route).is_some()")
            .expect("authorized reference gate");
        let rail = source[gate..]
            .find("RecordActivityRail { state")
            .expect("guarded activity rail mount");
        assert!(rail > 0);
        assert!(!source.contains(concat!("StableRecordRef::", "try_new")));
    }
}
