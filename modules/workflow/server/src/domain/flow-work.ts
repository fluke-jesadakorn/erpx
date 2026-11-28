import type { FlowKind } from "@erpx/platform-shared-ui/lib/flow";
import type { WorkItem } from "@erpx/work-projection-ui/lib/work-item";
export function flowWorkActionKind(flow: import("@erpx/platform-shared-ui/lib/flow").FlowView): "work" | "approval" {
    const key = flow.current?.key;
    return flow.kind === "order" && key === "submitted" || flow.kind === "payroll" && ["in_review", "payment_draft"].includes(key ?? "") ? "approval" : "work";
}
export function workFlowKind(type: WorkItem["type"]): FlowKind {
    return ["service_request", "equipment_request", "purchase_request"].includes(type) ? "request" : type as FlowKind;
}
