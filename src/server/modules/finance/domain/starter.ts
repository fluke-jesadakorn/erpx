import { z } from 'zod';
import { thaiFinanceStarterAccounts } from "@/lib/finance/finance-starter";
export const financeStarterSchema = z.object({ expectedVersion: z.literal(0), preset: z.literal('thai_services_v1'), name: z.string().trim().min(1).max(160), firstMonth: z.string().regex(/^[1-9]\d{3}-(0[1-9]|1[0-2])$/), organizationDimensions: z.boolean().default(true), reason: z.string().trim().min(3).max(2000) });
export type StarterInput = z.infer<typeof financeStarterSchema>;
export type StarterNode = {
    id: string;
    kind: string;
    display_name: string;
};
export type StarterCandidateFacts = {
    rows: {
        user_id: string;
        role: string;
    }[];
    permissions: {
        role: string;
        permissions: string[];
    }[];
};
export function starterDates(firstMonth: string) { const [year, month] = firstMonth.split('-').map(Number); return { start: `${firstMonth}-01`, end: new Date(Date.UTC(year, month, 0)).toISOString().slice(0, 10) }; }
export function starterConfiguration(input: StarterInput, companyId: string, id: string, nodes: StarterNode[]) { const dates = starterDates(input.firstMonth), periodId = `starter-period-${companyId}-${input.firstMonth}`, accounts = thaiFinanceStarterAccounts.map(a => ({ ...a, id: `${id}-${a.code}`, isControl: a.controlKind === 'none' ? 0 : 1 })), dimensions = (['branch', 'department'] as const).flatMap(kind => { const values = nodes.filter(n => n.kind === kind), dimensionId = `starter-${companyId}-${kind}`; return values.length ? [{ id: dimensionId, code: kind, name: kind === 'branch' ? 'Branch' : 'Department', values: values.map(node => ({ id: `${dimensionId}-${node.id}`, code: node.id, name: node.display_name.slice(0, 160) })) }] : []; }); return { id, periodId, ...dates, accounts, dimensions }; }
export type StarterPlan = ReturnType<typeof starterConfiguration>;
