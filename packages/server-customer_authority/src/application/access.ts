import type { Clock } from "@erpx/server-shared/application/ports";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { type AccessRow } from "@erpx/server-customer_authority/domain/access";
import { isCustomerPortalInvitableOrderState } from "@erpx/server-customer_authority/domain/portal-invitation";
/**
 * Retained Customer Authority access read projections. The request/approve/
 * reject/revoke commands are native-owned; this application only maps the
 * bounded workspace/detail/history/work reads consumed by presenters, Work
 * projection and staff notifications.
 */
export type AccessRepository = {
  binding(id: string): Promise<Row | null>; rows(): Promise<Row[]>; sources(): Promise<Row[]>;
  events(id: string): Promise<Row[]>; reads(id: string): Promise<Row[]>;
};
export function createCustomerAccessApplication(ports: { actorId: string; require(permission: "commerce.read" | "commerce.sales" | "commerce.approve"): void; canApprove(): boolean; canSales(): boolean; canRead(): boolean; repository(): Promise<AccessRepository>; clock: Clock; error(status: number, code: string, message: string): Error }) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  const map = (row: Row): AccessRow => ({ ...row, canReview: row.status === "pending" && Number(row.actor_eligible) > 0 && ports.canApprove(), canRevoke: ["pending", "approved"].includes(String(row.status)) && (row.created_by === ports.actorId || ports.canApprove()), canInvite: row.status === "approved" && Number(row.expires_at) > ports.clock.now() && isCustomerPortalInvitableOrderState(String(row.order_state ?? "")) && (row.created_by === ports.actorId || ports.canApprove()) }) as unknown as AccessRow;
  async function customerAccessBinding(id: string) { ports.require("commerce.read"); ports.require("commerce.sales"); const row = await (await ports.repository()).binding(id); if (!row) return fail(404, "CUSTOMER_ACCESS_NOT_FOUND", "Customer access request not found in this scope."); return map(row); }
  async function customerAccessWorkspace() { ports.require("commerce.read"); ports.require("commerce.sales"); const source = await ports.repository(), rows = await source.rows(), sources = await source.sources(); return { rows: rows.map(map), sources }; }
  async function customerAccessHistory(id: string) { const binding = await customerAccessBinding(id), source = await ports.repository(); return { binding, events: await source.events(id), reads: await source.reads(id) }; }
  async function customerAccessWork(): Promise<WorkItem[]> {
    if (!ports.canSales() || !ports.canRead()) return [];
    const { rows } = await customerAccessWorkspace();
    return rows.map(row => { const pending = row.status === "pending" && Number(row.expires_at) > ports.clock.now(); return { id: String(row.id), type: "customer_access", title: `Customer access · ${row.number}`, detail: "Verify the recipient and authority evidence before sharing this source.", href: `/customer-access/${encodeURIComponent(String(row.id))}`, status: Number(row.expires_at) <= ports.clock.now() ? "expired" : String(row.status), updatedAt: Number(row.updated_at), queue: pending ? row.canReview ? "todo" : "waiting" : "completed", canAct: pending && row.canReview, responsibleQueue: "Independent sales manager", nextAction: pending ? "Review or revoke the access request" : "Read retained access evidence", blocker: pending && !Number(row.reviewers) ? "No eligible independent sales manager; administrator coverage is required" : null, relationships: [row.created_by === ports.actorId ? "requested" : row.canReview ? "approval" : "team"] }; });
  }
  return { customerAccessBinding, customerAccessWorkspace, customerAccessHistory, customerAccessWork };
}
