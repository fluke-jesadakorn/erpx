import 'server-only';

/** r37 additions: placeholder-style-parameterized leave-policy version lookup.
 * The two inline literals differ only in placeholder style ($1/$2 vs ?) and are
 * otherwise byte-identical; each call renders exactly the literal it replaces.
 * Callers keep their own binds. Text only: no r17-r36 helper, lock or spaced
 * JOIN constant is changed. */
export const leavePolicyVersionLookup = (
  leavePolicyVersions: string,
  companyPlaceholder: string,
  levelPlaceholder: string,
) =>
  `SELECT d.policy_version_id,p.effective_from FROM level_leave_defaults d JOIN (${leavePolicyVersions}) p ON p.id=d.policy_version_id WHERE d.company_id=${companyPlaceholder} AND d.level_id=${levelPlaceholder} ORDER BY d.version DESC LIMIT 1`;
