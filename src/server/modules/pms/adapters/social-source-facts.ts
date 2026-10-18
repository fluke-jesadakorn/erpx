import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {pmsSourcesSql} from './pms/source-query';
/** Closed original PMS identity; this port changes no PMS read, release,
 * purchase, property, business-date, or participant policy. */
export const socialPmsSourceFacts=pmsSourcesSql;
export function socialPmsSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pms_records,pms_properties,pms_transfer_requests,pms_charge_corrections IN SHARE MODE');}

/** Private-file audit effects may write native source rows in this unit. */
export function socialPmsAttachmentSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pms_records,pms_properties,pms_transfer_requests,pms_charge_corrections IN SHARE ROW EXCLUSIVE MODE');}
