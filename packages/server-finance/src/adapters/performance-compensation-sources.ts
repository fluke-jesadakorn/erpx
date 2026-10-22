import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Original immutable capture only. Evaluation joins its own actual wage FK;
 * neither source labels nor this declaration grant Finance authority. */
export const performanceCompensationMoneySources=`SELECT money.*,to_jsonb(rate) rate_image FROM money_transaction_snapshots money LEFT JOIN company_fx_rates rate ON rate.id=money.fx_rate_id WHERE money.source_type='hr_compensation_revision'`;
export function performanceCompensationMoneySourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE money_transaction_snapshots,company_fx_rates IN SHARE MODE')];}
