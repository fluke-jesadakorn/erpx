/** One closed scope-tuple family for the sales read tail. `mode` is an
 * owner-local fixed token, never caller input, so each call renders exactly
 * the same bytes as its pre-r29 inline literal. */
export type ScopeTupleMode = 'actor' | 'company' | 'scope';
export const scopeTuple = (mode: ScopeTupleMode) => {
  const tail = mode === 'actor'
    ? ',?::text branch_id,?::text department_id'
    : mode === 'company'
      ? ',NULL::text branch_id,NULL::text department_id'
      : '';
  return `(SELECT ?::text company_id${tail})`;
};
