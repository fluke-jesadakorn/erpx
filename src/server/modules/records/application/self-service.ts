import { isMyAction } from "@/lib/work-item";
import { selfServiceActions } from "@/lib/self-service";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { activeStaff } from "@/server/modules/operations/public/staff-policy";
import type { leaveSummary } from "@/server/modules/people_payroll/public/leave-self-service";
import type { overtimeSetup } from "@/server/modules/people_payroll/public/overtime";
import type { workSnapshot } from "@/server/modules/work_projection/public/work-items";
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
export interface SelfServiceRepository {
    readSelfServiceSummarySources(companyId: string, engagement_id: string, now: number): Promise<{
        id: string;
        name: string;
        local_date: string;
        start_time: string;
        end_time: string;
        starts_at: number;
        ends_at: number;
        timezone: string;
    } | null>;
}
export interface SelfServicePorts<Effect> {
    repository: SelfServiceRepository;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: RecordsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    leaveSummary(...args: Parameters<typeof leaveSummary>): PortResult<ReturnType<typeof leaveSummary>, Effect>;
    overtimeSetup(...args: Tail<Parameters<typeof overtimeSetup>>): PortResult<ReturnType<typeof overtimeSetup>, Effect>;
    workSnapshot(...args: Parameters<typeof workSnapshot>): PortResult<ReturnType<typeof workSnapshot>, Effect>;
}
export function createSelfServiceApplication<Effect>(ports: SelfServicePorts<Effect>) {
    const r = ports.repository;
    async function selfServiceSummary(actor: WorkContext, onWorkCount?: (count: number | null) => void) {
        await ports.prepare(actor.holdingId);
        const sources = await Promise.allSettled([
            (async () => { const staff = await ports.activeStaff(actor.companyId, actor.userId); return { rosterRequired: staff.roster_required === 1, nextShift: staff.roster_required === 1 ? await r.readSelfServiceSummarySources(actor.companyId, staff.engagement_id, ports.clock.now()) : null }; })(),
            ports.leaveSummary(actor), ports.overtimeSetup(actor), ports.workSnapshot(actor),
        ]);
        const [shift, leave, overtime, work] = sources;
        onWorkCount?.(work.status === 'fulfilled' && !work.value.unavailableSources.length ? work.value.items.filter(isMyAction).length : null);
        const staffSetupRequired = [shift, overtime].some(s => s.status === 'rejected' && s.reason instanceof BusinessRuleError && s.reason.code === 'STAFF_LINK_REQUIRED');
        const unavailable = sources.flatMap((s, i) => s.status === 'rejected' && !(s.reason instanceof BusinessRuleError && s.reason.code === 'STAFF_LINK_REQUIRED') ? [["Schedule", "Leave", "Overtime", "Work"][i]] : []);
        const setup = sources.flatMap(s => s.status === 'rejected' && s.reason instanceof BusinessRuleError && s.reason.code !== 'STAFF_LINK_REQUIRED' ? [s.reason.message] : []);
        if (staffSetupRequired)
            setup.push('Connect this account to one active employee or contractor profile to unlock personal schedule, leave, and overtime.');
        if (leave.status === 'fulfilled' && leave.value.setupMessage && !staffSetupRequired)
            setup.push(leave.value.setupMessage);
        if (overtime.status === 'fulfilled')
            setup.push(...overtime.value.blockers);
        return { asOf: ports.clock.now(), companyId: actor.companyId, actions: selfServiceActions.filter(action => action.id !== 'shifts' || shift.status === 'fulfilled' && shift.value.rosterRequired), rosterRequired: shift.status === 'fulfilled' ? shift.value.rosterRequired : true, staffSetupRequired, setupBlockers: [...new Set(setup)], unavailableSources: [...new Set([...unavailable, ...(work.status === 'fulfilled' ? work.value.unavailableSources : [])])],
            nextShift: shift.status === 'fulfilled' ? shift.value.nextShift : null,
            attention: work.status === 'fulfilled' ? work.value.items.filter(isMyAction).slice(0, 3) : null,
            recentRequests: work.status === 'fulfilled' ? work.value.items.filter(i => i.relationships?.includes('requested') && i.queue !== 'completed').sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 3) : null,
            leave: leave.status === 'fulfilled' ? { setupRequired: leave.value.setupRequired, policy: leave.value.policy } : null,
        };
    }
    async function selfServiceSeed(actor: WorkContext): Promise<{
        data: Awaited<ReturnType<typeof selfServiceSummary>>;
        workCount: number | null;
    }> {
        let workCount: number | null = null;
        const data = await selfServiceSummary(actor, count => { workCount = count; });
        return { data, workCount };
    }
    return { selfServiceSummary, selfServiceSeed };
}
