import type {Snapshot} from "@/server/modules/pms/public/rules/pms-model";
import {availability as evaluate} from "@/lib/pms/pms-inventory-domain";
/** Compatibility boundary supplies time for callers without an evaluated snapshot. */
export function availability(s:Snapshot,typeId:string,date:string,excludeStay="",excludeBlock=""){return evaluate({...s,asOf:s.asOf??Date.now()},typeId,date,excludeStay,excludeBlock);}
