import { financeReferenceListSql } from './finance-read-tail-r42-sql';
import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/finance-server/adapters/purchase-boundary";
import { financeCompanyTools as nativeFinanceCompanyTools } from '@erpx/collaboration-server/adapters/native-finance-company-tools';
import { createCompanyTools } from '@erpx/finance-server/application/company-tools';
import type { FinanceTool, FinanceToolInput, CompanyTool, SetCompanyToolResult } from '@erpx/finance-server/domain/company-tools';
export { financeToolSchema } from '@erpx/finance-server/domain/company-tools';
export type { FinanceTool } from '@erpx/finance-server/domain/company-tools';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { holdingResources } from '../../../../../platform/runtime/server/src/public/runtime';
import { requirePermission } from '@erpx/identity-access-server/public/iam-permissions';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/workflow-server/public/commands';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
export async function financeCompanyTools(actor: WorkContext) { return withFinancePurchasedRead(actor,db=>companyToolService(db).financeCompanyTools(actor),"finance.accounting",true); }
export async function requireFinanceTool(db: SqlDatabase, companyId: string, tool: FinanceTool) { return companyToolService(db).requireFinanceTool(companyId, tool); }
export async function setFinanceCompanyTool(actor: WorkContext, input: FinanceToolInput, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.tool.set",purpose:input.enabled?"new-work":"settle-existing",capability:["instruments","group_reporting"].includes(input.tool)?"finance.advanced":"finance.accounting"},tx=>companyToolService(tx).setFinanceCompanyTool(actor,input,key)); }
function companyToolService(provided?: SqlDatabase) { let pending: Promise<SqlDatabase> | undefined; const db = (actor: WorkContext) => provided ? Promise.resolve(provided) : pending ??= holdingResources(actor.holdingId).then(r => r.write); return createCompanyTools({ error: (...args) => new ApiError(...args), permission: requirePermission, ready: async (actor) => { await db(actor); }, now: Date.now, rows: async (actor) => (await (await db(actor)).prepare(financeReferenceListSql('finance_company_tools','company_id=?','tool')).bind(actor.companyId).all<CompanyTool>()).results, enabled: async (companyId, tool) => { if (!provided)
        throw new Error('Company tool eligibility requires its supplied database'); return !!await provided.prepare('SELECT 1 FROM finance_company_tools WHERE company_id=? AND tool=? AND enabled=1').bind(companyId, tool).first(); }, replay: async (actor, key, operation, id, input) => replayCommand<SetCompanyToolResult>(await db(actor), actor, key, operation, id, input), commit: async ({ actor, input, key, id, operation, version, now, result }) => { const write = await db(actor); return executeCommand(write, actor, key, operation, input, id, version, result, [assertion(write, key, 'tool-version', 'COALESCE((SELECT version FROM finance_company_tools WHERE company_id=? AND tool=?),0)=?', [actor.companyId, input.tool, input.expectedVersion]), await nativeFinanceCompanyTools(write, actor.companyId).set({ tool: input.tool, enabled: input.enabled, version, userId: actor.userId, reason: input.reason, now })]); } }); }
