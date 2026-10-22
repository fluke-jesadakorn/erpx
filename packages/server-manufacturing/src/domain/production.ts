import {z} from 'zod';import {decimalToMinor} from "@erpx/feature-finance/lib/finance-money";export const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive(),
  date = z.string().date(),
  qty = z.string().regex(/^\d+(\.\d{1,3})?$/);
export const q = (x: string) => decimalToMinor(x, 3);
export const specSchema = z.object({
  name: z.string().trim().min(2).max(160),
  mode: z.enum(["bom", "formula"]),
  outputProductId: z.string().min(1),
  batchQuantity: qty,
  expectedYieldBps: z.number().int().min(1).max(20000),
  lines: z
    .array(
      z.object({
        productId: z.string().min(1),
        quantity: qty,
        substitutionGroup: z.string().max(80).nullable().optional(),
      }),
    )
    .min(1)
    .max(100),
  operations: z.array(z.object({
    resourceId: z.string().min(1), name: z.string().trim().min(2).max(160),
    setupMinutes: z.number().int().nonnegative(), runMinutesPerBatch: z.number().int().positive(),
  })).max(50).default([]),
});
export const reviewSchema = z.object({
  action: z.enum(["approve", "reject", "retire"]),
  expectedVersion: version,
  reason,
});
export const orderSchema = z.discriminatedUnion("action", [
  z.object({
    action: z.literal("reserve"),
    expectedVersion: version,
    expiresAt: z.number().int().positive(),
    reason,
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    entryDate: date.optional(),
  }),
  z.object({ action: z.literal("issue"), expectedVersion: version, reason, entryDate: date.optional(),
    materialLotAllocations: z.array(z.object({ productId: z.string().min(1), lotId: z.string().min(1), quantity: qty, balanceVersion: version })).max(500).default([]),
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]) }),
  z.object({
    action: z.literal("report"),
    expectedVersion: version,
    outputQuantity: qty,
    scrapQuantity: qty,
    laborCost: z.string().regex(/^\d+(\.\d{1,2})?$/),
    overheadCost: z.string().regex(/^\d+(\.\d{1,2})?$/),
    reason,
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    entryDate: date.optional(),
  }),
  z.object({
    action: z.enum(["pass-qc", "fail-qc", "rework", "cancel", "scrap"]),
    expectedVersion: version,
    reason,
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    entryDate: date.optional(),
    outputLotAllocations: z.array(z.object({ quantity: qty, lotCode: z.string().trim().min(1).max(120).optional(),
      serialNumber: z.string().trim().min(1).max(120).optional(), expiryDate: date.optional() })).max(500).default([]),
  }),
  z.object({
    action: z.literal("complete-operation"), expectedVersion: version, operationId: z.string().min(1),
    laborMinutes: z.number().int().nonnegative(), machineMinutes: z.number().int().nonnegative(), reason,
    entryDate: date.optional(), attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
  }),
]);
