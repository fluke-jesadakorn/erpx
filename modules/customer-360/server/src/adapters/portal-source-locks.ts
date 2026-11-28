import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
/** Original public action receipts are local writer sources, never a staff
 * role or purchase decision for an external account. */
export function customerPortalActionSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE customer_portal_actions IN SHARE ROW EXCLUSIVE MODE')];}
