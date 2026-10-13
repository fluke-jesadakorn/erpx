export const flowKinds = ["expense", "leave", "staff_task", "request", "stock_count", "transfer", "order", "invoice", "journal", "payroll", "record"] as const;
export type FlowKind = (typeof flowKinds)[number];
export type FlowPerson = { userId: string; name: string; teamId: string | null; teamName: string };
export type FlowStep = {
  key: string;
  action: string;
  role: string;
  allowedRoles?: string[];
  teamName: string;
  primary: FlowPerson | null;
  candidates: FlowPerson[];
  status: "current" | "upcoming" | "recorded";
  blocker: string | null;
};
export type FlowView = {
  kind: FlowKind;
  recordId: string;
  recordVersion: number;
  version: number;
  defaultsRevision: number;
  state: string;
  steps: FlowStep[];
  current: FlowStep | null;
  next: FlowStep | null;
  canAct: boolean;
  canManage: boolean;
  blocker: string | null;
  history: { action: string; actorName: string; at: number; reason: string; step: string }[];
};
