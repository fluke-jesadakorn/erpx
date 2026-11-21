import type { Clock } from "@erpx/server-shared/application/ports";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { type ChangeRow } from "@erpx/server-customer_authority/domain/identity";
/** Retained Customer Authority identity-change read projections. The request
 * and review/resubmit commands are native-owned (`CustomerIdentityHttpCommand`);
 * this application only maps the workspace/detail/work reads used by presenters
 * and Work projection. */
export type IdentityRepository = {
  rows(): Promise<Row[]>; customers(): Promise<Row[]>; history(id: string): Promise<Row[]>;
};
export function createCustomerIdentityApplication(ports: { actorId: string; companyScope: boolean; require(permission: "commerce.read" | "commerce.sales" | "commerce.approve"): void; canApprove(): boolean; canSales(): boolean; canRead(): boolean; repository(): Promise<IdentityRepository>; clock: Clock; error(status: number, code: string, message: string): Error }) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function customerIdentityWorkspace() { ports.require("commerce.read"); ports.require("commerce.sales"); const source = await ports.repository(), raw = await source.rows(), rows = raw.map(row => ({ ...row, canReview: row.status === "pending" && row.created_by !== ports.actorId && Number(row.actor_eligible) > 0 && ports.canApprove(), canResubmit: row.status === "rejected" && (row.created_by === ports.actorId || ports.canApprove()) }) as ChangeRow), customers = await source.customers(); return { rows, customers, canCreate: ports.companyScope }; }
  async function customerIdentityDetail(id: string) { const data = await customerIdentityWorkspace(), row = data.rows.find(row => row.id === id); if (!row) return fail(404, "CUSTOMER_IDENTITY_CHANGE_NOT_FOUND", "Identity change not found in this Company."); return { row, history: await (await ports.repository()).history(id) }; }
  async function customerIdentityWork(): Promise<WorkItem[]> { if (!ports.canSales() || !ports.canRead()) return []; const { rows } = await customerIdentityWorkspace(); return rows.map(row => ({ id: String(row.id), type: "customer_identity", title: `Identity change · ${row.customer_name}`, detail: "Review the before/after identity snapshot before changing the customer master.", href: `/customer-identity/${encodeURIComponent(String(row.id))}`, status: String(row.status), updatedAt: Number(row.updated_at), queue: row.status === "pending" ? row.canReview ? "todo" : "waiting" : "completed", canAct: Boolean(row.canReview), responsibleQueue: "Independent Company sales manager", nextAction: row.status === "pending" ? "Review the identity change" : "Read retained identity evidence", blocker: row.status === "pending" && !row.canReview ? "Awaiting an eligible independent Company sales manager" : null, relationships: [row.created_by === ports.actorId ? "requested" : row.canReview ? "approval" : "team"] })); }
  return { customerIdentityWorkspace, customerIdentityDetail, customerIdentityWork };
}
