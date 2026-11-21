import type { StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { TemplateRow } from '@erpx/server-operations/domain/routines';
import { BusinessRuleError } from '@erpx/server-shared/index';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: never[]) => unknown ? Value : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface RoutinesRepository<Effect> {
    readListTemplatesEffect(companyId: string, userId: string, value: 0 | 1): Promise<{
        results: (TemplateRow)[];
    }>;
    readGetTemplateRow(id: string, companyId: string): Promise<TemplateRow | null>;
}
export interface RoutinesPorts<Effect> {
    repository: RoutinesRepository<Effect>;
    companyAuthority(...args: Tail<Parameters<typeof companyAuthority>>): PortResult<ReturnType<typeof companyAuthority>, Effect>;
}
export function createRoutinesApplication<Effect>(ports: RoutinesPorts<Effect>) {
    const r = ports.repository;
    async function listTemplates(actor: StaffActor) {
        const owner = await ports.companyAuthority(actor, ["holding_owner"]);
        return (await r.readListTemplatesEffect(actor.companyId, actor.userId, owner ? 1 : 0)).results;
    }
    async function getTemplate(actor: StaffActor, id: string) {
        const row = await r.readGetTemplateRow(id, actor.companyId);
        if (!row ||
            (!row.published &&
                row.creator_user_id !== actor.userId &&
                !(await ports.companyAuthority(actor, ["holding_owner"]))))
            throw new BusinessRuleError(404, "TEMPLATE_NOT_FOUND", "Routine template not found.");
        return row;
    }
    async function canEditTemplate(actor: StaffActor, row: TemplateRow) {
        return (row.creator_user_id === actor.userId ||
            (await ports.companyAuthority(actor, ["holding_owner"])));
    }
    return { listTemplates, getTemplate, canEditTemplate };
}
