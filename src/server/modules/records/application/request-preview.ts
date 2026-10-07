import type { RequestPreview } from "@/lib/request-preview";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { ApprovalRoute } from "@/server/modules/operations/public/staff-policy";
import { type Clock, type IdentifierSource } from '../../shared';
import type { RecordsCommands } from '../domain/commands';
export interface RequestPreviewRepository {
    readNamedRoutePeople(companyId: string, ids: string[]): Promise<{
        results: ({
            user_id: string;
            legal_name: string;
        })[];
    }>;
}
export interface RequestPreviewPorts<Effect> {
    repository: RequestPreviewRepository;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: RecordsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
}
export function createRequestPreviewApplication<Effect>(ports: RequestPreviewPorts<Effect>) {
    const r = ports.repository;
    async function namedRoute(actor: WorkContext, route: ApprovalRoute): Promise<RequestPreview> {
        const ids = route.steps.map(s => s.userId);
        const people = ids.length ? (await r.readNamedRoutePeople(actor.companyId, ids)).results : [];
        const names = new Map(people.map(p => [p.user_id, p.legal_name]));
        return { steps: route.steps.map(s => ({ label: (s.stage ?? s.authority).replaceAll("_", " "), person: names.get(s.userId) ?? "Eligible reviewer" })), blockers: route.blockedReason ? [route.blockedReason] : [], setupPath: `/profiles/${encodeURIComponent(actor.userId)}` };
    }
    return { namedRoute };
}
