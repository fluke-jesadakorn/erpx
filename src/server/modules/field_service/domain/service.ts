import { z } from "zod";
import type { WorkItem } from "@/lib/work_projection/work-item";
import { BusinessRuleError, consumptionLotSchema } from "../../shared";

const reason = z.string().trim().min(3).max(3000), version = z.number().int().positive();
export const agreementActionSchema = z.object({ action: z.enum(["activate", "reject", "retire"]), expectedVersion: version, reason });
export const createOrderSchema = z.object({ ticketId: z.string().min(1), ticketVersion: version, agreementId: z.string().min(1), agreementVersion: version, assetId: z.string().nullable().optional(), issue: reason, site: z.string().trim().min(3).max(300), technicianId: z.string().min(1), scheduledAt: z.number().int().positive().nullable().optional() });
export const orderActionSchema = z.discriminatedUnion("action", [
  z.object({ action: z.literal("schedule"), expectedVersion: version, technicianId: z.string().min(1), scheduledAt: z.number().int().positive(), reason }),
  z.object({ action: z.literal("reserve-part"), expectedVersion: version, warehouseId: z.string().min(1), productId: z.string().min(1), quantityMilli: z.number().int().positive(), expiresAt: z.number().int().positive(), reason }),
  z.object({ action: z.literal("issue-part"), expectedVersion: version, reservationId: z.string().min(1), quantityMilli: z.number().int().positive(), entryDate: z.string().date().optional(), allocations: z.array(consumptionLotSchema).max(200).default([]), reason }),
  z.object({ action: z.enum(["dispatch", "start", "resume", "cancel"]), expectedVersion: version, reason }),
  z.object({ action: z.literal("exception"), expectedVersion: version, reason }),
  z.object({ action: z.literal("complete"), expectedVersion: version, laborMinutes: z.number().int().nonnegative(), evidence: z.string().trim().min(3).max(4000), customerConfirmation: z.string().trim().min(3).max(4000), reason }),
  z.object({ action: z.literal("release-billing"), expectedVersion: version, reason }),
]);
export type AgreementAction = z.infer<typeof agreementActionSchema>;
export type CreateOrderInput = z.infer<typeof createOrderSchema>;
export type OrderAction = z.infer<typeof orderActionSchema>;
export type FieldRow = Record<string, string | number | null>;
export type Agreement = FieldRow & { id: string; version: number; status: string; created_by: string; updated_at: number; can_review: number; warranty_discount_bps: number };
export type Order = FieldRow & { id: string; version: number; status: string; technician_id: string; created_by: string; updated_at: number; scheduled_at: number | null; technician_eligible: number; can_review: number; issue: string; site: string; exception_reason: string | null };
export type OrderFacts = Order & { agreement_id: string; visit_rate_minor: number; warranty_discount_bps: number; warranty_end: string | null; currency: string; agreement_status: string; ticket_id: string; customer_id: string };
export type FieldWorkspace = { agreements: Agreement[]; orders: Order[]; customers: FieldRow[]; assets: FieldRow[]; tickets: FieldRow[]; people: FieldRow[]; nodes: FieldRow[]; canWrite: boolean };
export type BillingTerms = { invoiceDate: string; dueDate: string; warrantyDiscountBps: number; warrantyApplied: boolean; amountMinor: number };
export type OrderTransition = { status: string; technicianId: string; scheduledAt: number | null; exceptionReason: string | null; details: Record<string, unknown>; billing: BillingTerms | null };

export function agreementTransition(input: AgreementAction) {
  return { expected: input.action === "retire" ? "active" : "draft", status: input.action === "activate" ? "active" : input.action === "reject" ? "rejected" : "retired" };
}
export function assertOrderAuthority(row: OrderFacts, input: OrderAction, actorId: string, canReview: boolean) {
  const manage = canReview && Number(row.can_review) > 0, own = row.technician_id === actorId && Number(row.technician_eligible) > 0;
  if (!own && !manage) throw new BusinessRuleError(403, "FIELD_SERVICE_OWNER_REQUIRED", "The technician or an eligible reviewer must perform this action.");
  const allowed: Record<OrderAction["action"], string[]> = { schedule: ["open", "scheduled"], "reserve-part": ["open", "scheduled"], dispatch: ["scheduled"], start: ["dispatched"], exception: ["dispatched", "in_visit"], resume: ["exception"], "issue-part": ["in_visit"], complete: ["in_visit"], "release-billing": ["completed"], cancel: ["open", "scheduled", "dispatched", "exception"] };
  if (!allowed[input.action].includes(row.status)) throw new BusinessRuleError(409, "FIELD_SERVICE_STATE_CHANGED", "Refresh the service order before continuing.");
  return { manage, own };
}
export function orderTransition(row: OrderFacts, input: OrderAction, actorId: string, manage: boolean, now: number): OrderTransition {
  const plan: OrderTransition = { status: row.status, technicianId: row.technician_id, scheduledAt: row.scheduled_at, exceptionReason: row.exception_reason, details: {}, billing: null };
  switch (input.action) {
    case "schedule": plan.status = "scheduled"; plan.technicianId = input.technicianId; plan.scheduledAt = input.scheduledAt; break;
    case "reserve-part":
      if (input.expiresAt <= now) throw new BusinessRuleError(422, "FIELD_SERVICE_PART_EXPIRY", "Part reservation must expire in the future.");
      plan.details = { warehouseId: input.warehouseId, productId: input.productId, quantityMilli: input.quantityMilli }; break;
    case "dispatch": plan.status = "dispatched"; break;
    case "start": plan.status = "in_visit"; break;
    case "exception": plan.status = "exception"; plan.exceptionReason = input.reason; break;
    case "resume": plan.status = "in_visit"; plan.exceptionReason = null; break;
    case "complete": plan.status = "completed"; plan.details = { laborMinutes: input.laborMinutes, evidence: input.evidence, customerConfirmation: input.customerConfirmation }; break;
    case "release-billing": {
      if (!manage || row.technician_id === actorId) throw new BusinessRuleError(409, "INDEPENDENT_BILLING_RELEASE_REQUIRED", "A different eligible reviewer must release billing.");
      const invoiceDate = new Date(now).toISOString().slice(0, 10), warrantyDiscountBps = Math.max(0, Math.min(10000, Number(row.warranty_discount_bps ?? 0))), warrantyApplied = Boolean(row.warranty_end && String(row.warranty_end) >= invoiceDate), amountMinor = warrantyApplied ? Math.floor((Number(row.visit_rate_minor) * (10000 - warrantyDiscountBps)) / 10000) : Number(row.visit_rate_minor);
      plan.status = "billed"; plan.billing = { invoiceDate, dueDate: new Date(now + 30 * 86400000).toISOString().slice(0, 10), warrantyDiscountBps, warrantyApplied, amountMinor }; break;
    }
    case "cancel": plan.status = "cancelled"; break;
    case "issue-part": break;
  }
  return plan;
}
export function fieldServiceWorkItems(data: FieldWorkspace, actorId: string, canReview: boolean): WorkItem[] {
  const agreementWork = data.agreements.filter(g => g.status === "draft").map(g => ({ id: String(g.id), type: "field_service" as const, title: `Agreement review · ${g.title}`, detail: `${g.customer_name} · ${g.start_date} to ${g.end_date}`, href: "/field-service", status: String(g.status), updatedAt: Number(g.updated_at), queue: Number(g.can_review) && g.created_by !== actorId ? "todo" as const : "waiting" as const, canAct: Boolean(Number(g.can_review) && g.created_by !== actorId), responsibleQueue: "Independent service manager", relationships: [g.created_by === actorId ? "requested" as const : "approval" as const] }));
  const orders = data.orders.map(o => {
    const terminal = ["cancelled", "billed"].includes(o.status), manage = canReview && Number(o.can_review) > 0, own = o.technician_id === actorId && Number(o.technician_eligible) > 0, canAct = !terminal && (own || manage);
    return { id: String(o.id), type: "field_service" as const, title: `Field visit · ${o.ticket_title}`, detail: `${o.site} · ${o.customer_name}`, href: `/field-service/${o.id}`, status: String(o.status), updatedAt: Number(o.updated_at), dueAt: o.scheduled_at ? Number(o.scheduled_at) : null, queue: terminal ? "completed" as const : canAct ? "todo" as const : "waiting" as const, canAct, responsibleQueue: o.status === "completed" ? "Independent billing reviewer" : "Assigned technician", nextAction: o.status === "completed" ? "Review visit evidence and release billing" : o.status === "exception" ? "Resolve site exception" : "Continue scheduled field visit", blocker: Number(o.technician_eligible) ? o.exception_reason : "Assigned technician is no longer eligible; reviewer reassignment is required", relationships: [o.technician_id === actorId ? "assigned" as const : o.created_by === actorId ? "requested" as const : "team" as const] };
  });
  return [...agreementWork, ...orders];
}
