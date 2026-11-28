/** Shared verified-lifecycle-account probe heads for the r39 identity_access
 * read-tail dedupe. The helper owns the common statement head once; a closed
 * union selects the original optional disable-operation tail so every rendered
 * statement stays byte-identical to its pre-dedupe literal and the bind list
 * stays untouched. No lock, write, authority evaluation, native kind or
 * earlier-round helper is changed. */

export type VerifiedLifecycleAccountShape = 'plain' | 'disable-probe';

export const verifiedLifecycleAccountSql = (shape: VerifiedLifecycleAccountShape): string => {
  const head = `SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE m.company_id=c.company_id AND m.person_id=c.person_id AND m.status='active'`;
  const tail = shape === 'plain'
    ? ''
    : " AND NOT EXISTS(SELECT 1 FROM people_directory_operations o WHERE o.company_id=c.company_id AND o.connection_id=i.connection_id AND o.status='completed' AND o.result->>'verified'='true' AND o.definition->'action'->>'kind'='account.disable' AND o.definition->'action'->>'caseId'=c.id AND o.definition->'action'->>'objectId'=i.external_id)";
  return `${head}${tail}`;
};
