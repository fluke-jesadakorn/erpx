import type { Clock } from "@erpx/server-shared/application/ports";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { duplicateChangeSchema, type DuplicateChange, type DuplicateRow } from "@erpx/server-customer_authority/domain/duplicates";
export type DuplicateCommand = { key: string; operation: string; id: string; input: DuplicateChange };
export type DuplicateRepository = {
  rows(): Promise<Row[]>; customers(): Promise<Row[]>; history(id: string): Promise<Row[]>;
  replay(command: DuplicateCommand): Promise<Record<string, unknown> | null>;
  blockers(loser: string): Promise<{ notifications: boolean; grants: boolean; agreements: boolean; billing: boolean; orderEvidence: boolean }>;
  parties(canonical: string, loser: string): Promise<Row | null>; currency(id: string): Promise<string | null>;
  merge(command: DuplicateCommand, facts: { canonical: string; loser: string; now: number }): Promise<Record<string, unknown>>;
  change(command: DuplicateCommand, facts: { status: string; canonical: string | null; from: string; now: number }): Promise<Record<string, unknown>>;
};
export function createCustomerDuplicateApplication(ports: { actorId: string; companyScope: boolean; require(permission: "commerce.read" | "commerce.sales" | "commerce.approve"): void; canApprove(): boolean; canSales(): boolean; canRead(): boolean; repository(): Promise<DuplicateRepository>; clock: Clock; error(status: number, code: string, message: string): Error }) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function customerDuplicateWorkspace() { ports.require("commerce.read"); ports.require("commerce.sales"); const source = await ports.repository(), rows = await source.rows(), customers = await source.customers(), mapped = rows.map(row => { const eligible = Number(row.actor_eligible) > 0 && ports.canApprove(), independent = row.created_by !== ports.actorId, active = String(row.left_status) === "active" && String(row.right_status) === "active" && !row.left_merged_into_id && !row.right_merged_into_id; return { ...row, canReview: row.status === "pending" && independent && eligible, canReopen: row.status !== "pending" && active && eligible, canMerge: row.status === "duplicate" && active && independent && eligible } as DuplicateRow; }); return { rows: mapped, customers, canCreate: ports.companyScope }; }
  async function customerDuplicateDetail(id: string) { const data = await customerDuplicateWorkspace(), row = data.rows.find(row => row.id === id); if (!row) return fail(404, "CUSTOMER_DUPLICATE_NOT_FOUND", "Duplicate review not found in this Company."); return { row, history: await (await ports.repository()).history(id) }; }
  async function changeCustomerDuplicate(id: string, raw: unknown, key: string) {
    ports.require("commerce.approve"); ports.require("commerce.read"); ports.require("commerce.sales"); if (!ports.companyScope) return fail(403, "CUSTOMER_COMPANY_REVIEW_REQUIRED", "Use a Company assignment for identity review.");
    const input = duplicateChangeSchema.parse(raw), source = await ports.repository(), command = { key, operation: `customer.duplicate.${input.action}`, id, input }, prior = await source.replay(command);
    if (prior) return prior;
    const { row } = await customerDuplicateDetail(id); if (input.action !== "reopen" && row.created_by === ports.actorId) return fail(403, "INDEPENDENT_APPROVER_REQUIRED", "Another manager must decide whether these customers are duplicates.");
    const now = ports.clock.now();
    if (input.action === "merge") {
      if (String(row.status) !== "duplicate" || !row.canonical_customer_id) return fail(409, "CUSTOMER_MERGE_REQUIRES_DUPLICATE", "Confirm the duplicate decision before merging customer records.");
      const canonical = String(row.canonical_customer_id), loser = canonical === String(row.left_customer_id) ? String(row.right_customer_id) : String(row.left_customer_id), blockers = await source.blockers(loser);
      const blocker = blockers.notifications ? "Customer notifications retain immutable recipient snapshots." : blockers.grants ? "Customer access bindings retain immutable source scope." : blockers.agreements ? "Field Service agreements retain immutable customer scope." : blockers.billing ? "Field Service billing releases retain immutable customer scope." : blockers.orderEvidence ? "Customer orders with source evidence must be corrected by their owner workflow first." : null;
      if (blocker) return fail(409, "CUSTOMER_MERGE_BLOCKED", blocker);
      const party = await source.parties(canonical, loser);
      if (!party || party.left_status !== "active" || party.right_status !== "active" || party.left_merged_into_id || party.right_merged_into_id || String(party.currency) !== String(await source.currency(canonical))) return fail(409, "CUSTOMER_MERGE_SOURCE_CHANGED", "Both active customer records must remain in the same company and currency before merge.");
      return source.merge(command, { canonical, loser, now });
    }
    return source.change(command, { status: input.action === "reopen" ? "pending" : input.action, canonical: input.action === "duplicate" ? input.canonicalCustomerId : null, from: input.action === "reopen" ? String(row.status) : "pending", now });
  }
  async function customerDuplicateWork(): Promise<WorkItem[]> { if (!ports.canSales() || !ports.canRead()) return []; const { rows } = await customerDuplicateWorkspace(); return rows.map(row => { const canAct = Boolean(row.canReview || row.canMerge); return { id: String(row.id), type: "customer_duplicate", actionKind: row.status === "pending" && row.canReview ? "approval" : "work", title: `Duplicate review · ${row.left_name} / ${row.right_name}`, detail: "Compare the customer records and record an explainable decision.", href: `/customer-duplicates/${encodeURIComponent(String(row.id))}`, status: String(row.status), updatedAt: Number(row.updated_at), queue: canAct ? "todo" : row.status === "pending" ? "waiting" : "completed", canAct, responsibleQueue: "Independent Company sales manager", nextAction: row.status === "pending" ? "Decide duplicate or distinct" : row.canMerge ? "Merge eligible source records into the canonical customer" : "Read retained identity decision", blocker: row.status === "pending" && !row.canReview ? "Awaiting an eligible independent Company sales manager" : null, relationships: [row.created_by === ports.actorId ? "requested" : canAct ? "approval" : "team"] }; }); }
  return { customerDuplicateWorkspace, customerDuplicateDetail, changeCustomerDuplicate, customerDuplicateWork };
}
