import type { Clock, IdentifierSource } from "../../shared/application/ports";
import type { StoredFile } from "../../runtime/public/runtime/types";
import type { CustomerPortalBindingScope, CustomerPortalSessionActor, CustomerSharedFile } from "../domain/portal";

export type SharedPreview = { company_id: string; link_id: string; attachment_id: string; preview_object_key: string; preview_media_type: string };
export type PortalAttachmentRepository = {
  binding(actor: Pick<CustomerPortalSessionActor, "userId" | "email">, id: string, now: number): Promise<CustomerPortalBindingScope | null>;
  target(kind: "set" | "link" | "upload", id: string): Promise<string | null>;
  sharedFiles(actor: Pick<CustomerPortalSessionActor, "userId" | "email">, ids: string[], now: number): Promise<{ binding_id: string; link_id: string; label: string }[]>;
  preview(actor: Pick<CustomerPortalSessionActor, "userId" | "email">, linkId: string, now: number): Promise<SharedPreview | null>;
  access(event: { id: string; companyId: string; attachmentId: string; linkId: string; userId: string; now: number }): Promise<void>;
};
export function createCustomerPortalAttachmentApplication(ports: { repository: PortalAttachmentRepository; clock: Clock; identifiers: IdentifierSource; files: { get(key: string): Promise<StoredFile | null> }; error(status: number, code: string, message: string): Error }) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  async function customerPortalBindingScope(actor: Pick<CustomerPortalSessionActor, "userId" | "email">, id: string) {
    const binding = await ports.repository.binding(actor, id, ports.clock.now());
    if (!binding) return fail(404, "CUSTOMER_PORTAL_ATTACHMENT_UNAVAILABLE", "This customer file area is unavailable.");
    return binding;
  }
  async function customerPortalAttachmentTarget(kind: "set" | "link" | "upload", id: string) {
    const target = await ports.repository.target(kind, id);
    if (!target) return fail(404, "CUSTOMER_PORTAL_ATTACHMENT_UNAVAILABLE", "This customer file is unavailable.");
    return target;
  }
  async function customerPortalSharedFiles(actor: Pick<CustomerPortalSessionActor, "userId" | "email">, ids: string[]) {
    const result = new Map<string, CustomerSharedFile[]>();
    if (!ids.length) return result;
    for (const row of await ports.repository.sharedFiles(actor, ids, ports.clock.now())) {
      const files = result.get(row.binding_id) ?? [];
      files.push({ linkId: row.link_id, label: row.label, previewAvailable: true });
      result.set(row.binding_id, files);
    }
    return result;
  }
  async function customerPortalSharedPreview(actor: Pick<CustomerPortalSessionActor, "userId" | "email">, linkId: string) {
    const row = await ports.repository.preview(actor, linkId, ports.clock.now());
    if (!row) return fail(404, "CUSTOMER_SHARED_FILE_NOT_FOUND", "Shared file not found.");
    const object = await ports.files.get(row.preview_object_key);
    if (!object) return fail(503, "FILE_UNAVAILABLE", "The shared preview is temporarily unavailable.");
    await ports.repository.access({ id: `portal-attachment-access-${ports.identifiers.next()}`, companyId: row.company_id, attachmentId: row.attachment_id, linkId: row.link_id, userId: actor.userId, now: ports.clock.now() });
    return { object, mediaType: row.preview_media_type };
  }
  return { customerPortalBindingScope, customerPortalAttachmentTarget, customerPortalSharedFiles, customerPortalSharedPreview };
}
