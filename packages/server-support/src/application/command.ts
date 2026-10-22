import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
export type { SupportRow } from "@erpx/server-support/domain/support-facts";
export type SupportMutation = {
    readonly native: unknown;
};
export type SupportCommand = {
    actor: WorkContext;
    key: string;
    operation: string;
    id: string;
    input: unknown;
    version: number;
    result: Record<string, unknown>;
    now: number;
    scope?: Pick<WorkContext, "companyId" | "branchId" | "departmentId">;
};
export interface SupportCommandPorts {
    authorize(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    now(): number;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string, unknown> | null>;
    commit(command: SupportCommand, mutation: SupportMutation): Promise<Record<string, unknown>>;
}
