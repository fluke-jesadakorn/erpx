import { workflowRecordPage } from './document-sources';
import type {SqlStatement, SqlDatabase } from "../../runtime/public/runtime/types";
import 'server-only';
import { type FlowKind } from "@/lib/shared/flow";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { companyAuthority } from "@/server/modules/operations/public/staff-policy";
import { requireFlowAccess } from "@/server/modules/workflow/adapters/flow-access";
import { resolveFlow } from "@/server/modules/workflow/adapters/flow-routing";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { createRoutingAttentionApplication, type RoutingAttentionRepository } from '../application/routing-attention';
import { BusinessRuleError } from '../../shared';
export { routingQuerySchema } from '../domain/routing-attention';
function application(provided?: SqlDatabase) { let database = provided!; const repository: RoutingAttentionRepository = { readRoutingAttentionRows(companyId: string, after: string, query: {
        kind: FlowKind;
        after: string;
        limit: number;
    }) { return workflowRecordPage(database, query.kind, companyId, after); } }; return createRoutingAttentionApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => { if (!provided)
        database = (await nativeHoldingResources(holding)).write; }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)), requireFlowAccess: (...args) => fromNative(async () => { await requireFlowAccess(args[0], args[1], args[2], database); }), resolveFlow: (...args) => fromNative(() => resolveFlow(database, ...args)) }); }
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> { try {
    return await run();
}
catch (error) {
    if (error instanceof NativeApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function requireRoutingManager(actor: WorkContext) { const db = (await nativeHoldingResources(actor.holdingId)).write; await invoke(() => application(db).requireRoutingManager(actor)); return db; }
export async function routingAttention(actor: WorkContext, input: unknown) { return invoke(() => application().routingAttention(actor, input)); }
