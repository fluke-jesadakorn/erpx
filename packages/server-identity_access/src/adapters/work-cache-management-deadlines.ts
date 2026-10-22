export const workCacheManagementDeadlines=()=>`SELECT (definition->>'validUntil')::bigint FROM um_candidates`;
