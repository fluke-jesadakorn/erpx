/** Shared exec metric label head for the r40 identity_access read-tail dedupe.
 * Both metric projections previously carried this exact byte sequence inline.
 * The label expression and the projection body stay with their caller, so the
 * rendered statement, alias, whitespace and bind order remain unchanged. This
 * helper owns no authority evaluation, lock, native route or public contract. */
export const execMetricLabelHead = (label: string): string => `SELECT ${label} label,count(*)::text value FROM `;
