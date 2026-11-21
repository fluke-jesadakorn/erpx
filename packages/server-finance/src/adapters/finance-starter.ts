import { withFinanceConfigurationCommand } from "@erpx/server-finance/adapters/purchase-boundary";
import { nativeFinanceStarter } from '@erpx/server-collaboration/adapters/native-finance-starter';
import 'server-only';
import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import { financeSetupCandidates, financeSetupIndependent } from '@erpx/server-identity_access/public/finance-setup-sources';
import { lockFinanceStarterCompany, financeStarterOrganizationNodes } from '@erpx/server-organization/public/finance-starter-sources';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { financeTransaction } from '@erpx/server-finance/public/finance-transactions';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import { requireFinanceMoneyScale } from '@erpx/server-finance/public/finance-money-policy';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { createFinanceStarterService } from '@erpx/server-finance/application/starter';
import { financeStarterSchema } from '@erpx/server-finance/domain/starter';
export { financeStarterSchema } from '@erpx/server-finance/domain/starter';
export async function financeSetupAuthority(db: SqlDatabase, companyId: string) { return nativeStarter(db).financeSetupAuthority(companyId); }
export async function createFinanceStarter(actor: WorkContext, raw: z.input<typeof financeStarterSchema>, key: string) { return withFinanceConfigurationCommand(actor,{key,operation:"finance.starter.create",purpose:"new-work"},tx=>nativeStarter(tx).createFinanceStarter(actor,raw,key)); }
function nativeStarter(provided?: SqlDatabase) { let pending: Promise<SqlDatabase> | undefined, owned: SqlDatabase | undefined; const database = () => owned ? Promise.resolve(owned) : provided ? Promise.resolve(provided) : pending!; return createFinanceStarterService({ error: (...args) => new ApiError(...args), permission: requirePermission, ready: async (actor) => { if (!provided)
        await (pending ??= holdingResources(actor.holdingId).then(r => r.write)); }, now: Date.now, transaction: async <Result>(work: () => Promise<Result>) => provided ? work() : financeTransaction(await database(), async (tx) => { owned = tx; try {
        return await work();
    }
    finally {
        owned = undefined;
    } }), uniqueConflict: error => { const databaseError = error instanceof ApiError ? error.cause : error; return databaseError instanceof Error && 'code' in databaseError && databaseError.code === '23505'; }, replay: async (actor, key, op, id, input) => replayCommand(await database(), actor, key, op, id, input), requireMoneyScale: async (actor) => { await requireFinanceMoneyScale(await database(), actor.companyId, 'THB'); }, company: async (actor) => lockFinanceStarterCompany(await database(), actor.companyId), existingChart: async (actor) => !!await (await database()).prepare('SELECT 1 FROM finance_charts WHERE company_id=?').bind(actor.companyId).first(), overlappingPeriod: async (actor, start, end) => !!await (await database()).prepare('SELECT 1 FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=?').bind(actor.companyId, end, start).first(), organizationNodes: async (actor) => financeStarterOrganizationNodes(await database(), actor.companyId), candidates: async (company, now) => financeSetupCandidates(await database(), company, now), independent: async (company, preparers, approvers) => financeSetupIndependent(await database(), company, preparers, approvers), commit: async ({ actor, input, key, id, op, plan, now }) => { const db = await database(), statements = [assertion(db, key, 'no-existing-chart', 'NOT EXISTS(SELECT 1 FROM finance_charts WHERE company_id=?)', [actor.companyId]), await nativeFinanceStarter(db, actor.companyId).chart([id, actor.companyId, input.name, actor.userId, now, now]), ...await Promise.all(plan.accounts.map(async a => await nativeFinanceStarter(db, actor.companyId).account([a.id, id, actor.companyId, a.code, a.name, a.category, a.isControl, a.controlKind, now, now]))), await nativeFinanceStarter(db, actor.companyId).period([plan.periodId, actor.companyId, input.firstMonth, plan.start, plan.end, now])]; for (const dimension of plan.dimensions) {
        statements.push(await nativeFinanceStarter(db, actor.companyId).dimension([dimension.id, actor.companyId, dimension.code, dimension.name, actor.userId, now]));
        for (const value of dimension.values)
            statements.push(await nativeFinanceStarter(db, actor.companyId).dimensionValue([value.id, actor.companyId, dimension.id, value.code, value.name, now]));
    } return executeCommand(db, actor, key, op, input, id, 1, { id, periodId: plan.periodId, currency: 'THB', preset: input.preset, version: 1, accountCount: plan.accounts.length, policyReviewRequired: true }, statements); } }); }
