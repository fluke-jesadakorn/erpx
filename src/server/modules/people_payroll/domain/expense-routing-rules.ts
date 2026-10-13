import type { SpendingException, SpendingKind } from "@/lib/shared/spending";

/** Specific position exceptions precede request-kind and company-wide exceptions. */
export function selectSpendingException(exceptions: SpendingException[], positionId: string | null, kind: SpendingKind) {
  return exceptions.filter(exception => (!exception.positionId || exception.positionId === positionId) && (!exception.requestKind || exception.requestKind === kind))
    .sort((a, b) => (Number(!!b.positionId) * 2 + Number(!!b.requestKind)) - (Number(!!a.positionId) * 2 + Number(!!a.requestKind)))[0];
}

export function hasIndependentSpendingTeam(preparers: string[], reviewers: string[], payers: string[], excluded: string[] = []) {
  const forbidden = new Set(excluded);
  return preparers.some(preparer => !forbidden.has(preparer) && reviewers.some(reviewer => reviewer !== preparer && !forbidden.has(reviewer) && payers.some(payer => payer !== preparer && payer !== reviewer && !forbidden.has(payer))));
}

/** Saved candidates remain eligible only when later independent stages are possible. */
export function independentAccountingCandidates(stage: string | null | undefined, candidates: string[], reviewers: string[], payers: string[]) {
  if (stage === "accounts_approval") return candidates.filter(candidate => payers.some(payer => payer !== candidate));
  if (stage === "accounts_review") return candidates.filter(candidate => reviewers.some(reviewer => reviewer !== candidate && payers.some(payer => payer !== candidate && payer !== reviewer)));
  return candidates;
}
