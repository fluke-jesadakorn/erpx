import { z } from 'zod';
export const financeToolSchema = z.object({ tool: z.enum(['planning', 'bank_automation', 'collections', 'business_contracts', 'instruments', 'asset_policies', 'group_reporting']), enabled: z.boolean(), expectedVersion: z.number().int().nonnegative(), reason: z.string().trim().min(3).max(2000) });
export type FinanceTool = z.infer<typeof financeToolSchema>['tool'];
export type FinanceToolInput = z.infer<typeof financeToolSchema>;
export type CompanyTool = {
    tool: FinanceTool;
    enabled: number;
    version: number;
};
export type SetCompanyToolResult = {
    id: string;
    version: number;
    enabled: boolean;
};
