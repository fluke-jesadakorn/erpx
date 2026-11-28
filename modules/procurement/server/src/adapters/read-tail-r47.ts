/** Procurement read-tail r47 compatibility barrel. The head/FROM wrapper was
 * parameterized-identical to the r33 quotedProjection wrapper, so the duplicate
 * literal was removed and callers keep the same export name via this
 * alias. Byte-identical settlement-fact SQL, binds, UNION ALL separators, scope
 * CTE and locks are unchanged. r17-r46 helpers stay untouched. */
export { quotedProjection as procurementSettlementFactSelect } from './read-tail-r33';
