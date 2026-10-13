import type { Clock } from "../../shared/application/ports";
import type { Row } from "../../commerce/public/commerce";
import type { WorkItem } from "@/lib/work_projection/work-item";
import { identityRequestSchema, identityChangeSchema, type ChangeRow, type IdentityRequest, type IdentityChange, type CustomerIdentity } from "../domain/identity";
export type IdentityCommand<Input = IdentityRequest | IdentityChange> = { key: string; operation: string; id: string; input: Input };
export type IdentityRepository = {
  rows(): Promise<Row[]>; customers(): Promise<Row[]>; history(id: string): Promise<Row[]>;
  customer(id: string): Promise<Row | null>; replay(command: IdentityCommand): Promise<Record<string, unknown> | null>;
  request(command: IdentityCommand<IdentityRequest>, previous: CustomerIdentity, now: number): Promise<Record<string, unknown>>;
  change(command: IdentityCommand<IdentityChange>, row: ChangeRow, facts: { proposed: CustomerIdentity; customerVersion: number; status: string; now: number }): Promise<Record<string, unknown>>;
};
export function createCustomerIdentityApplication(ports: { actorId: string; companyScope: boolean; require(permission: "commerce.read" | "commerce.sales" | "commerce.approve"): void; canApprove(): boolean; canSales(): boolean; canRead(): boolean; repository(): Promise<IdentityRepository>; clock: Clock; error(status: number, code: string, message: string): Error }) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function customerIdentityWorkspace() { ports.require("commerce.read"); ports.require("commerce.sales"); const source = await ports.repository(), raw = await source.rows(), rows = raw.map(row => ({ ...row, canReview: row.status === "pending" && row.created_by !== ports.actorId && Number(row.actor_eligible) > 0 && ports.canApprove(), canResubmit: row.status === "rejected" && (row.created_by === ports.actorId || ports.canApprove()) }) as ChangeRow), customers = await source.customers(); return { rows, customers, canCreate: ports.companyScope }; }
  async function customerIdentityDetail(id: string) { const data = await customerIdentityWorkspace(), row = data.rows.find(row => row.id === id); if (!row) return fail(404, "CUSTOMER_IDENTITY_CHANGE_NOT_FOUND", "Identity change not found in this Company."); return { row, history: await (await ports.repository()).history(id) }; }
  async function requestCustomerIdentityChange(raw: unknown, key: string) {
    ports.require("commerce.sales"); ports.require("commerce.read"); if (!ports.companyScope) return fail(403, "CUSTOMER_COMPANY_REVIEW_REQUIRED", "Use a Company assignment for identity changes.");
    const input = identityRequestSchema.parse(raw), source = await ports.repository(), command = { key, operation: "customer.identity.request", id: `customer-identity-${key}`, input }, prior = await source.replay(command);
    if (prior) return prior;
    const customer = await source.customer(input.customerId); if (!customer) return fail(404, "CUSTOMER_NOT_FOUND", "Active customer not found.");
    const rawAddress = customer.address_json && typeof customer.address_json === "object" ? customer.address_json : typeof customer.address_json === "string" ? JSON.parse(String(customer.address_json)) : {};
    const previous: CustomerIdentity = { name: String(customer.name), taxRegistration: customer.tax_registration ? String(customer.tax_registration) : null, phone: customer.phone ? String(customer.phone) : null, address: Object.keys(rawAddress).length ? rawAddress : null, notificationEmail: customer.notification_email ? String(customer.notification_email) : null, notificationConsent: String(customer.notification_consent ?? "unknown") as CustomerIdentity["notificationConsent"] };
    if (JSON.stringify(previous) === JSON.stringify(input.proposed)) return fail(422, "CUSTOMER_IDENTITY_UNCHANGED", "Propose a different customer identity value.");
    return source.request(command, previous, ports.clock.now());
  }
  async function changeCustomerIdentity(id: string, raw: unknown, key: string) {
    ports.require("commerce.sales"); ports.require("commerce.read"); const input = identityChangeSchema.parse(raw), source = await ports.repository(), command = { key, operation: `customer.identity.${input.action}`, id, input }, prior = await source.replay(command);
    if (prior) return prior;
    const { row } = await customerIdentityDetail(id);
    if (input.action !== "resubmit") { ports.require("commerce.approve"); if (row.created_by === ports.actorId) return fail(403, "INDEPENDENT_APPROVER_REQUIRED", "Another manager must review this identity change."); }
    else if (row.created_by !== ports.actorId && !ports.canApprove()) return fail(403, "FORBIDDEN", "Only the requester or a sales manager may resubmit.");
    const proposed: CustomerIdentity = input.action === "resubmit" ? input.proposed : JSON.parse(String(row.proposed_json)), customerVersion = input.action === "resubmit" ? input.expectedCustomerVersion : Number(row.expected_customer_version), status = input.action === "apply" ? "applied" : input.action === "reject" ? "rejected" : "pending";
    return source.change(command, row, { proposed, customerVersion, status, now: ports.clock.now() });
  }
  async function customerIdentityWork(): Promise<WorkItem[]> { if (!ports.canSales() || !ports.canRead()) return []; const { rows } = await customerIdentityWorkspace(); return rows.map(row => ({ id: String(row.id), type: "customer_identity", title: `Identity change · ${row.customer_name}`, detail: "Review the before/after identity snapshot before changing the customer master.", href: `/customer-identity/${encodeURIComponent(String(row.id))}`, status: String(row.status), updatedAt: Number(row.updated_at), queue: row.status === "pending" ? row.canReview ? "todo" : "waiting" : "completed", canAct: Boolean(row.canReview), responsibleQueue: "Independent Company sales manager", nextAction: row.status === "pending" ? "Review the identity change" : "Read retained identity evidence", blocker: row.status === "pending" && !row.canReview ? "Awaiting an eligible independent Company sales manager" : null, relationships: [row.created_by === ports.actorId ? "requested" : row.canReview ? "approval" : "team"] })); }
  return { customerIdentityWorkspace, customerIdentityDetail, requestCustomerIdentityChange, changeCustomerIdentity, customerIdentityWork };
}
