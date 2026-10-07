import 'server-only';
import { z } from 'zod';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import { requirePermission } from '@/server/modules/identity_access/public/iam-permissions';
import { financeSetupCandidates, financeSetupIndependent } from '@/server/modules/identity_access/public/finance-setup-sources';
import { lockFinanceStarterCompany, financeStarterOrganizationNodes } from '@/server/modules/organization/public/finance-starter-sources';
import { holdingResources } from '@/server/modules/runtime/public/runtime';
import { financeTransaction } from '../public/finance-transactions';
import { assertion, executeCommand, replayCommand } from '@/server/modules/workflow/public/commands';
import { requireFinanceMoneyScale } from '../public/finance-money-policy';
import { ApiError } from '@/server/modules/runtime/public/http';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { createFinanceStarterService } from '../application/starter';
import { financeStarterSchema } from '../domain/starter';
export { financeStarterSchema } from '../domain/starter';
export async function financeSetupAuthority(db: SqlDatabase, companyId: string) { return nativeStarter(db).financeSetupAuthority(companyId); }
export async function createFinanceStarter(actor: WorkContext, raw: z.input<typeof financeStarterSchema>, key: string) { return nativeStarter().createFinanceStarter(actor, raw, key); }
function nativeStarter(provided?: SqlDatabase) { let pending: Promise<SqlDatabase> | undefined, owned: SqlDatabase | undefined; const database = () => owned ? Promise.resolve(owned) : provided ? Promise.resolve(provided) : pending!; return createFinanceStarterService({ error: (...args) => new ApiError(...args), permission: requirePermission, ready: async (actor) => { if (!provided)
        await (pending ??= holdingResources(actor.holdingId).then(r => r.write)); }, now: Date.now, transaction: async <Result>(work: () => Promise<Result>) => financeTransaction(await database(), async (tx) => { owned = tx; try {
        return await work();
    }
    finally {
        owned = undefined;
    } }), uniqueConflict: error => { const databaseError = error instanceof ApiError ? error.cause : error; return databaseError instanceof Error && 'code' in databaseError && databaseError.code === '23505'; }, replay: async (actor, key, op, id, input) => replayCommand(await database(), actor, key, op, id, input), requireMoneyScale: async (actor) => { await requireFinanceMoneyScale(await database(), actor.companyId, 'THB'); }, company: async (actor) => lockFinanceStarterCompany(await database(), actor.companyId), existingChart: async (actor) => !!await (await database()).prepare('SELECT 1 FROM finance_charts WHERE company_id=?').bind(actor.companyId).first(), overlappingPeriod: async (actor, start, end) => !!await (await database()).prepare('SELECT 1 FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=?').bind(actor.companyId, end, start).first(), organizationNodes: async (actor) => financeStarterOrganizationNodes(await database(), actor.companyId), candidates: async (company, now) => financeSetupCandidates(await database(), company, now), independent: async (company, preparers, approvers) => financeSetupIndependent(await database(), company, preparers, approvers), commit: async ({ actor, input, key, id, op, plan, now }) => { const db = await database(), statements = [assertion(db, key, 'no-existing-chart', 'NOT EXISTS(SELECT 1 FROM finance_charts WHERE company_id=?)', [actor.companyId]), db.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES(?,?,?,'THB','Company policy pending review',?,?,?)").bind(id, actor.companyId, input.name, actor.userId, now, now), ...plan.accounts.map(a => db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,?,?)").bind(a.id, id, actor.companyId, a.code, a.name, a.category, a.isControl, a.controlKind, now, now)), db.prepare('INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?,?,?,?)').bind(plan.periodId, actor.companyId, input.firstMonth, plan.start, plan.end, now)]; for (const dimension of plan.dimensions) {
        statements.push(db.prepare('INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,created_by,created_at) VALUES(?,?,?,?,0,?,?)').bind(dimension.id, actor.companyId, dimension.code, dimension.name, actor.userId, now));
        for (const value of dimension.values)
            statements.push(db.prepare('INSERT INTO finance_dimension_values(id,company_id,dimension_id,code,name,created_at) VALUES(?,?,?,?,?,?)').bind(value.id, actor.companyId, dimension.id, value.code, value.name, now));
    } return executeCommand(db, actor, key, op, input, id, 1, { id, periodId: plan.periodId, currency: 'THB', preset: input.preset, version: 1, accountCount: plan.accounts.length, policyReviewRequired: true }, statements); } }); }
