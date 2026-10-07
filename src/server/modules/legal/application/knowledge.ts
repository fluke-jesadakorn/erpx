import { z } from "zod";
import type { Clock, IdentifierSource } from "../../shared/application/ports";
import type { DocumentVersion, LegalItem } from "../domain/model";
import { dateSchema } from "../domain/model";
import type { LegalEvidenceSource, LegalAiEvidence } from "../domain/knowledge";
export type LegalKnowledgeRepository = {
  lawSources(query: string, asOf: string): Promise<LegalEvidenceSource[]>; lawConflicts(asOf: string): Promise<number>;
  lawSource(id: string, asOf: string): Promise<LegalEvidenceSource | null>;
  reviewed(query: string): Promise<LegalEvidenceSource[]>; authored(query: string): Promise<LegalEvidenceSource[]>; authoredItem(itemId: string): Promise<LegalEvidenceSource[]>;
  documentAvailable(id: string): Promise<boolean>; textApproved(reviewId: string, documentId: string): Promise<boolean>;
  recordReads(rows: { item_id: string; document_id: string; identifier: string; now: number; action: string }[]): Promise<void>;
};
export function createLegalKnowledgeApplication(ports: { require(): void; access(id: string): Promise<LegalItem>; document(id: string): Promise<DocumentVersion>; repository: LegalKnowledgeRepository; clock: Clock; identifiers: IdentifierSource; error(status: number, code: string, message: string): Error }) {
  const repository = ports.repository, fail = (message: string, status = 404): never => { throw ports.error(status, "LEGAL_CITATION_UNAVAILABLE", message); };
  function asOfDate(raw?: unknown) { return raw == null ? new Date(ports.clock.now() + 7 * 3600000).toISOString().slice(0, 10) : dateSchema.parse(raw); }
  async function recordReads(sources: LegalEvidenceSource[], action: string) {
    const unique = [...new Map(sources.map(source => [source.document_id, source])).values()];
    if (unique.length) await repository.recordReads(unique.map(source => ({ item_id: source.item_id, document_id: source.document_id, identifier: ports.identifiers.next(), now: ports.clock.now(), action })));
  }
  async function lawKnowledge(query: unknown, rawAsOf?: unknown) {
    ports.require(); const q = z.string().trim().min(2).max(200).parse(query), asOf = asOfDate(rawAsOf), sources = await repository.lawSources(q, asOf), conflicts = await repository.lawConflicts(asOf);
    await recordReads(sources, "law.knowledge.read");
    return { query: q, asOf, sources, conflicts: Number(conflicts), notice: Number(conflicts) ? "Overlapping approved law revisions need independent review. Conflicting sources are excluded; no legal conclusion can be inferred for them." : sources.length ? "Only independently approved text and Company applicability effective on the selected Gregorian date are shown. Verify that each cited provision supports the conclusion." : "No approved applicable official-law source matched this date. Evidence is insufficient for a legal conclusion; request the official original and an independent applicability review." };
  }
  async function lawCitation(chunkId: string, rawAsOf?: unknown) {
    ports.require(); const asOf = asOfDate(rawAsOf), id = z.string().min(1).max(200).parse(chunkId).replace(/^law:/, ""), source = await repository.lawSource(id, asOf);
    if (!source) return fail("The cited law source is unavailable, unapproved, conflicting or not applicable on this date.");
    await recordReads([source], "law.citation.read");
    return { ...source, asOf, href: `/legal?record=${encodeURIComponent(source.item_id)}`, originalUrl: `${source.source_url}${source.page && source.source_url?.endsWith(".pdf") ? `#page=${source.page}` : ""}`, notice: "The quotation and original version are retained for review. Legal interpretation still requires an independent legal reviewer." };
  }
  async function legalKnowledge(query: unknown, rawAsOf?: unknown) {
    ports.require(); const q = z.string().trim().min(2).max(200).parse(query), reviewed = await repository.reviewed(q), authored = await repository.authored(q), laws = await lawKnowledge(q, rawAsOf), documents = [...reviewed, ...authored];
    await recordReads(documents, "knowledge.read"); const sources = [...documents, ...laws.sources];
    return { query: q, asOf: laws.asOf, sources, conflicts: laws.conflicts, notice: sources.length ? laws.conflicts ? laws.notice : "Review each cited immutable version and exact page or section before relying on its text. Extracted file text and official laws require independent review." : "No accessible reviewed source matched. Evidence is insufficient; request the missing documents or independently approved official law revision." };
  }
  async function retrieveLegalAiEvidence(itemId: string, question: string): Promise<LegalAiEvidence> {
    await ports.access(itemId); const result = await legalKnowledge(question.slice(0, 200)), authored = await repository.authoredItem(itemId), sources = [...new Map([...result.sources, ...authored].map(source => [source.sourceId, { ...source, text: source.text.slice(0, 12000), excerpt: source.excerpt.slice(0, 12000) }])).values()].slice(0, 20);
    await recordReads(authored, "ai.source.read");
    return { sources, asOf: result.asOf, conflicts: result.conflicts, insufficientEvidence: sources.length === 0 || result.conflicts > 0, notice: result.notice };
  }
  async function revalidateLegalAiEvidence(evidence: LegalAiEvidence) {
    for (const source of evidence.sources) {
      if (source.kind === "law") { const fresh = await lawCitation(source.id, evidence.asOf); if (fresh.checksum !== source.checksum || fresh.revision_id !== source.revision_id) fail("The law source changed during generation.", 409); }
      else {
        const document = await ports.document(source.document_id);
        if (document.checksum !== source.checksum) fail("A cited source changed.", 409);
        if (!await repository.documentAvailable(document.id)) fail("A cited source is no longer available.", 409);
        if (source.text_review_id && !await repository.textApproved(source.text_review_id, document.id)) fail("Extracted text is no longer approved.", 409);
      }
    }
  }
  return { lawKnowledge, lawCitation, legalKnowledge, retrieveLegalAiEvidence, revalidateLegalAiEvidence };
}
