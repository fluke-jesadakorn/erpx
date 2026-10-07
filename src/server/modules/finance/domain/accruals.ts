import { z } from 'zod';
import type { FinanceError } from './invoice-line-revision';
const date = z.string().date();
export const accrualScheduleSchema = z.object({
    description: z.string().trim().min(3).max(240),
    debitAccountId: z.string().min(1),
    creditAccountId: z.string().min(1),
    currency: z.string().length(3).transform(value => value.toUpperCase()),
    total: z.string().regex(/^\d+(\.\d{1,2})?$/),
    startDate: date,
    endDate: date,
    frequency: z.enum(["monthly", "quarterly", "annual"]).default("monthly"),
});
export const accrualScheduleDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(1000) });
export const accrualRunPostSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(1000) });
export type AccrualRow = Record<string, string | number | null>;
export type AccrualAccount = {
    id: string;
    chart_id: string;
    currency: string;
    status: string;
    base_currency: string;
};
export type AccrualInput = z.infer<typeof accrualScheduleSchema>;
export type AccrualDecision = z.infer<typeof accrualScheduleDecisionSchema>;
export type AccrualRunPost = z.infer<typeof accrualRunPostSchema>;
export function createAccrualPolicy(error: FinanceError) {
    function addMonths(value: string, count: number) { const dateValue = new Date(`${value}T00:00:00Z`); dateValue.setUTCMonth(dateValue.getUTCMonth() + count); return dateValue.toISOString().slice(0, 10); }
    function periodEnd(start: string, frequency: "monthly" | "quarterly" | "annual") { const dateValue = new Date(`${start}T00:00:00Z`); dateValue.setUTCMonth(dateValue.getUTCMonth() + ({ monthly: 1, quarterly: 3, annual: 12 }[frequency])); dateValue.setUTCDate(0); return dateValue.toISOString().slice(0, 10); }
    function frequencyMonths(frequency: string) { return frequency === "annual" ? 12 : frequency === "quarterly" ? 3 : 1; }
    function periodCount(start: string, end: string, frequency: string) { let count = 0, cursor = start; const step = frequencyMonths(frequency); while (cursor <= end && count < 241) {
        count++;
        cursor = addMonths(cursor, step);
    } if (!count || cursor <= end)
        throw error(422, "ACCRUAL_RANGE_TOO_LONG", "Accrual schedules must contain between one and 240 periods."); return count; }
    function periodIndex(start: string, value: string, frequency: string) { let index = 0, cursor = start; const step = frequencyMonths(frequency); while (cursor < value && index < 241) {
        cursor = addMonths(cursor, step);
        index++;
    } return index; }
    function allocate(total: number, count: number, index: number) { const base = Math.floor(total / count), remainder = total % count; return base + (index < remainder ? 1 : 0); }
    return { addMonths, periodEnd, frequencyMonths, periodCount, periodIndex, allocate };
}
