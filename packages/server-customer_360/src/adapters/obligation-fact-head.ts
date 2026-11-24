import "server-only";

/** Byte-identical query head for the two scoped obligation fact UNION branches. Only the
 * fact label, row alias/id and projection aliases vary; each branch keeps its own pending
 * predicate, FROM/WHERE tail and bindings so emitted SQL is unchanged. r3/r5-r9 helpers,
 * source locks and cross-owner projection pins stay untouched. */
export const obligationFactHead = (kind: string, kindAlias: string, id: string, alias: string, factAlias: string) =>
  `SELECT '${kind}'${kindAlias},${id},to_jsonb(${alias})${factAlias},NOT `;
