import type { Clock, IdentifierSource } from "@erpx/server-shared/application/ports";
import type { DocumentVersion, LegalItem } from "@erpx/server-legal/domain/model";
import type { LegalEvidenceSource } from "@erpx/server-legal/domain/knowledge";
export type LegalDocumentCitationChunk = { id: string; document_id: string; page: number | null; page_mapping: boolean; section: string; text: string; document_checksum: string };
export function createLegalCitationQuery(ports: {
  law(id: string, asOf?: unknown): Promise<LegalEvidenceSource & { asOf: string; href: string; originalUrl: string; notice: string }>;
  chunk(id: string): Promise<LegalDocumentCitationChunk | null>; document(id: string): Promise<DocumentVersion>; item(id: string): Promise<LegalItem>;
  available(documentId: string): Promise<boolean>; recordRead(itemId: string, documentId: string, identifier: string, now: number): Promise<void>;
  clock: Clock; identifiers: IdentifierSource; error(status: number, code: string, message: string): Error;
}) {
  return async (sourceId: string, asOf?: unknown) => {
    if (sourceId.startsWith("law:")) return ports.law(sourceId, asOf);
    const fail = (message: string): never => { throw ports.error(404, "LEGAL_CITATION_UNAVAILABLE", message); };
    const id = sourceId.replace(/^document:/, ""), chunk = await ports.chunk(id);
    if (sourceId.startsWith("document:") && !chunk) fail("The reviewed passage is unavailable.");
    const document = await ports.document(chunk?.document_id ?? id), item = await ports.item(document.item_id);
    if (!await ports.available(document.id)) fail("The cited original is unavailable or awaiting security review.");
    if (chunk && chunk.document_checksum !== document.checksum) fail("The source version changed.");
    if (!chunk && (document.media_type !== "text/plain" || document.object_key || item.kind === "rule")) fail("A reviewed source passage is required.");
    await ports.recordRead(item.id, document.id, ports.identifiers.next(), ports.clock.now());
    return { kind: "document" as const, title: item.title, name: document.name, item_id: item.id, document_id: document.id, version: document.version, checksum: document.checksum, page: chunk?.page ?? null, page_mapping: chunk?.page_mapping ?? false, section: chunk?.section ?? "", text: chunk?.text ?? document.body, notice: "Read this immutable original and verify that the cited passage supports the conclusion. Legal interpretation requires independent review." };
  };
}
