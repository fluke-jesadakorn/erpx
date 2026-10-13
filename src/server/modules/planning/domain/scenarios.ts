import {z} from 'zod';import {decimalToMinor} from "@/lib/finance/finance-money";import type {PlanningContext} from './capacity';export const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive(),
  date = z.string().date(),
  qty = z.string().regex(/^\d+(\.\d{1,3})?$/);
export const q = (x: string) => decimalToMinor(x, 3);
export const planSchema = z.object({
  warehouseId: z.string().min(1),
  specificationId: z.string().min(1),
  specificationVersion: version,
  demandQuantity: qty.optional(),
  salesOrderId: z.string().min(1).optional(),
  salesOrderVersion: version.optional(),
  dueDate: date,
  startDate: date.optional(),
  capacityMinutes: z.number().int().nonnegative().default(0),
  manualMinutesPerUnit: z.number().int().positive().default(10),
  comparisonScenarioId: z.string().min(1).optional(),
}).refine(i => i.salesOrderId ? Boolean(i.salesOrderVersion) : Boolean(i.demandQuantity), "Choose a source order/version or enter manual demand.")
  .refine(i => !i.startDate || i.startDate <= i.dueDate, "The planning start date must be on or before the due date.");
export const recommendationSchema = z.object({
  action: z.enum(["approve", "reject"]),
  expectedVersion: version,
  reason,
});
export type PlanningExecutiveEvidenceMetric =
  | "planning-shortage-scenarios"
  | "proposed-planning-recommendations"
  | "capacity-feasible-recommendations";
export type PlanningScenarioResult = {
  id: string; recommendationId: string; status: string; version: number;
  shortages: Array<{ productId: string; requiredMilli: number; availableMilli: number; incomingMilli: number; shortageMilli: number }>;
  feasibleDate: string; requiredMinutes: number; capacityMinutes: number; context: PlanningContext;
};
