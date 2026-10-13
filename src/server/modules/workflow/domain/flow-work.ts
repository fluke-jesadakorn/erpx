import type { FlowKind } from "@/lib/shared/flow";
import type { WorkItem } from "@/lib/work_projection/work-item";
export function flowWorkActionKind(flow: import("@/lib/shared/flow").FlowView): "work" | "approval" {
    const key = flow.current?.key;
    return flow.kind === "order" && key === "submitted" || flow.kind === "payroll" && ["in_review", "payment_draft"].includes(key ?? "") ? "approval" : "work";
}
export function workFlowKind(type: WorkItem["type"]): FlowKind {
    return ["service_request", "equipment_request", "purchase_request"].includes(type) ? "request" : type as FlowKind;
}
