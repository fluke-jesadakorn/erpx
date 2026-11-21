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
import { nativeOrgStatement } from './native-organization-owner-write';
export {companyAcceptanceSubmitSchema,companyAcceptanceDecisionSchema,companyAcceptanceRevokeSchema} from "@erpx/server-organization/domain/company-business-acceptance";
function bind(db:SqlDatabase, actor?: WorkContext){return createCompanyAcceptanceService<SqlStatement>({error:(status,code,message)=>new ApiError(status,code,message),now:Date.now,companyAuthority:actor=>companyAuthority(db,actor),commands:{replay:(actor,key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),commit:(actor,key,operation,input,id,version,result,effects,scope)=>executeCommand(db,actor,key,operation,input,id,version,result,effects,scope)},repository:{async company(companyId:string,companyId2:string){return db.prepare("SELECT id,display_name,status FROM organization_nodes WHERE id=? AND company_id=? AND kind='company'").bind(companyId, companyId2).first<{ id: string; display_name: string; status: string }>();},
async branches(companyId:string){return db.prepare("SELECT count(*) count FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active'").bind(companyId).first<{ count: number }>();},
levels:(...args:Parameters<AcceptanceRepository<SqlStatement>["levels"]>)=>operationsFacts(db).levels(...args),
async ceo(companyId:string){return db.prepare("SELECT count(*) count FROM organization_positions WHERE company_id=? AND active=1 AND is_ceo=1").bind(companyId).first<{ count: number }>();},
incompletePlacement:(...args:Parameters<AcceptanceRepository<SqlStatement>["incompletePlacement"]>)=>people_payrollFacts(db).incompletePlacement(...args),
async acceptance(companyId:string){return db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(companyId).first<AcceptanceRow>();},
async events(companyId:string){return db.prepare("SELECT id,action,actor_id,version,reason,created_at FROM company_business_acceptance_events WHERE company_id=? ORDER BY version,created_at,id").bind(companyId).all<EventRow>();},
async submitCurrent(companyId:string){return db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(companyId).first<AcceptanceRow>();},
submitVersion(key:string,value:"acceptance-version",companyId:string,version:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status IN ('draft','rejected','revoked'))", [companyId, version]);},
async submitAcceptance(companyId:string,recordVersion:number,userId:string,now:number,reason:string,now2:number){return nativeOrgStatement(db,companyId,'acceptance_submit',[companyId,recordVersion,userId,now,reason,now2],`company-acceptance-submit:${companyId}:${recordVersion}`,now);},
async submittedEvent(value:string,companyId:string,userId:string,recordVersion:number,reason:string,now:number){return nativeOrgStatement(db,companyId,'acceptance_submitted_event',[value,companyId,userId,recordVersion,reason,now],`company-acceptance-event:${value}`,now);},
reviewerNotifications:(...args:Parameters<AcceptanceRepository<SqlStatement>["reviewerNotifications"]>)=>collaborationFacts(db,actor!).reviewerNotifications(...args),
async reviewCurrent(companyId:string){return db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(companyId).first<AcceptanceRow>();},
reviewVersion(key:string,value:"acceptance-version",companyId:string,expectedVersion:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status='submitted')", [companyId, expectedVersion]);},
independentReview:(...args:Parameters<AcceptanceRepository<SqlStatement>["independentReview"]>)=>identity_accessFacts(db).independentReview(...args),
async reviewAcceptance(status:"rejected" | "accepted",version:number,userId:string,now:number,reason:string,now2:number,companyId:string){return nativeOrgStatement(db,companyId,'acceptance_review',[status,version,userId,now,reason,now2,companyId],`company-acceptance-review:${companyId}:${version}`,now);},
async reviewEvent(value:string,companyId:string,status:"rejected" | "accepted",userId:string,version:number,reason:string,now:number){return nativeOrgStatement(db,companyId,'acceptance_review_event',[value,companyId,status,userId,version,reason,now],`company-acceptance-event:${value}`,now);},
resultNotifications:(...args:Parameters<AcceptanceRepository<SqlStatement>["resultNotifications"]>)=>collaborationFacts(db,actor!).resultNotifications(...args),
async revokeCurrent(companyId:string){return db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(companyId).first<AcceptanceRow>();},
revokeVersion(key:string,value:"acceptance-version",companyId:string,expectedVersion:number){return assertion(db, key, value, "EXISTS(SELECT 1 FROM company_business_acceptance WHERE company_id=? AND version=? AND status='accepted')", [companyId, expectedVersion]);},
async revokeAcceptance(version:number,now:number,reason:string,now2:number,companyId:string){return nativeOrgStatement(db,companyId,'acceptance_revoke',[version,now,reason,now2,companyId],`company-acceptance-revoke:${companyId}:${version}`,now);},
async revokeEvent(value:string,companyId:string,userId:string,version:number,reason:string,now:number){return nativeOrgStatement(db,companyId,'acceptance_revoked_event',[value,companyId,userId,version,reason,now],`company-acceptance-event:${value}`,now);},
async workCurrent(companyId:string){return db.prepare("SELECT * FROM company_business_acceptance WHERE company_id=?").bind(companyId).first<AcceptanceRow>();}}});}
export function companyBusinessAcceptance(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["companyBusinessAcceptance"]>){return bind(db, args[0] as WorkContext).companyBusinessAcceptance(...args);}
export function submitCompanyBusinessAcceptance(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["submitCompanyBusinessAcceptance"]>){return bind(db, args[0] as WorkContext).submitCompanyBusinessAcceptance(...args);}
export function decideCompanyBusinessAcceptance(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["decideCompanyBusinessAcceptance"]>){return bind(db, args[0] as WorkContext).decideCompanyBusinessAcceptance(...args);}
export function revokeCompanyBusinessAcceptance(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["revokeCompanyBusinessAcceptance"]>){return bind(db, args[0] as WorkContext).revokeCompanyBusinessAcceptance(...args);}
export function companyBusinessAcceptanceWork(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["companyBusinessAcceptanceWork"]>){return bind(db, args[0] as WorkContext).companyBusinessAcceptanceWork(...args);}
