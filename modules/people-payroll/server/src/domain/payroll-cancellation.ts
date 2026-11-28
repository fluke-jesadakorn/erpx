import {z} from 'zod';
export const payrollCancelSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(8).max(1000),
}).strict();
export type CancellationRun = {
  id: string; company_id: string; status: string; mode: string; version: number;
  revision: number; created_by: string; submitted_by: string | null;
  approved_by: string | null; posted_by: string | null; source_snapshot: string;
};
export const cancellableStates = new Set(["draft", "calculating", "failed", "preview"]);
export type PayrollCancelInput=z.infer<typeof payrollCancelSchema>;
