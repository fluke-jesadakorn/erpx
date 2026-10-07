import { samePerson } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";
import type { ExpenseCommandPort } from "./expense-commands";
import type { StaffActor } from "@/server/modules/operations/public/staff-policy";

export type AiClaim = Record<string, string | number | null> & {
  id: string; company_id: string; workflow_stage: string; workflow_version: number; version: number;
  claimant_user_id: string; created_by: string; prepared_by: string; branch_id: string | null; department_id: string | null; accounting_json: string;
};
export type EvidencePage = { page: number; key: string; checksum: string; bytes: number };
export type EvidenceDocument = { id: string; name: string; type: string; purpose: string; key: string; checksum: string; bytes: number; pageCount: number | null; pages: EvidencePage[] };
export type ExpenseAiActor = Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId" | "authority" | "iam">;
export interface ExpenseAiEligibilityPorts {
  currentActor(actor: ExpenseAiActor): Promise<boolean>;
  canDecide(actor: ExpenseAiActor, claim: AiClaim): Promise<boolean>;
  draftPayment(actor: ExpenseAiActor, claim: AiClaim, paymentId: string): Promise<boolean>;
  approvedPeople(actor: ExpenseAiActor, claim: AiClaim): Promise<string[]>;
  treasury(claim: AiClaim): Promise<string[]>;
}
export function createExpenseAiAllowed(ports: ExpenseAiEligibilityPorts) {
  return async (actor: ExpenseAiActor, claim: AiClaim, paymentId: string | null) => {
    if (!await ports.currentActor(actor)) return false;
    if (claim.cancelled_at || claim.company_id !== actor.companyId) return false;
    if (claim.workflow_stage === "draft") return !paymentId && [claim.claimant_user_id, claim.created_by].some(id => samePerson(actor, id));
    if (["department_approval", "accounts_review", "accounts_approval"].includes(claim.workflow_stage)) return !paymentId && await ports.canDecide(actor, claim);
    if (claim.workflow_stage !== "payment" || !paymentId) return false;
    const payment = await ports.draftPayment(actor, claim, paymentId), decisions = await ports.approvedPeople(actor, claim);
    const forbidden = [claim.claimant_user_id, claim.created_by, claim.prepared_by, (JSON.parse(claim.accounting_json) as { preparedBy?: string }).preparedBy, ...decisions];
    return payment && hasPermission(actor, "expense.pay") && !forbidden.some(id => samePerson(actor, id)) && (await ports.treasury(claim)).includes(actor.userId);
  };
}
export function createExpenseAiClaim(ports: { claim(companyId: string, id: string): Promise<AiClaim | null>; error: ExpenseCommandPort["error"] }) {
  return async (companyId: string, id: string) => {
    const claim = await ports.claim(companyId, id);
    if (!claim) throw ports.error({ status: 404, code: "EXPENSE_NOT_FOUND", message: "Expense claim not found." });
    return claim;
  };
}
export interface ExpenseAiSourcePorts {
  error: ExpenseCommandPort["error"];
  claim(companyId: string, id: string): Promise<AiClaim>;
  /** Capture an evidence read snapshot before resolving its payment version. */
  manifest(companyId: string, id: string, paymentId: string | null): { read(): Promise<string> };
  payment(companyId: string, id: string, paymentId: string): Promise<Record<string, string | number | null> | null>;
  digest(value: string): Promise<string>;
}
export function createExpenseAiSource(ports: ExpenseAiSourcePorts) {
  return async (companyId: string, id: string, paymentId: string | null = null) => {
    const claim = await ports.claim(companyId, id), manifest = ports.manifest(companyId, id, paymentId);
    const payment = paymentId ? await ports.payment(companyId, id, paymentId) : null;
    if (paymentId && !payment) throw ports.error({ status: 409, code: "PAYMENT_CHANGED", message: "The payment preparation changed." });
    const raw = await manifest.read(), evidence = JSON.parse(raw) as EvidenceDocument[];
    const digest = await ports.digest(JSON.stringify({ raw, version: claim.version, paymentVersion: payment?.version ?? null }));
    return { claim, payment, evidence, digest, sourceVersion: Number(payment?.version ?? claim.version), raw };
  };
}
