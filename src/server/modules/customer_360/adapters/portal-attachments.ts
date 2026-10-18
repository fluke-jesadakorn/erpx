import "server-only";
import { currentSession } from "../../identity_access/public/authorization";
import { ApiError } from "../../runtime/public/http";
import { holdingBindingBySlug, holdingResources } from "../../runtime/public/runtime";
import type { FileStore, SqlDatabase } from "../../runtime/public/runtime/types";
import { readCustomerPortalBindingScope } from "../../customer_authority/public/portal-access";
import { customerPortalFileTarget, customerPortalSharedFileRows, customerPortalPreviewRow, recordCustomerPortalFileAccess } from "../../files/public/customer-portal-files";
import {withCustomerPortalSharedPreviewRead} from './shared-preview-read';
import { createCustomerPortalAttachmentApplication } from "../application/portal-attachments";
import { createCustomerPortalSessionApplication } from "../application/portal-session";
import { customerPortalWorkContext, type CustomerPortalSessionActor } from "../domain/portal";

export function portalAttachmentApplication(db: SqlDatabase, files?: FileStore) {
  return createCustomerPortalAttachmentApplication({ clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, error: (status, code, message) => new ApiError(status, code, message), files: { get: key => { if (!files) throw new Error("CUSTOMER_PORTAL_FILE_STORE_REQUIRED"); return files.get(key); } }, repository: {
    binding: (actor, id, now) => readCustomerPortalBindingScope(db, actor, id, now),
    target: (kind, id) => customerPortalFileTarget(db, kind, id),
    sharedFiles: (actor, ids, now) => customerPortalSharedFileRows(db, actor, ids, now),
    preview: (actor, id, now) => customerPortalPreviewRow(db, actor, id, now),
    access: event => recordCustomerPortalFileAccess(db, event),
  } });
}
export function customerPortalBindingScope(db: SqlDatabase, actor: Pick<CustomerPortalSessionActor, "userId" | "email">, bindingId: string) { return portalAttachmentApplication(db).customerPortalBindingScope(actor, bindingId); }
export function customerPortalAttachmentTarget(db: SqlDatabase, kind: "set" | "link" | "upload", id: string) { return portalAttachmentApplication(db).customerPortalAttachmentTarget(kind, id); }
export function customerPortalSharedFiles(db: SqlDatabase, actor: Pick<CustomerPortalSessionActor, "userId" | "email">, ids: string[]) { return portalAttachmentApplication(db).customerPortalSharedFiles(actor, ids); }
export function customerPortalSharedPreview(db: SqlDatabase, files: FileStore, actor: Pick<CustomerPortalSessionActor, "userId" | "email">, id: string,assertRequestAuthority?:()=>Promise<void>) { return withCustomerPortalSharedPreviewRead(db,actor,id,tx=>portalAttachmentApplication(tx,files).customerPortalSharedPreview(actor,id),assertRequestAuthority); }
export async function customerPortalAttachmentSession(holdingSlug: string) {
  const { session } = await currentSession(), application = createCustomerPortalSessionApplication({ error: (status, code, message) => new ApiError(status, code, message) }), user = application.verify(session.user), holding = await holdingBindingBySlug(holdingSlug), resources = await holdingResources(holding.id);
  return { ...resources, actor: application.actor(user, holding) };
}
export async function customerPortalAttachmentContext(holdingSlug: string, bindingId: string) {
  const context = await customerPortalAttachmentSession(holdingSlug), binding = await customerPortalBindingScope(context.write, context.actor, bindingId);
  return { ...context, binding, attachmentActor: customerPortalWorkContext(context.actor, binding) };
}
