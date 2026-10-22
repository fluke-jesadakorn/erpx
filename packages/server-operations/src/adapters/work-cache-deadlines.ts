import "server-only";
/** Trusted native fragment for the revision observer on its supplied transaction. */
export const workCacheOperationsDeadlines=()=>`SELECT starts_at FROM staff_task_delegations WHERE status='active' UNION ALL SELECT expires_at FROM staff_task_delegations WHERE status='active'`;
