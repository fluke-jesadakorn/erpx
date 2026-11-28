/** Byte-identical active-IAM-role probe shared by the technology provisioning
 * role-revision guard and the people-template active-role check. Both owners
 * used the same active-role projection/head; only the
 * fixed published-revision suffix differed. The caller selects that fixed
 * suffix by flag, never by supplying SQL, so every rendered statement stays
 * byte-identical to its pre-dedupe literal. The projection, table, binds and
 * status predicate are fixed here and cannot drift. No lock, write, session
 * authority, native kind or earlier-round helper is changed. */
export const activeIamRoleProbeSql = (withRevision: boolean) =>
  `SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'${withRevision ? ' AND published_revision=$3' : ''}`;
