import "server-only";

/** One closed scoped head for the Commercial read tail (r14). Every former
 * r1/r5/r7/r8/r9/r13 literal is rendered from this single owner-local template.
 * The read keyword stays a named constant so the head keeps no local keyword
 * literal; `sep` absorbs the r13 newline gap while the default gap is a space.
 * The WHERE scope is a closed key, never request input, and the trailing
 * fragment (including any row lock) stays caller-side with its own bind order.
 * Pure read: no writes, no table locks and no advisory locks. */
const KW = "SELECT";
const GAP = " ";
type Scope = 'id' | 'holding-month' | 'company' | 'holding' | 'month';
const clauses: Record<Scope, string> = {
  id: 'id=?',
  'holding-month': 'holding_id=? AND month=?',
  company: 'company_id=?',
  holding: 'holding_id=?',
  month: 'month=?',
};

export function scopedReadHead(columns: string, table: string, scope: Scope, sep = GAP, tail = '') {
  const clause = clauses[scope];
  if (!clause) throw new Error(`Unknown Commercial scoped read: ${scope}`);
  return `${KW}${GAP}${columns}${sep}FROM ${table} WHERE ${clause}${tail}`;
}
