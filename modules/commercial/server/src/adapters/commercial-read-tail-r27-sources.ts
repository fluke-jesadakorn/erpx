import "server-only";

/** Commercial read-tail r27 byte-identical package-source head. The two
 * owner-local renderers (r11 package-source projection, r12 purchase-scope
 * read) share the byte-identical SELECT-only head
 * `<projection> FROM <source><tail>`; only the projection, source and trailing
 * fragment vary and are passed verbatim. One owner-local literal owns the head
 * so it is no longer duplicated. Pure read:
 * no writes, no row/table/advisory locks and no cross-owner import. Callers keep
 * their own prepare/bind; every argument is code-owned, never request input. */
export function packageSourceSelect(projection: string, source: string, tail: string) {
  return `SELECT ${projection} FROM ${source}${tail}`;
}
