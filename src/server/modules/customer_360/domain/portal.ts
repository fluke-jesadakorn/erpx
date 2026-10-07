import { z } from "zod";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { HoldingId } from "../../runtime/public/runtime";

export type CustomerPortalSessionActor = { userId: string; email: string; holdingId?: HoldingId; holdingSlug?: string; holdingName?: string };
export type CustomerPortalBindingScope = { id: string; company_id: string; branch_id: string | null; department_id: string | null; version: number };
export type CustomerSharedFile = { linkId: string; label: string; previewAvailable: true };
export type PortalActionRow = { id: string; binding_id: string; order_id: string; action: string; status: string; created_at: number; request_hash: string; user_id: string };
export type PortalBinding = { binding_id: string; company_id: string; order_id: string; customer_id: string; email: string; status: string; expires_at: number; binding_version: number; order_version: number; number: string; state: string };
export type CustomerPortalItem = Record<string, unknown> & { binding_id: string; binding_version: number; order_id: string; number: string; kind: string; state: string; due_date: string; currency: string; updated_at: number; fulfillments: Record<string, unknown>[]; actions: Record<string, unknown>[]; sharedFiles: CustomerSharedFile[] };
export const portalCommandSchema = z.object({ bindingId: z.string().trim().min(1).max(200), expectedBindingVersion: z.coerce.number().int().positive(), action: z.enum(["acknowledge_delivery", "request_support"]), note: z.string().trim().max(500).default(""), attachmentSetIds: z.array(z.string().min(1).max(180)).max(20).default([]) });
export type PortalCommandInput = z.infer<typeof portalCommandSchema>;
export const portalActionResult = (row: PortalActionRow) => ({ id: row.id, bindingId: row.binding_id, orderId: row.order_id, action: row.action, status: row.status, createdAt: Number(row.created_at) });

export function customerPortalWorkContext(session: Required<CustomerPortalSessionActor>, binding: CustomerPortalBindingScope): WorkContext {
  return { userId: session.userId, name: session.email, email: session.email.toLowerCase(), assignmentId: `customer-portal:${binding.id}`, role: "external_customer", companyId: binding.company_id, branchId: null, departmentId: null, holdingId: session.holdingId, holdingSlug: session.holdingSlug, holdingName: session.holdingName, scope: { kind: "holding", holdingId: session.holdingId, holdingSlug: session.holdingSlug, holdingName: session.holdingName, companyId: binding.company_id }, assignmentScope: "holding", inherited: false, modules: [], assignments: [], workspaces: [] };
}
