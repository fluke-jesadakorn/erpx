import 'server-only';
import { finalRootOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/final-root-owner-projection-sources";
import {portalBindingOrderCore} from './customer-authority-read-tail-r44-sources';

/** One byte-identical binding source shared by the portal access scope reader
 * and the portal attachment facts reader. A caller-owned leading column list
 * keeps each reader's projection while the read core cannot drift. The core
 * now comes from the r44 universal builder (G4 read-tail r44). */
export const portalBindingSource = (columns = '') => portalBindingOrderCore(
  `${columns}b.company_id,o.branch_id,o.department_id,b.version`,
  '\n    ', '\n    ',
  commerceProjectionSources.commerce_orders,
  '',
);
