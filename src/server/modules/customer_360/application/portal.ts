import type { Clock, IdentifierSource } from "../../shared/application/ports";
import type { WorkContext } from "../../identity_access/public/authorization";
import { portalCommandSchema, portalActionResult, customerPortalWorkContext, type PortalBinding, type PortalActionRow, type CustomerPortalSessionActor, type CustomerPortalItem, type CustomerSharedFile, type PortalCommandInput } from "../domain/portal";

export type PortalEvidence = { manifest: unknown[]; commit(): Promise<void> };
export type PortalCommand = { input: PortalCommandInput; key: string; id: string; now: number; requestHash: string; actor: CustomerPortalSessionActor; binding: PortalBinding };
export type PortalRepository = {
  transaction<Result>(action: (repository: PortalRepository) => Promise<Result>): Promise<Result>;
  projection(actor: { userId: string; email: string }, now: number, readId: string): Promise<Record<string, unknown>[]>;
  sharedFiles(actor: { userId: string; email: string }, ids: string[]): Promise<Map<string, CustomerSharedFile[]>>;
  replay(key: string): Promise<PortalActionRow | null>;
  binding(id: string, email: string): Promise<PortalBinding | null>;
  identity(bindingId: string, actor: CustomerPortalSessionActor): Promise<boolean>;
  authorize?(actor:CustomerPortalSessionActor,binding:PortalBinding,purpose:'settle-existing'|'history'):Promise<()=>Promise<void>>;
  existing(bindingId: string, action: string): Promise<PortalActionRow | null>;
  evidence(command: PortalCommand, actor: WorkContext): Promise<PortalEvidence>;
  record(command: PortalCommand, evidence: PortalEvidence | null): Promise<void>;
  notify(command: PortalCommand, message: string): Promise<void>;
  audit(command: PortalCommand, eventId: string): Promise<void>;
  outbox(command: PortalCommand, eventId: string, manifest: unknown[]): Promise<void>;
  uniqueConflict(error: unknown): boolean;
};
export function createCustomerPortalApplication(ports: { repository: PortalRepository; clock: Clock; identifiers: IdentifierSource; hash(value: string): string; error(status: number, code: string, message: string): Error }) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  function replayResult(prior: PortalActionRow, actor: CustomerPortalSessionActor, requestHash: string) {
    if (prior.user_id !== actor.userId || prior.request_hash !== requestHash) return fail(409, "IDEMPOTENCY_CONFLICT", "This command key belongs to a different request.");
    return portalActionResult(prior);
  }
  async function customerPortalProjection(actor: { userId: string; email: string }) {
    const now = ports.clock.now(), readId = `customer-read-${ports.identifiers.next()}`, rows = await ports.repository.projection(actor, now, readId);
    const shared = await ports.repository.sharedFiles(actor, rows.map(row => String(row.binding_id)));
    const items = rows.map(row => ({ ...row, fulfillments: typeof row.fulfillments === "string" ? JSON.parse(row.fulfillments) : row.fulfillments, actions: typeof row.actions === "string" ? JSON.parse(row.actions) : row.actions, sharedFiles: shared.get(String(row.binding_id)) ?? [] }) as CustomerPortalItem);
    return { items, projectionVersion: "customer-order-v1" };
  }
  async function customerPortalCommand(actor: CustomerPortalSessionActor, raw: unknown, key: string) {
    if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key)) return fail(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.");
    const input = portalCommandSchema.parse(raw), requestHash = ports.hash(JSON.stringify({ actor: actor.userId, email: actor.email, input })), id = `customer-portal-action-${ports.identifiers.next()}`, now = ports.clock.now();
    const execute=async(repository:PortalRepository)=>{
        const prior = await repository.replay(key);
        const binding = await repository.binding(input.bindingId, actor.email.toLowerCase());
        if (!binding) return fail(403, "CUSTOMER_PORTAL_BINDING_UNAVAILABLE", "This customer document is not available to your account.");
        if (binding.status !== "approved" || Number(binding.expires_at) <= ports.clock.now()) return fail(403, "CUSTOMER_PORTAL_BINDING_EXPIRED", "This customer access has expired or was revoked.");
        if (!prior&&Number(binding.binding_version) !== input.expectedBindingVersion) return fail(409, "CUSTOMER_PORTAL_BINDING_CHANGED", "The customer document access changed. Refresh before submitting.");
        if (!["confirmed", "converted", "partially_fulfilled", "fulfilled"].includes(binding.state)) return fail(409, "CUSTOMER_PORTAL_SOURCE_UNAVAILABLE", "This customer document is no longer available for portal actions.");
        if (!await repository.identity(binding.binding_id, actor)) return fail(403, "CUSTOMER_PORTAL_IDENTITY_REQUIRED", "Open the approved document once before submitting a customer action.");
        const finish='authorize' in repository&&repository.authorize?await repository.authorize(actor,binding,prior?'history':'settle-existing'):null;
        if (prior) {const result=replayResult(prior,actor,requestHash);await finish?.();return result;}
        if (await repository.existing(binding.binding_id, input.action)) return fail(409, "CUSTOMER_PORTAL_ACTION_ALREADY_RECORDED", "This customer action has already been recorded.");
        if (input.action !== "request_support" && input.attachmentSetIds.length) return fail(422, "CUSTOMER_PORTAL_ATTACHMENT_ACTION", "Files may be attached only to a support request.");
        if (input.attachmentSetIds.length && (!actor.holdingId || !actor.holdingSlug || !actor.holdingName)) return fail(503, "CUSTOMER_PORTAL_ATTACHMENT_CONTEXT", "Reload the customer portal before attaching files.");
        const command = { input, key, id, now, requestHash, actor, binding };
        const attachmentActor = input.attachmentSetIds.length ? customerPortalWorkContext(actor as Required<CustomerPortalSessionActor>, { id: binding.binding_id, company_id: binding.company_id, branch_id: null, department_id: null, version: Number(binding.binding_version) }) : null;
        const evidence = attachmentActor ? await repository.evidence(command, attachmentActor) : null;
        await repository.record(command, evidence);
        await repository.notify(command, input.action === "request_support" ? "Customer requested support on an order; review and contact the customer." : "Customer acknowledged delivery; review the order and fulfillment record.");
        await repository.audit(command, ports.identifiers.next());
        await repository.outbox(command, ports.identifiers.next(), evidence?.manifest ?? []);
        await finish?.();
        return { id, bindingId: binding.binding_id, orderId: binding.order_id, action: input.action, status: "recorded", createdAt: now };
    };
    try {
      return await ports.repository.transaction(execute);
    } catch (error) {
      if (ports.repository.uniqueConflict(error)) {
        // A raced receipt still enters the original source transaction and
        // reauthorizes the current binding and verified account identity.
        return ports.repository.transaction(execute);
      }
      throw error;
    }
  }
  return { customerPortalProjection, customerPortalCommand };
}
