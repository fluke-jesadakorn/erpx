export type LegalEvidenceSource = {
  id: string;
  sourceId: string;
  kind: "law" | "document";
  item_id: string;
  title: string;
  document_id: string;
  version: number;
  checksum: string;
  text: string;
  excerpt: string;
  page: number | null;
  page_mapping?: boolean;
  section: string;
  revision_id?: string;
  revision_label?: string;
  source_url?: string;
  published_on?: string;
  effective_from?: string;
  effective_until?: string | null;
  text_review_id?: string;
  name?: string;
};
export type LegalAiEvidence={sources:LegalEvidenceSource[];asOf:string;conflicts:number;insufficientEvidence:boolean;notice:string};
type CitationClaim = { text: string; sources: string[]; quotes?: { sourceId: string; quote: string }[] };
/** Verify references and exact evidence quotation, without claiming automated legal entailment. */
export function validateLegalAiCitations<T extends { sources: string[]; claims: CitationClaim[] }>(proposal: T, evidence: LegalAiEvidence, error:(status:number,code:string,message:string)=>Error) {
  const fail=(message:string,code:string,status:number):never=>{throw error(status,code,message);};
  const sources = new Map(evidence.sources.map((source) => [source.sourceId, source]));
  if ([...proposal.sources, ...proposal.claims.flatMap((claim) => claim.sources)].some((id) => !sources.has(id))) fail("AI returned an inaccessible or unsupported source reference.", "LEGAL_AI_CITATION", 422);
  const claims = proposal.claims.map((claim) => {
    if (!claim.sources.length) return { ...claim, supportStatus: "insufficient_evidence", requiresHumanReview: true };
    for (const sourceId of claim.sources) {
      const quote = claim.quotes?.find((entry) => entry.sourceId === sourceId)?.quote;
      if (!quote || quote.trim().length < 8 || !sources.get(sourceId)!.text.normalize("NFC").includes(quote.normalize("NFC"))) fail("Each supported AI claim must quote the exact supplied evidence. Citation presence alone does not establish support.", "LEGAL_AI_CITATION", 422);
    }
    return { ...claim, supportStatus: "quotation_verified_pending_legal_review", requiresHumanReview: true };
  });
  return { ...proposal, claims, evidenceStatus: evidence.insufficientEvidence || !claims.length || claims.some((claim) => claim.supportStatus === "insufficient_evidence") ? "insufficient_evidence" : "pending_legal_review", limitations: ["The server verified source access, revision, date and exact quotations. An independent reviewer must determine whether each quotation supports its legal conclusion.", ...(!claims.length ? ["No supported claims were supplied. A source list alone cannot support the summary or draft."] : []), ...(evidence.insufficientEvidence ? [evidence.notice] : [])], citations: evidence.sources.filter((source) => proposal.sources.includes(source.sourceId) || proposal.claims.some((claim) => claim.sources.includes(source.sourceId))).map(({ text: _text, excerpt: _excerpt, ...source }) => { void _text; void _excerpt; return { ...source, asOf: evidence.asOf }; }) };
}
