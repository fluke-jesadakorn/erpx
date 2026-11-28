/** r23: byte-identical commerce-order-delivery existence guard shared by the
 * three CommerceFilesFacts read/guard call sites. Text only: every call site
 * still prepares its own statement with its own binds; no SQL text is accepted
 * from a caller and no result is cached here. r10-r22 helpers are untouched. */
import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "@erpx/commerce-server/public/hospitality-owner-projection-sources";
export const COMMERCE_DELIVERY_GUARD_SQL = `a.owner_type='commerce_order_delivery' AND a.owner_id=? AND a.purpose='proof_of_delivery' AND a.revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM attachment_links canonical WHERE canonical.attachment_id=a.id) AND NOT EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_fulfillment_evidence}) e WHERE e.attachment_id=a.id)`;
