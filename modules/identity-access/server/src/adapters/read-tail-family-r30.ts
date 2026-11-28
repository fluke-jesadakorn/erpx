/** Shared query heads for the r30 identity_access read-tail families. Each helper
 * pins one statement token and its fixed table/projection predicates; callers
 * inject only their varying fragment (including original whitespace and
 * newlines) so every rendered statement stays byte-identical to its pre-dedupe
 * literal. No lock, write, session authority, native kind or earlier-round
 * helper is changed. */
export const assignmentCompanyProbeSql = (tail: string): string =>
  `SELECT 1 FROM assignments WHERE company_id=? AND ${tail}`;

export const payrollOwnerVersionProbeSql = (from: string, tail: string): string =>
  `SELECT 1 ${from} WHERE id=? AND company_id=? AND ${tail}`;

export const umGroupProbeSql = (tail: string): string =>
  `SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2 AND NOT (definition->>'archived')::boolean${tail}`;

export const protectedPersonOwnerSql =
  "SELECT 1 FROM assignments a LEFT JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id WHERE a.company_id=? AND people_person_user(?,?,a.user_id) AND a.revoked_at IS NULL AND (a.role='holding_owner' OR m.login_policy='emergency')";
