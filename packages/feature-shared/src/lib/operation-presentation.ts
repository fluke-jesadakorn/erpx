import type { FlowView } from "@erpx/feature-shared/lib/flow";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";

/** Display data only. Domain services remain responsible for access and commands. */
export type OperationPresentation = {
  status: string;
  responsible: string;
  currentStep: string | null;
  nextAction: string;
  blockedReason: string | null;
  dueAt: number | null;
  recordHref: string | null;
  actionKind: "work" | "approval" | "override";
};

export function operationFromFlow(flow: FlowView, recordHref: string | null = null): OperationPresentation {
  return {
    status: flow.state,
    responsible: flow.current?.primary?.name ?? flow.current?.teamName ?? (flow.current ? "Assignment required" : "No owner needed"),
    currentStep: flow.current?.action ?? null,
    nextAction: flow.current?.action ?? "No further action",
    blockedReason: flow.blocker,
    dueAt: null,
    recordHref,
    actionKind: flow.current && flow.current.role !== "finance_preparer" && /^(department_approval|accounts_approval|awaiting_approval|approval_|purchase_|start_|completion_)/.test(flow.current.key) ? "approval" : "work",
  };
}

export function operationFromWork(item: WorkItem, recordHref: string | null = null): OperationPresentation {
  return {
    status: item.status,
    responsible: item.flow?.current?.primary?.name ?? item.responsibleQueue ?? "Unassigned",
    currentStep: item.flow?.current?.action ?? null,
    nextAction: item.nextAction ?? item.detail,
    blockedReason: item.blocker ?? item.flow?.blocker ?? null,
    dueAt: item.dueAt ?? null,
    recordHref,
    actionKind: item.actionKind ?? (item.relationships?.includes("approval") ? "approval" : "work"),
  };
}
