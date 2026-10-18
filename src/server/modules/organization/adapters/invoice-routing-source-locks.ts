import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Actual level/template inputs of the original People Flow eligibility snapshot.
 * Compose after the command receipt writer fence, before the first source read;
 * normal Organization commands commit these effects after their receipt. */
export function invoiceRoutingOrganizationSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE staff_levels,spending_approval_templates IN SHARE MODE')];}
