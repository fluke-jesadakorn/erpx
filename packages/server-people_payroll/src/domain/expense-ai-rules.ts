export type ExpenseAiEvidenceFacts = {
  id: string; name: string; type: string; purpose: string; checksum: string;
  pageCount: number | null; pages: unknown[];
};
export type ExpenseAiProposalFacts = {
  stage: string; source_version: number; evidence_digest: string; provider_json: string;
  finance_policy_version: number | null;
};

export function expenseAiDocuments<Evidence extends ExpenseAiEvidenceFacts>(evidence: Evidence[], paymentId: string | null) {
  return paymentId ? evidence.filter(document => document.purpose === "expense_payment") : evidence;
}

export function incompleteExpenseAiDocuments<Evidence extends ExpenseAiEvidenceFacts>(documents: Evidence[]) {
  return documents.filter(document => (document.type === "application/pdf" && (!document.pageCount || document.pages.length !== document.pageCount)) || (document.type.startsWith("image/") && !document.pages.length));
}

export function currentExpenseAiProposal<Proposal extends ExpenseAiProposalFacts>(rows: Proposal[], source: { stage: string; version: number; digest: string }, providerSnapshot: string | null, financePolicyVersion: number | undefined) {
  return rows.find(proposal => (proposal.finance_policy_version == null || Number(proposal.finance_policy_version) === Number(financePolicyVersion)) && proposal.stage === source.stage && proposal.source_version === source.version && proposal.evidence_digest === source.digest && providerSnapshot !== null && proposal.provider_json === providerSnapshot);
}
