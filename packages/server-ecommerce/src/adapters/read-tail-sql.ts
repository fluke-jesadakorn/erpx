/** Ecommerce read-tail fragments shared across owner adapters. Text only: binds and locks are unchanged. */
/** r22: one parameterized one-row probe frame; every probe site rebuilds
 *  byte-identically. Text only: binds, locks and emitted SQL unchanged. */
export const ecommerceSelectOne = (tail: string) => `SELECT 1 ${tail}`;
/** r26: one parameterized scalar head; the label, child-aggregate,
 *  scope-row and reviewer-scope wrappers rebuild byte-identically. Text only:
 *  binds, locks and emitted SQL unchanged. */
export const ecommerceScalar = (body: string) => `SELECT ${body}`;
export const ecommerceModuleAssignment = (alias = '') => {
 const p = alias ? `${alias}.` : '';
 return ecommerceSelectOne(`FROM assignments${alias ? ` ${alias}` : ''} WHERE ${p}id=? AND ${p}company_id=? AND (jsonb_array_length(${p}modules_json::jsonb)=0 OR ${p}modules_json::jsonb @> '["ecommerce"]'::jsonb)`);
};
/** r23: one parameterized bare one-row probe frame. The workforce-scope
 *  current-assignment probe and the command-receipt replay probe rebuild
 *  byte-identically. Text only: binds, source locks and emitted SQL unchanged.
 *  r24: composes from the shared r22 select-one frame, byte-identically. */
export const ecommerceRowProbe = (from: string, where: string, gap = ' ') =>
  ecommerceSelectOne(`FROM${gap}${from} WHERE ${where}`);
/** r21: one scoped-row frame emits both column shapes byte-identically. The
 *  r20 separator stays configurable; text only, binds and locks unchanged. */
export const ecommerceScopeRow = (alias: string, branch = false, separator = ' ') =>
 `(${ecommerceScalar(`?::text company_id${branch ? ',?::text branch_id,?::text department_id' : ''}`)})${separator}${alias}`;
/** r21: one release/feature gate frame emits every probe site byte-identically.
 *  Text only: binds, source locks and the emitted SQL are unchanged. */
export const ecommerceReleaseGate = (scope: string, predicate: string, selectAlias = ' allowed') =>
 `SELECT ${predicate}${selectAlias} FROM ${scope}`;
/** r14: one parameterized EXISTS row guard; the channel, cart and checkout
 *  assertion callers rebuild byte-identically. Text only: binds unchanged. */
export const ecommerceExistsGuard = (table: string, where: string) =>
  `EXISTS(${ecommerceSelectOne(`FROM ${table} WHERE ${where}`)})`;
/** r14: one parameterized ordered list read; the listing, cart and checkout
 *  workspace callers rebuild byte-identically. Text only: binds unchanged.
 *  r24: the ORDER BY tail is optional so the three r11 source-row reads
 *  compose from the same frame, byte-identically. r26: WHERE is optional so
 *  the two CTE source frames compose byte-identically with zero binds. */
export const ecommerceOrderedList = (columns: string, from: string, where?: string, order?: string) =>
  `SELECT ${columns} FROM ${from}${where ? ` WHERE ${where}` : ''}${order ? ` ORDER BY ${order}` : ''}`;
/** r14/r15: one parameterized listing frame. The channel-row and evidence
 *  projections rebuild byte-identically through this single frame; the product
 *  join is optional and all three aliases are configurable. text only: the two
 *  binds and source locks are unchanged. r25: the image projection keeps its
 *  trailing newline verbatim. */
export const ecommerceListingFrame = (projection: string, { l = 'l', c = 'c', p = 'p', withProduct = true }: { l?: string; c?: string; p?: string; withProduct?: boolean } = {}) =>
  `SELECT ${projection} FROM ecommerce_listings ${l} JOIN ecommerce_channels ${c} ON ${c}.id=${l}.channel_id${withProduct ? ` JOIN commerce_products ${p} ON ${p}.id=${l}.product_id AND ${p}.company_id=${c}.company_id` : ''} WHERE ${l}.id=? AND ${c}.company_id=?`;
/** r14: one parameterized listing-channel row read; the repository detail and
 *  attachment-facts callers rebuild byte-identically. Binds unchanged.
 *  r25: composes from the shared listing frame, byte-identically. */
export const ecommerceListingChannelRow = (columns: string) =>
  ecommerceListingFrame(columns, { withProduct: false });
/** r14: one parameterized source-identity read; the channel and listing source
 *  callers rebuild byte-identically. Binds unchanged. */
export const ecommerceSourceIdentity = (columns: string, from: string) =>
  `SELECT ${columns} FROM ${from} WHERE x.id=?`;
/** r15: one parameterized listing-evidence read; the attachment content and
 *  purchase-witness callers rebuild byte-identically. Binds unchanged. The
 *  projection may carry a trailing newline to preserve the caller's line break.
 *  r25: composes from the shared listing frame, byte-identically. */
export const ecommerceListingEvidence = (projection: string, l = 'l', c = 'c', p = 'p') =>
  ecommerceListingFrame(projection, { l, c, p });
