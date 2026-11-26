import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";

import {createCompanyAcceptanceService,type AcceptanceRepository} from "@erpx/server-organization/application/company-business-acceptance";
import type {SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {AcceptanceRow,EventRow} from "@erpx/server-organization/domain/company-business-acceptance";
import {companyAcceptanceFacts as operationsFacts} from "@erpx/server-operations/public/company-acceptance-facts";
import {companyAcceptanceFacts as people_payrollFacts} from "@erpx/server-people_payroll/public/company-acceptance-facts";
import {companyAcceptanceFacts as collaborationFacts} from "@erpx/server-collaboration/adapters/native-company-acceptance-facts";
import {companyAcceptanceFacts as identity_accessFacts} from "@erpx/server-identity_access/public/company-acceptance-facts";
import { nativeOrgAll, nativeOrgSql, nativeOrgStatement } from './native-organization-owner-write';
export {companyAcceptanceSubmitSchema,companyAcceptanceDecisionSchema,companyAcceptanceRevokeSchema} from "@erpx/server-organization/domain/company-business-acceptance";
function bind(db:SqlDatabase, actor?: WorkContext){return createCompanyAcceptanceService<SqlStatement>({error:(status,code,message)=>new ApiError(status,code,message),now:Date.now,companyAuthority:actor=>companyAuthority(db,actor),commands:{replay:(actor,key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),commit:(actor,key,operation,input,id,version,result,effects,scope)=>executeCommand(db,actor,key,operation,input,id,version,result,effects,scope)},repository:{async company(companyId:string,companyId2:string){return nativeOrgAll<{ id: string; display_name: string; status: string }>(db,companyId,'acceptance_read_company',[companyId,companyId2]).then(rows=>rows[0]??null);},
async branches(companyId:string){return nativeOrgAll<{ count: number }>(db,companyId,'acceptance_read_branches',[companyId]).then(rows=>rows[0]??null);},
levels:(...args:Parameters<AcceptanceRepository<SqlStatement>["levels"]>)=>operationsFacts(db).levels(...args),
async ceo(companyId:string){return nativeOrgAll<{ count: number }>(db,companyId,'acceptance_read_ceo',[companyId]).then(rows=>rows[0]??null);},
incompletePlacement:(...args:Parameters<AcceptanceRepository<SqlStatement>["incompletePlacement"]>)=>people_payrollFacts(db).incompletePlacement(...args),
async acceptance(companyId:string){return nativeOrgAll<AcceptanceRow>(db,companyId,'acceptance_read_current',[companyId]).then(rows=>rows[0]??null);},
async events(companyId:string){return nativeOrgAll<EventRow>(db,companyId,'acceptance_read_events',[companyId]).then(results=>({results}));},
async submitCurrent(companyId:string){return nativeOrgAll<AcceptanceRow>(db,companyId,'acceptance_read_current',[companyId]).then(rows=>rows[0]??null);},
async submitVersion(key:string,value:"acceptance-version",companyId:string,version:number){const {query,args}=await nativeOrgSql(companyId,'acceptance_submit_version_check',[companyId,version]);return assertion(db, key, value, query, args);},
async submitAcceptance(companyId:string,recordVersion:number,userId:string,now:number,reason:string,now2:number){return nativeOrgStatement(db,companyId,'acceptance_submit',[companyId,recordVersion,userId,now,reason,now2],`company-acceptance-submit:${companyId}:${recordVersion}`,now);},
async submittedEvent(value:string,companyId:string,userId:string,recordVersion:number,reason:string,now:number){return nativeOrgStatement(db,companyId,'acceptance_submitted_event',[value,companyId,userId,recordVersion,reason,now],`company-acceptance-event:${value}`,now);},
reviewerNotifications:(...args:Parameters<AcceptanceRepository<SqlStatement>["reviewerNotifications"]>)=>collaborationFacts(db,actor!).reviewerNotifications(...args),
async reviewCurrent(companyId:string){return nativeOrgAll<AcceptanceRow>(db,companyId,'acceptance_read_current',[companyId]).then(rows=>rows[0]??null);},
async reviewVersion(key:string,value:"acceptance-version",companyId:string,expectedVersion:number){const {query,args}=await nativeOrgSql(companyId,'acceptance_review_version_check',[companyId,expectedVersion]);return assertion(db, key, value, query, args);},
independentReview:(...args:Parameters<AcceptanceRepository<SqlStatement>["independentReview"]>)=>identity_accessFacts(db).independentReview(...args),
async reviewAcceptance(status:"rejected" | "accepted",version:number,userId:string,now:number,reason:string,now2:number,companyId:string){return nativeOrgStatement(db,companyId,'acceptance_review',[status,version,userId,now,reason,now2,companyId],`company-acceptance-review:${companyId}:${version}`,now);},
async reviewEvent(value:string,companyId:string,status:"rejected" | "accepted",userId:string,version:number,reason:string,now:number){return nativeOrgStatement(db,companyId,'acceptance_review_event',[value,companyId,status,userId,version,reason,now],`company-acceptance-event:${value}`,now);},
resultNotifications:(...args:Parameters<AcceptanceRepository<SqlStatement>["resultNotifications"]>)=>collaborationFacts(db,actor!).resultNotifications(...args),
async revokeCurrent(companyId:string){return nativeOrgAll<AcceptanceRow>(db,companyId,'acceptance_read_current',[companyId]).then(rows=>rows[0]??null);},
async revokeVersion(key:string,value:"acceptance-version",companyId:string,expectedVersion:number){const {query,args}=await nativeOrgSql(companyId,'acceptance_revoke_version_check',[companyId,expectedVersion]);return assertion(db, key, value, query, args);},
async revokeAcceptance(version:number,now:number,reason:string,now2:number,companyId:string){return nativeOrgStatement(db,companyId,'acceptance_revoke',[version,now,reason,now2,companyId],`company-acceptance-revoke:${companyId}:${version}`,now);},
async revokeEvent(value:string,companyId:string,userId:string,version:number,reason:string,now:number){return nativeOrgStatement(db,companyId,'acceptance_revoked_event',[value,companyId,userId,version,reason,now],`company-acceptance-event:${value}`,now);},
async workCurrent(companyId:string){return nativeOrgAll<AcceptanceRow>(db,companyId,'acceptance_read_current',[companyId]).then(rows=>rows[0]??null);}}});}
export function companyBusinessAcceptance(db:SqlDatabase,actor:WorkContext){return bind(db,actor).companyBusinessAcceptance(actor);}
export function submitCompanyBusinessAcceptance(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["submitCompanyBusinessAcceptance"]>){return bind(db, args[0] as WorkContext).submitCompanyBusinessAcceptance(...args);}
export function decideCompanyBusinessAcceptance(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["decideCompanyBusinessAcceptance"]>){return bind(db, args[0] as WorkContext).decideCompanyBusinessAcceptance(...args);}
export function revokeCompanyBusinessAcceptance(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["revokeCompanyBusinessAcceptance"]>){return bind(db, args[0] as WorkContext).revokeCompanyBusinessAcceptance(...args);}
export function companyBusinessAcceptanceWork(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["companyBusinessAcceptanceWork"]>){return bind(db, args[0] as WorkContext).companyBusinessAcceptanceWork(...args);}
