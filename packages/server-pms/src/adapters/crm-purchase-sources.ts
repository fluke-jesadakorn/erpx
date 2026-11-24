import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import { crmHotelReservationSourceSql } from './pms-read-tail-r31-sql';
import { pmsRecordPropertyJoinSql } from './pms-read-tail-r45-sql';
export const crmHotelGuestSources=pmsRecordPropertyJoinSql('crmGuestSource') + " WHERE guest.kind='guest'";
export const crmHotelReservationSources=`${crmHotelReservationSourceSql(false)} WHERE reservation.kind='reservation'`;
export function crmHotelSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE pms_properties,pms_records IN SHARE MODE")];}
