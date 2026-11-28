import type { StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { TemplateRow } from '@erpx/server-operations/domain/routines';
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
export interface RoutinesPorts<Effect> {
    companyAuthority(...args: Tail<Parameters<typeof companyAuthority>>): PortResult<ReturnType<typeof companyAuthority>, Effect>;
}
export function createRoutinesApplication<Effect>(ports: RoutinesPorts<Effect>) {
    async function canEditTemplate(actor: StaffActor, row: TemplateRow) {
        return (row.creator_user_id === actor.userId ||
            (await ports.companyAuthority(actor, ["holding_owner"])));
    }
    return { canEditTemplate };
}
