import {z} from 'zod';
export type QualityAlert = {
  id: string;
  hold_id: string;
  phase: "due_soon" | "overdue";
  source_version: number;
  hold_version: number;
  due_date: string;
  detected_at: number;
  version: number;
  acknowledged_by: string | null;
  acknowledged_at: number | null;
  reason: string | null;
  company_id: string;
  branch_id: string;
  status: string;
  product_name: string;
  warehouse_name: string;
  can_ack: number;
  recipients: number;
};


export const acknowledgeSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});


export type QualityAlertAcknowledgement=z.infer<typeof acknowledgeSchema>;
export type QualityAlertResult={dueSoon:number;overdue:number};
export const qualityTerminalStates=['released','cancelled','disposed'];
export function qualityAlertDate(now: number) {
  return new Date(now).toISOString().slice(0, 10);
}
