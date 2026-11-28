import "server-only";
/** Canonical active-delegation deadline projection shared by the two cache-fence
 *  UNION branches. Only the deadline column varies; each rendering is
 *  byte-identical to the retired inline literal. */
const activeDelegationDeadline = (column: string) => `SELECT ${column} FROM staff_task_delegations WHERE status='active'`;
/** Trusted native fragment for the revision observer on its supplied transaction. */
export const workCacheOperationsDeadlines=()=>`${activeDelegationDeadline('starts_at')} UNION ALL ${activeDelegationDeadline('expires_at')}`;
