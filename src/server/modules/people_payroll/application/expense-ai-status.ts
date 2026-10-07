import { expenseAiResultSchema } from "@/lib/expense-ai";
import { currentExpenseAiProposal, expenseAiDocuments, incompleteExpenseAiDocuments, type ExpenseAiEvidenceFacts, type ExpenseAiProposalFacts } from "../domain/expense-ai-rules";

export type ExpenseAiStatusProposal = ExpenseAiProposalFacts & { id: string; state: string; result_json: string | null; error: string | null };
export type ExpenseAiStatusSource<Claim extends { workflow_stage: string }> = { claim: Claim; evidence: ExpenseAiEvidenceFacts[]; sourceVersion: number; digest: string };

export interface ExpenseAiStatusPorts<Actor, Claim extends { workflow_stage: string }> {
  source(actor: Actor, id: string, paymentId: string | null): Promise<ExpenseAiStatusSource<Claim>>;
  allowed(actor: Actor, claim: Claim, paymentId: string | null): Promise<boolean>;
  proposals(actor: Actor, id: string, paymentId: string | null): Promise<ExpenseAiStatusProposal[]>;
  providerConfiguration(actor: Actor): Promise<{ snapshot: string | null; unavailable: string | null }>;
  financePolicyVersion(actor: Actor): Promise<number | undefined>;
}

export function createExpenseAiStatus<Actor, Claim extends { workflow_stage: string }>(ports: ExpenseAiStatusPorts<Actor, Claim>) {
  return async (actor: Actor, id: string, paymentId: string | null) => {
    const source = await ports.source(actor, id, paymentId), allowed = await ports.allowed(actor, source.claim, paymentId);
    const rows = await ports.proposals(actor, id, paymentId), configuration = await ports.providerConfiguration(actor);
    const policyVersion = await ports.financePolicyVersion(actor);
    const candidate = currentExpenseAiProposal(rows, { stage: source.claim.workflow_stage, version: source.sourceVersion, digest: source.digest }, configuration.snapshot, policyVersion);
    const documents = expenseAiDocuments(source.evidence, paymentId);
    return {
      allowed, unavailable: configuration.unavailable, hasEvidence: documents.length > 0,
      incomplete: incompleteExpenseAiDocuments(documents).map(document => ({ id: document.id, name: document.name, type: document.type, checksum: document.checksum })),
      proposal: candidate ? { id: candidate.id, state: candidate.state, result: candidate.result_json ? expenseAiResultSchema.parse(JSON.parse(candidate.result_json)) : null, error: candidate.error } : null,
      stale: !candidate && rows.length > 0, sourceVersion: source.sourceVersion, evidenceDigest: source.digest,
    };
  };
}
