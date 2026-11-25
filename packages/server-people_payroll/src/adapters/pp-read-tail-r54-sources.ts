import 'server-only';
/** Canonical parameterized read-only owner builder shared across People Payroll
 * adapters (G4 read-tail r54). It owns the single hr_payroll_runs evidence/version
 * EXISTS literal; the four former inline copies now delegate here and keep no local
 * copy. The two tails are copied verbatim from those call sites, preserving exact
 * status-list ordering, NULL tails and bind order. Pure read-only: no writes,
 * table locks, row locks or advisory statements. */
export const ppRd54RunEvidenceGuard = (statusTail: string, nullTail: string) =>
  `EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND ${statusTail}${nullTail})`;
