import { z } from "zod";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
const amount = z
    .string()
    .regex(/^\d+(\.\d{1,2})?$/, "Use a positive amount with at most two decimal places.");
const quantity = z
    .string()
    .regex(/^\d+(\.\d{1,3})?$/, "Use a quantity with at most three decimal places.");
export const orderSchema = z.object({
    kind: z.enum(["quote", "sales", "purchase"]),
    requestId: z.string().min(1).max(180).optional(),
    requestVersion: z.number().int().positive().optional(),
    rfqId: z.string().min(1).max(180).optional(),
    rfqVersion: z.number().int().positive().optional(),
    partyId: z.string().min(1),
    warehouseId: z.string().default(""),
    orderDate: z.iso.date(),
    dueDate: z.iso.date(),
    note: z.string().max(2000).default(""),
    priceListId: z.string().min(1).optional(),
    discountReason: z.string().trim().max(1000).optional(),
    lines: z
        .array(z.object({
        productId: z.string().min(1),
        quantity,
        price: amount,
        taxBps: z.number().int().min(0).max(10000),
    }))
        .min(1)
        .max(20),
    expectedVersion: z.number().int().positive().optional(),
});
export const transitionSchema = z.object({
    action: z.enum(["submit", "approve", "return", "cancel", "convert"]),
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().max(1000).optional(),
});
export const fulfillmentSchema = z.object({
    completionMode: z.enum(["goods", "service", "mixed"]).default("goods"),
    expectedVersion: z.number().int().positive(),
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    date: z.iso.date(),
    evidenceIds: z.array(z.string().min(1)).max(10).default([]),
    evidenceNote: z.string().trim().max(1000).default(''),
    lines: z
        .array(z.object({
        lineId: z.string().min(1),
        quantity,
        allocations: z.array(z.object({
            lotId: z.string().min(1).optional(),
            lotCode: z.string().trim().min(1).max(100).optional(),
            serialNumber: z.string().trim().min(1).max(160).optional(),
            quantity,
            manufacturedDate: z.union([z.iso.date(), z.literal("")]).default(""),
            expiryDate: z.union([z.iso.date(), z.literal("")]).default(""),
            disposition: z.enum(["available", "quarantine"]).default("available"),
            quarantineReason: z.string().trim().max(2000).default(""),
            quarantineDueDate: z.union([z.iso.date(), z.literal("")]).default(""),
        })).max(200).default([]),
    }))
        .min(1)
        .max(20),
});
export const invoiceFromDeliverySchema = z.object({
    useSalesDefaults: z.boolean().optional(),
    additionalFulfillments: z.array(z.object({ id: z.string().min(1), expectedVersion: z.number().int().positive() })).max(20).optional(),
    matchId: z.string().min(1).optional(),
    matchVersion: z.number().int().positive().optional(),
    expectedVersion: z.number().int().positive(),
    date: z.iso.date(),
    dueDate: z.iso.date(),
});
export const settlementSchema = z.object({
    periodId: z.string().min(1).optional(),
    bankAccountId: z.string().min(1).optional(),
    paymentSlipId: z.string().min(1).optional(),
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    expectedVersion: z.number().int().positive(),
    date: z.iso.date(),
    amount,
    reference: z.string().trim().min(2).max(100),
});
export const returnRequestSchema = z.object({
    expectedVersion: z.number().int().positive(),
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    date: z.iso.date(),
    reason: z.string().trim().min(3).max(500),
    lines: z.array(z.object({ lineId: z.string().min(1), quantity, allocations: z.array(z.object({ lotId: z.string().min(1), quantity })).max(200).default([]), condition: z.enum(['unopened', 'damaged', 'unknown']).default('unknown'), conditionNote: z.string().trim().max(500).default(''), qualityDueDate: z.union([z.iso.date(), z.literal('')]).default('') })).min(1).max(20),
});
export const returnDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(500), attachmentSetIds: z.array(z.string().min(1)).max(10).optional() });
export const returnCompleteSchema = z.object({ expectedVersion: z.number().int().positive(), fxGainAccountId: z.string().min(1).optional(), fxLossAccountId: z.string().min(1).optional() });
export const productSchema = z.object({
    itemType: z.enum(["stock", "service"]).default("stock"),
    sku: z.string().trim().min(2).max(40),
    name: z.string().trim().min(2).max(160),
    unit: z.string().trim().min(1).max(30).default("each"),
    trackingMode: z.enum(["none", "lot", "serial"]).default("none"),
    price: amount,
    taxBps: z.number().int().min(0).max(10000).default(0),
    status: z.enum(["active", "archived"]).default("active"),
    expectedVersion: z.number().int().positive().optional(),
}).refine(value => value.itemType !== "service" || value.trackingMode === "none", { message: "Services cannot use lot or serial tracking.", path: ["trackingMode"] });
export const partyMasterSchema = z.object({
    kind: z.enum(["customer", "supplier"]),
    code: z.string().trim().min(2).max(40),
    name: z.string().trim().min(2).max(160),
    paymentTermsDays: z.number().int().min(0).max(365).default(30),
    creditLimit: amount.default("0"),
    status: z.enum(["active", "blocked"]).default("active"),
    notificationEmail: z.string().trim().max(320).refine(value => !value || z.email().safeParse(value).success, 'Use a valid email address.').default(''),
    notificationConsent: z.enum(['unknown', 'granted', 'revoked']).default('unknown'),
    expectedVersion: z.number().int().positive().optional(),
});
export const warehouseSchema = z.object({
    code: z.string().trim().min(2).max(40),
    name: z.string().trim().min(2).max(160),
    branchId: z.string().min(1),
    status: z.enum(["active", "archived"]).default("active"),
    expectedVersion: z.number().int().positive().optional(),
});
export const roles = { sales: "commerce.sales", purchase: "commerce.purchase", stock: "inventory.prepare", finance: "finance.write", read: "commerce.read" } as const;
export function commercePermissions(actor: WorkContext) {
    return {
        sales: hasPermission(actor, roles.sales),
        purchase: hasPermission(actor, roles.purchase),
        stock: hasPermission(actor, roles.stock),
        finance: hasPermission(actor, roles.finance),
        read: hasPermission(actor, roles.read),
        approve: hasPermission(actor, "commerce.approve"),
    };
}
export const stockReservationSchema = z.object({
    expectedVersion: z.number().int().positive(),
    action: z.enum(['reserve', 'release']),
    lineId: z.string().min(1),
    quantity,
    expiresAt: z.number().int().positive().optional(),
    reason: z.string().trim().min(3).max(500),
});
