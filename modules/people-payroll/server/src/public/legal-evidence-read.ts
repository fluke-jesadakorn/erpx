import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {getLeave} from '@erpx/people-payroll-server/public/leave-read';
import {directoryPerson} from "@erpx/people-payroll-server/public/people-directory";
/** Preserve the original actor visibility rules in the caller's preservation transaction. */
export function legalLeaveSource(db:SqlDatabase,holdingId:string,id:string,actor:WorkContext){return getLeave(holdingId,id,actor,db);}
export function legalPersonSource(db:SqlDatabase,actor:WorkContext,id:string,engagementId?:string){return directoryPerson(actor,id,engagementId,db);}
