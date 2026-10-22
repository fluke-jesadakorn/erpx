import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
import { financeToolSchema, type FinanceTool, type FinanceToolInput, type CompanyTool, type SetCompanyToolResult } from '@erpx/server-finance/domain/company-tools';
export interface CompanyToolPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: 'finance.read' | 'finance.write'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    rows(actor: WorkContext): Promise<CompanyTool[]>;
    enabled(companyId: string, tool: FinanceTool): Promise<boolean>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: FinanceToolInput): Promise<SetCompanyToolResult | null>;
    commit(facts: {
        actor: WorkContext;
        input: FinanceToolInput;
        key: string;
        id: string;
        operation: string;
        version: number;
        now: number;
        result: SetCompanyToolResult;
    }): Promise<SetCompanyToolResult>;
}
export function createCompanyTools(ports: CompanyToolPorts) {
    async function financeCompanyTools(actor: WorkContext) { actor = ports.permission(actor, 'finance.read'); await ports.ready(actor); return ports.rows(actor); }
    async function requireFinanceTool(companyId: string, tool: FinanceTool) { if (!await ports.enabled(companyId, tool))
        throw ports.error(409, 'FINANCE_TOOL_DISABLED', 'Enable this Company tool in Finance setup before preparing new work.'); }
    async function setFinanceCompanyTool(actor: WorkContext, raw: FinanceToolInput, key: string) { actor = ports.permission(actor, 'finance.write'); const input = financeToolSchema.parse(raw); await ports.ready(actor); const id = `${actor.companyId}:${input.tool}`, operation = 'finance.tool.set', prior = await ports.replay(actor, key, operation, id, input); if (prior)
        return prior; const version = input.expectedVersion + 1, result = { id, version, enabled: input.enabled }; return ports.commit({ actor, input, key, id, operation, version, now: ports.now(), result }); }
    return { financeCompanyTools, requireFinanceTool, setFinanceCompanyTool };
}
