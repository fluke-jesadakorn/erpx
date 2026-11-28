import 'server-only';
/** r30 additions: source/predicate-parameterized active-level engagement guard.
 * This family is new; every r17-r29 helper and its call sites are left untouched.
 * Every call renders byte-identical SQL to the literal it replaces; only the
 * owner-supplied engagement source and WHERE predicate vary (never caller
 * input). Text only: callers keep their own binds and supplied statements. */
export const activeLevelAbsent = (engagementSource: string, predicate: string) =>
  `SELECT 1 FROM (${engagementSource}) e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE ${predicate}`;
