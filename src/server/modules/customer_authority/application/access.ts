import type { Clock } from "../../shared/application/ports";
import type { Row } from "../../commerce/public/commerce";
import type { WorkItem } from "@/lib/work_projection/work-item";
import { customerAccessRequestSchema, customerAccessChangeSchema, type AccessRow, type AccessRequest, type AccessChange } from "../domain/access";
import { isCustomerPortalInvitableOrderState } from "../domain/portal-invitation";
export type AccessCommand<Input = AccessRequest | AccessChange> = { key: string; operation: string; id: string; input: Input };
export type AccessRepository = {
  binding(id: string): Promise<Row | null>; rows(): Promise<Row[]>; sources(): Promise<Row[]>;
  replay(command: AccessCommand): Promise<Record<string, unknown> | null>;
  order(id: string): Promise<Awaited<ReturnType<typeof import("../../commerce/public/commerce").getOrder>>["order"]>;
  request(command: AccessCommand<AccessRequest>, order: Awaited<ReturnType<AccessRepository["order"]>>, now: number): Promise<Record<string, unknown>>;
  change(command: AccessCommand<AccessChange>, row: AccessRow, facts: { review: boolean; status: string; now: number }): Promise<Record<string, unknown>>;
  events(id: string): Promise<Row[]>; reads(id: string): Promise<Row[]>;
};
export function createCustomerAccessApplication(ports: { actorId: string; require(permission: "commerce.read" | "commerce.sales" | "commerce.approve"): void; canApprove(): boolean; canSales(): boolean; canRead(): boolean; repository(): Promise<AccessRepository>; clock: Clock; error(status: number, code: string, message: string): Error }) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  const map = (row: Row): AccessRow => ({ ...row, canReview: row.status === "pending" && Number(row.actor_eligible) > 0 && ports.canApprove(), canRevoke: ["pending", "approved"].includes(String(row.status)) && (row.created_by === ports.actorId || ports.canApprove()), canInvite: row.status === "approved" && Number(row.expires_at) > ports.clock.now() && isCustomerPortalInvitableOrderState(String(row.order_state ?? "")) && (row.created_by === ports.actorId || ports.canApprove()) }) as unknown as AccessRow;
  async function customerAccessBinding(id: string) { ports.require("commerce.read"); ports.require("commerce.sales"); const row = await (await ports.repository()).binding(id); if (!row) return fail(404, "CUSTOMER_ACCESS_NOT_FOUND", "Customer access request not found in this scope."); return map(row); }
  async function customerAccessWorkspace() { ports.require("commerce.read"); ports.require("commerce.sales"); const source = await ports.repository(), rows = await source.rows(), sources = await source.sources(); return { rows: rows.map(map), sources }; }
  async function requestCustomerAccess(raw: unknown, key: string) {
    ports.require("commerce.sales"); ports.require("commerce.read"); const input = customerAccessRequestSchema.parse(raw), source = await ports.repository(), command = { key, operation: "customer.access.request", id: `customer-access-${key}`, input }, prior = await source.replay(command);
    if (prior) return prior;
    const now = ports.clock.now(); if (input.expiresAt <= now || input.expiresAt > now + 90 * 86400000) return fail(422, "CUSTOMER_ACCESS_EXPIRY", "Choose an expiry within the next 90 days.");
    return source.request(command, await source.order(input.orderId), now);
  }
  async function changeCustomerAccess(id: string, raw: unknown, key: string) {
    ports.require("commerce.sales"); ports.require("commerce.read"); const input = customerAccessChangeSchema.parse(raw), source = await ports.repository(), command = { key, operation: `customer.access.${input.action}`, id, input }, prior = await source.replay(command);
    if (prior) return prior;
    const row = await customerAccessBinding(id), review = input.action !== "revoke";
    if (review) { ports.require("commerce.approve"); if (row.created_by === ports.actorId) return fail(403, "INDEPENDENT_APPROVER_REQUIRED", "Another authorized sales manager must review customer access."); }
    else if (row.created_by !== ports.actorId && !ports.canApprove()) return fail(403, "FORBIDDEN", "Only the requester or a sales manager may revoke this access.");
    return source.change(command, row, { review, now: ports.clock.now(), status: input.action === "approve" ? "approved" : input.action === "reject" ? "rejected" : "revoked" });
  }
  async function customerAccessHistory(id: string) { const binding = await customerAccessBinding(id), source = await ports.repository(); return { binding, events: await source.events(id), reads: await source.reads(id) }; }
  async function customerAccessWork(): Promise<WorkItem[]> {
    if (!ports.canSales() || !ports.canRead()) return [];
    const { rows } = await customerAccessWorkspace();
    return rows.map(row => { const pending = row.status === "pending" && Number(row.expires_at) > ports.clock.now(); return { id: String(row.id), type: "customer_access", title: `Customer access · ${row.number}`, detail: "Verify the recipient and authority evidence before sharing this source.", href: `/customer-access/${encodeURIComponent(String(row.id))}`, status: Number(row.expires_at) <= ports.clock.now() ? "expired" : String(row.status), updatedAt: Number(row.updated_at), queue: pending ? row.canReview ? "todo" : "waiting" : "completed", canAct: pending && row.canReview, responsibleQueue: "Independent sales manager", nextAction: pending ? "Review or revoke the access request" : "Read retained access evidence", blocker: pending && !Number(row.reviewers) ? "No eligible independent sales manager; administrator coverage is required" : null, relationships: [row.created_by === ports.actorId ? "requested" : row.canReview ? "approval" : "team"] }; });
  }
  return { customerAccessBinding, customerAccessWorkspace, requestCustomerAccess, changeCustomerAccess, customerAccessHistory, customerAccessWork };
}
