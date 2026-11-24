import "server-only";
/** Commercial read-tail r12 parameterized purchase-scope read family.
 * The builder owns one SQL literal and renders both original purchase-scope
 * reads byte-identically: the enforced-mode capability guard projection and
 * the shadow-mode observation projection share the same shape over the
 * materialized `purchase_scope` CTE. Every caller keeps its own CTE prefix,
 * bind/prepare and exports. The projection argument is a code-owned SQL
 * fragment only, never request input. The r1-r11 helpers, advisory/row locks
 * and native owner statement kinds stay untouched. Pure read-only: no writes,
 * no row locks and no advisory lock. */
export function purchaseScopeRead(projection: string) {
  return `SELECT ${projection} FROM purchase_scope`;
}
