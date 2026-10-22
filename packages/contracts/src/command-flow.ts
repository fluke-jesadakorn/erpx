/** Owner projections identify a workflow; the composition executes its typed port. */
export type OwnerCommandFlowTarget =
  | Readonly<{ kind: "order"; id: string; action: "invoice" }>
  | Readonly<{ kind: "expense"; id: string; action: "payment" }>;
