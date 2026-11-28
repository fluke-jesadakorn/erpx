/** Shared jsonb_array_elements EXISTS head for the r42 identity_access read-tail
 * dedupe. Both call sites previously carried this exact byte sequence inline.
 * The array expression, row alias, predicate and bind list stay with the caller,
 * so the rendered statement remains byte-identical to its pre-dedupe literal and
 * bind order is untouched. This helper owns no lock, write, authority evaluation,
 * native route or public contract and changes no earlier-round helper. */
export const jsonbArrayHeadSql = (arrayExpr: string): string =>
  `SELECT 1 FROM jsonb_array_elements(${arrayExpr})`;
