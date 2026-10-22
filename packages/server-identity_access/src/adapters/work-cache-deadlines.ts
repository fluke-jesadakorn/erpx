import "server-only";
/** Trusted native fragment for the revision observer on its supplied transaction. */
export const workCacheIdentityAccessDeadlines=()=>`SELECT valid_from deadline FROM assignments UNION ALL SELECT valid_until FROM assignments UNION ALL SELECT expires_at FROM iam_emergency_sessions UNION ALL SELECT observed_at+300000 FROM iam_identities`;
