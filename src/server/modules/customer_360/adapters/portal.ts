import "server-only";
import type { SqlDatabase, TransactionalSqlDatabase } from "../../runtime/public/runtime/types";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import { ApiError } from "../../runtime/public/http";
import { hash } from "../../legal/public/legal/service";
import { projectCustomerPortalAccess, lockCustomerPortalBinding, customerPortalIdentityExists, prepareCustomerPortalEvidenceChange } from "../../customer_authority/public/portal-access";
import { prepareAttachmentBinding } from "../../files/public/attachments/service";
import { notifyCustomerPortalStaff } from "../../collaboration/public/customer-portal-notifications";
import { auditCustomerPortalCommand, enqueueCustomerPortalCommand, clearCustomerPortalAssertions } from "../../workflow/public/customer-portal-events";
import { createCustomerPortalApplication, type PortalRepository } from "../application/portal";
import type { CustomerPortalSessionActor, PortalActionRow } from "../domain/portal";
import { customerPortalAttachmentSession, customerPortalSharedFiles } from "./portal-attachments";

function portalRepository(db: SqlDatabase): PortalRepository {
  const action = (predicate: string, values: string[]) => db.prepare(`SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE ${predicate}`).bind(...values).first<PortalActionRow>();
  return {
    transaction: callback => {
      const transactional = db as Partial<TransactionalSqlDatabase>;
      if (!transactional.transaction) throw new ApiError(503, "CUSTOMER_PORTAL_TRANSACTION_REQUIRED", "Customer actions require a transaction.");
      return transactional.transaction(tx => callback(portalRepository(tx)));
    },
    projection: (actor, now, readId) => projectCustomerPortalAccess(db, actor, now, readId),
    sharedFiles: (actor, ids) => customerPortalSharedFiles(db, actor, ids),
    replay: key => action("request_key=?", [key]),
    existing: (id, actionName) => action("binding_id=? AND action=?", [id, actionName]),
    binding: (id, email) => lockCustomerPortalBinding(db, id, email),
    identity: (id, actor) => customerPortalIdentityExists(db, id, actor),
    evidence: async (command, actor) => {
      const { input, binding, key, id, now } = command;
      const prepared = await prepareAttachmentBinding(db, actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: "customer_access_binding", bindingType: "customer_portal_support_request", contexts: [{ purposeKey: "customer_support_evidence", componentKind: "action", componentRef: "request_support" }] }, targetId: binding.binding_id, expectedTargetVersion: Number(binding.binding_version), commandId: key, bindingId: id });
      return { manifest: prepared.manifest, commit: async () => { await db.batch([...prepared.guards, ...prepareCustomerPortalEvidenceChange(db, { now, bindingId: binding.binding_id, companyId: binding.company_id, version: Number(binding.binding_version), actorId: actor.userId, key }), ...prepared.steps, clearCustomerPortalAssertions(db, key)]); } };
    },
    record: async (command, evidence) => {
      const { id, input, binding, actor, key, requestHash, now } = command;
      await db.prepare(`INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'recorded',?)`).bind(id, binding.company_id, binding.binding_id, binding.order_id, binding.customer_id, actor.userId, input.action, input.note, Number(binding.order_version), key, requestHash, now).run();
      if (evidence) await evidence.commit();
    },
    notify: (command, message) => notifyCustomerPortalStaff(db, { id: command.id, message, now: command.now, orderId: command.binding.order_id, companyId: command.binding.company_id }),
    audit: (command, eventId) => auditCustomerPortalCommand(db, command, eventId),
    outbox: (command, eventId, manifest) => enqueueCustomerPortalCommand(db, command, eventId, manifest),
    uniqueConflict: error => (error as { code?: string })?.code === "23505",
  };
}
function portalApplication(db: SqlDatabase) { return createCustomerPortalApplication({ repository: portalRepository(db), clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, hash, error: (status, code, message) => new ApiError(status, code, message) }); }
export const customerPortalContext = customerPortalAttachmentSession;
export function customerPortalProjection(db: SqlDatabase, actor: { userId: string; email: string }) { return portalApplication(db).customerPortalProjection(actor); }
export async function customerPortalList(slug: string) { const { write, actor } = await customerPortalContext(slug); return customerPortalProjection(write, actor); }
export function customerPortalCommand(db: PostgresDatabase, actor: CustomerPortalSessionActor, raw: unknown, key: string) { return portalApplication(db).customerPortalCommand(actor, raw, key); }
