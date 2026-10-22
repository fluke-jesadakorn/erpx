import type { FlowKind } from "@erpx/feature-shared/lib/flow";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
export function flowWorkActionKind(flow: import("@erpx/feature-shared/lib/flow").FlowView): "work" | "approval" {
    const key = flow.current?.key;
    return flow.kind === "order" && key === "submitted" || flow.kind === "payroll" && ["in_review", "payment_draft"].includes(key ?? "") ? "approval" : "work";
}
export function workFlowKind(type: WorkItem["type"]): FlowKind {
    return ["service_request", "equipment_request", "purchase_request"].includes(type) ? "request" : type as FlowKind;
}
