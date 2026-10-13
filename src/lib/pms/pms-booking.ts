export * from "@/lib/pms/pms-booking-domain";
import type {Snapshot} from "@/server/modules/pms/public/rules/pms-model";
import {bookingQuote as evaluate,bookingOffers as evaluateOffers} from "@/lib/pms/pms-booking-domain";
/** Compatibility boundary supplies time for older UI and adapter callers. */
export function bookingQuote(s:Snapshot,raw:unknown,blockId=""){return evaluate({...s,asOf:s.asOf??Date.now()},raw,blockId);}
export function bookingOffers(s:Snapshot,raw:unknown,publicOnly=false,blockId=""){return evaluateOffers({...s,asOf:s.asOf??Date.now()},raw,publicOnly,blockId);}
