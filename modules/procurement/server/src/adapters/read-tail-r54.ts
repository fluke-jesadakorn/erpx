/** Procurement read-tail r54 compatibility barrel. The columns/FROM/WHERE head
 * renderer was parameterized-identical to the r31 sourceImage wrapper, so the
 * duplicate literal was removed and callers keep the same export name
 * via this alias. Byte-identical rendered SQL, binds, ordering and locks are
 * unchanged. r17-r53 helpers, row locks, native registries and cross-owner
 * consumers stay untouched. */
export { sourceImage as columnsFromWhereHead } from './read-tail-r31';
