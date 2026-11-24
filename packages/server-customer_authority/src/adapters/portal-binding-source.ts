import 'server-only';
import { finalRootOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/final-root-owner-projection-sources";

/** One byte-identical binding source shared by the portal access scope reader
 * and the portal attachment facts reader. A caller-owned leading column list
 * keeps each reader's projection while the read core cannot drift. */
export const portalBindingSource = (columns = '') => `SELECT ${columns}b.company_id,o.branch_id,o.department_id,b.version
    FROM customer_access_bindings b
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id`;
