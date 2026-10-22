import {z} from 'zod';
const quantityInput = z.string().regex(/^\d{1,12}(?:\.\d{1,3})?$/, "Use a positive quantity with at most three decimal places.");
export const inventoryReorderPolicySchema = z.object({
  warehouseId: z.string().min(1).max(200),
  productId: z.string().min(1).max(200),
  reorderPoint: quantityInput,
  reason: z.string().trim().min(5).max(2000),
  expectedVersion: z.number().int().positive().optional(),
}).strict();
export const inventoryReorderPolicyActionSchema = z.object({
  action: z.enum(["approve", "reject", "withdraw", "retire"]),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(5).max(2000),
}).strict();
export type InventoryReorderPolicyRow = {
  id: string;
  company_id: string;
  warehouse_id: string;
  warehouse_name: string;
  branch_id: string;
  product_id: string;
  sku: string;
  product_name: string;
  unit: string;
  current_reorder_point_milli: string | null;
  proposed_reorder_point_milli: string | null;
  requested_by: string;
  approved_by: string | null;
  approved_at: number | null;
  retired_at: number | null;
  last_decision: "approved" | "rejected" | "retired" | null;
  last_reviewed_by: string | null;
  last_review_reason: string | null;
  last_reviewed_at: number | null;
  version: number;
  created_at: number;
  updated_at: number;
};
