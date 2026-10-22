import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { staffSettings } from "@erpx/server-operations/public/staff-settings";
import type { ApprovalKind, ApprovalView } from "@erpx/server-workflow/domain/approvals";

export type ApprovalAdministrationOptions = Awaited<ReturnType<typeof staffSettings>>;
export type ApprovalAdministrationRecord = { record_type: ApprovalKind; record_id: string };
export type ApprovalAdministrationRow = ApprovalAdministrationRecord & {
  approval: ApprovalView;
  version: number;
  destination: string;
};
export interface ApprovalAdministrationPorts {
  settings(actor: WorkContext): Promise<ApprovalAdministrationOptions>;
  queue(companyId: string, kind: string | null, record: string | null): Promise<ApprovalAdministrationRecord[]>;
  record(actor: WorkContext, kind: ApprovalKind, id: string): Promise<{ version: unknown }>;
  approval(actor: WorkContext, kind: ApprovalKind, id: string): Promise<ApprovalView>;
  inaccessible(error: unknown): boolean;
}

export function createApprovalAdministrationApplication(ports: ApprovalAdministrationPorts) {
  return {
    async load(actor: WorkContext, kind?: string, record?: string) {
      const options = await ports.settings(actor);
      if (!options.canManage || (actor.beta && kind && kind !== "expense") ||
          (record && !["leave", "expense", "task"].includes(kind ?? ""))) return null;
      const queue = await ports.queue(actor.companyId, kind ?? null, record ?? null);
      const rows: ApprovalAdministrationRow[] = [];
      for (const row of record
        ? [{ record_type: kind as ApprovalKind, record_id: record }]
        : queue.filter(row => !actor.beta || row.record_type === "expense")) {
        try {
          // Administrative authority does not grant access to the underlying record.
          const current = await ports.record(actor, row.record_type, row.record_id);
          const approval = await ports.approval(actor, row.record_type, row.record_id);
          const destination = row.record_type === "leave"
            ? `tasks/people_payroll/${row.record_id}`
            : row.record_type === "expense"
              ? `expenses/${row.record_id}`
              : `staff-tasks/${row.record_id}`;
          rows.push({ ...row, approval, version: Number(current.version), destination });
        } catch (error) {
          if (!ports.inaccessible(error)) throw error;
        }
      }
      return { options, rows };
    },
  };
}
