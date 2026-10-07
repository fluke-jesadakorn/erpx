import type { StaffActor } from '../../operations/public/staff-policy-contract';
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: StaffActor, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: StaffActor, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: {
        companyId: string;
        branchId: string | null;
        departmentId: string | null;
    }): Promise<Result>;
}
