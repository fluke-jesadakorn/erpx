export type PayrollPurchaseControls = {newWork: boolean; settleExisting: boolean; history: boolean; revision: string};

/** Presentation hints only. The named Payroll command still authorizes the
 * actual record, current role, source version and purchase at commit. */
export function payrollCommandPurpose(path: string, input?: unknown): "new-work" | "settle-existing" | "history" {
  if (/\/schedule$/.test(path)) return input && typeof input === "object" && "enabled" in input && input.enabled === false ? "settle-existing" : "new-work";
  if (/^(?:payroll\/runs|pay-groups|payroll\/advances|payroll\/thai-profiles|country-packs)$/.test(path) || /\/account-mappings$/.test(path)) return "new-work";
  if (/\/(?:export-review|issue-payslips)$/.test(path)) return "history";
  return "settle-existing";
}
export function payrollActionAllowed(controls: PayrollPurchaseControls | undefined, purpose: ReturnType<typeof payrollCommandPurpose>) {
  return !controls || (purpose === "new-work" ? controls.newWork : purpose === "history" ? controls.history : controls.settleExisting);
}

export function payrollSnapshotMatches<T extends {runId: string; version?: number; runVersion?: number}>(snapshot: T | null | undefined, runId: string, version: number): snapshot is T {
  return !!snapshot && snapshot.runId === runId && (snapshot.version ?? snapshot.runVersion) === version;
}
