/** Ecommerce read-tail fragments shared across owner adapters. Text only: binds and locks are unchanged. */
export const ecommerceModuleAssignment = (alias = '') => {
 const p = alias ? `${alias}.` : '';
 return `SELECT 1 FROM assignments${alias ? ` ${alias}` : ''} WHERE ${p}id=? AND ${p}company_id=? AND (jsonb_array_length(${p}modules_json::jsonb)=0 OR ${p}modules_json::jsonb @> '["ecommerce"]'::jsonb)`;
};
/** r21: one scoped-row frame emits both column shapes byte-identically. The
 *  r20 separator stays configurable; text only, binds and locks unchanged. */
export const ecommerceScopeRow = (alias: string, branch = false, separator = ' ') =>
 `(SELECT ?::text company_id${branch ? ',?::text branch_id,?::text department_id' : ''})${separator}${alias}`;
/** r21: one release/feature gate frame emits every probe site byte-identically.
 *  Text only: binds, source locks and the emitted SQL are unchanged. */
export const ecommerceReleaseGate = (scope: string, predicate: string, selectAlias = ' allowed') =>
 `SELECT ${predicate}${selectAlias} FROM ${scope}`;
/** r14: one parameterized EXISTS row guard; the channel, cart and checkout
 *  assertion callers rebuild byte-identically. Text only: binds unchanged. */
export const ecommerceExistsGuard = (table: string, where: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE ${where})`;
/** r14: one parameterized ordered list read; the listing, cart and checkout
 *  workspace callers rebuild byte-identically. Text only: binds unchanged. */
export const ecommerceOrderedList = (columns: string, from: string, where: string, order: string) =>
  `SELECT ${columns} FROM ${from} WHERE ${where} ORDER BY ${order}`;
/** r14: one parameterized listing-channel row read; the repository detail and
 *  attachment-facts callers rebuild byte-identically. Binds unchanged. */
export const ecommerceListingChannelRow = (columns: string) =>
  `SELECT ${columns} FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?`;
/** r14: one parameterized source-identity read; the channel and listing source
 *  callers rebuild byte-identically. Binds unchanged. */
export const ecommerceSourceIdentity = (columns: string, from: string) =>
  `SELECT ${columns} FROM ${from} WHERE x.id=?`;
/** r15: one parameterized listing-evidence read; the attachment content and
 *  purchase-witness callers rebuild byte-identically. Binds unchanged. The
 *  projection may carry a trailing newline to preserve the caller's line break. */
export const ecommerceListingEvidence = (projection: string, l = 'l', c = 'c', p = 'p') =>
  `SELECT ${projection} FROM ecommerce_listings ${l} JOIN ecommerce_channels ${c} ON ${c}.id=${l}.channel_id JOIN commerce_products ${p} ON ${p}.id=${l}.product_id AND ${p}.company_id=${c}.company_id WHERE ${l}.id=? AND ${c}.company_id=?`;
