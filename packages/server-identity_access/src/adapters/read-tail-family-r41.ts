/** Shared company/person alias-member projection for the r41 identity_access
 * read-tail dedupe. The single statement token owns the exact retired literal;
 * both callers interpolate it so rendered SQL and bind order stay byte-identical.
 * No lock, write, authority evaluation, native kind or earlier helper changes. */
export const iamMemberByCompanyPersonSql = (): string =>
  'SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?';
