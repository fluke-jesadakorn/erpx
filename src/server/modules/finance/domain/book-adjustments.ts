import {z} from 'zod';
import {createJournalSchema} from './finance';
import {financeReviewSchema} from './review';
export const bookAdjustmentSchema = createJournalSchema
  .pick({ chartId: true, periodId: true, entryDate: true, lines: true })
  .extend({
    primaryJournalId: z.string().min(1),
    reason: z.string().trim().min(3).max(2000),
  });
export const bookAdjustmentReversalSchema = financeReviewSchema.extend({
  periodId: z.string().min(1),
  entryDate: z.string().date(),
});

export type BookAdjustmentRow=Record<string,string|number|null>;
export type BookAdjustmentSource={source:BookAdjustmentRow;accounts:BookAdjustmentRow[];hash:string};
export type BookAdjustmentConsole={rows:BookAdjustmentRow[];books:BookAdjustmentRow[];accounts:BookAdjustmentRow[];sources:BookAdjustmentRow[];periods:BookAdjustmentRow[]};
