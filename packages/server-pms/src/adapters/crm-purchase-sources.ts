import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import { crmHotelReservationSourceSql } from './pms-read-tail-r31-sql';
export const crmHotelGuestSources="SELECT guest.id,property.company_id,property.branch_id,property.id property_id FROM pms_records guest JOIN pms_properties property ON property.id=guest.property_id WHERE guest.kind='guest'";
export const crmHotelReservationSources=`${crmHotelReservationSourceSql(false)} WHERE reservation.kind='reservation'`;
export function crmHotelSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE pms_properties,pms_records IN SHARE MODE")];}
