import type {Snapshot} from "./pms-model";
import {availability as evaluate} from "./pms-inventory-domain";
/** Compatibility boundary supplies time for callers without an evaluated snapshot. */
export function availability(s:Snapshot,typeId:string,date:string,excludeStay="",excludeBlock=""){return evaluate({...s,asOf:s.asOf??Date.now()},typeId,date,excludeStay,excludeBlock);}
