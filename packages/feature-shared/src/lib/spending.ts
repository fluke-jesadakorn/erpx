export type SpendingKind = "expense" | "purchase" | "payment";
export type SpendingException = {
  positionId: string | null;
  requestKind: SpendingKind | null;
  action: "skip_supervisor" | "named_supervisor";
  supervisorUserId: string | null;
  reason: string;
};
export type SpendingTemplate = { version: number; exceptions: SpendingException[]; reason: string };
