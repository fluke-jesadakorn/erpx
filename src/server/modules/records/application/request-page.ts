import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { activeStaff } from "@/server/modules/operations/public/staff-policy";
import type { inServiceWorkspace } from "@/server/modules/support/public/service-access";
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { RecordsCommands } from '../domain/commands';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: infer Args) => infer Result ? (...args: Args) => PortResult<Result, Effect> : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface RequestPageRepository {
    readRequestPageOptionsNodesResultcompanySettings(companyId: string, branchId: string | null, branchId2: string | null, branchId3: string | null, departmentId: string | null, departmentId2: string | null): Promise<{
        results: ({
            id: string;
            kind: string;
            display_name: string;
            branch_id: string | null;
        })[];
    }>;
    readRequestPageOptionsNodesResultcompanySettings2(companyId: string): Promise<{
        currency: string;
    } | null>;
}
export interface RequestPagePorts<Effect> {
    repository: RequestPageRepository;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: RecordsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    inServiceWorkspace(...args: Parameters<typeof inServiceWorkspace>): PortResult<ReturnType<typeof inServiceWorkspace>, Effect>;
}
export function createRequestPageApplication<Effect>(ports: RequestPagePorts<Effect>) {
    const r = ports.repository;
    async function requestPageOptions(actor: WorkContext) {
        const [nodesResult, companySettings] = await Promise.all([
            r.readRequestPageOptionsNodesResultcompanySettings(actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId),
            r.readRequestPageOptionsNodesResultcompanySettings2(actor.companyId),
        ]);
        const nodes = nodesResult.results;
        let branchId = actor.branchId, departmentId = actor.departmentId;
        try {
            const staff = await ports.activeStaff(actor.companyId, actor.userId);
            if (ports.inServiceWorkspace(actor, staff)) {
                branchId = staff.branch_id;
                departmentId = staff.department_id;
            }
        }
        catch (e) {
            if (!(e instanceof BusinessRuleError) || e.status !== 409)
                throw e;
        }
        return {
            nodes,
            branchId,
            departmentId,
            companyId: actor.companyId,
            userId: actor.userId,
            assignmentId: actor.assignmentId,
            currency: companySettings?.currency ?? "",
        };
    }
    return { requestPageOptions };
}
