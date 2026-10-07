import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkContext} from "../../identity_access/public/authorization";
import {getLeave} from "./leave";
import {directoryPerson} from "./people-directory";
/** Preserve the original actor visibility rules in the caller's preservation transaction. */
export function legalLeaveSource(db:SqlDatabase,holdingId:string,id:string,actor:WorkContext){return getLeave(holdingId,id,actor,db);}
export function legalPersonSource(db:SqlDatabase,actor:WorkContext,id:string,engagementId?:string){return directoryPerson(actor,id,engagementId,db);}
