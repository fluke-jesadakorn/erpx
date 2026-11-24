import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {Row} from "@erpx/server-files/public/contracts/attachments-finance";
import {NativeOwnerWriteError,nativeOwnerWrite,NATIVE_OWNER_WRITE_NOOP_SQL} from "./native-owner-write";
import {companyProvisioningPointSql} from "./company-provisioning-point-sql";
export function FinanceAttachmentManageErpxFacts(db:SqlDatabase){return {
readOwner12:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>db.prepare(companyProvisioningPointSql('company_id,NULL::text branch_id,NULL::text department_id,version')).bind(facts.targetId,facts.companyId).first<Row>(),
versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>targetType==='company_provisioning'?assertion(db,commandId,`attachment-owner-${index}`,`EXISTS(${companyProvisioningPointSql('1',' AND version=?')})`,[targetId,companyId,version]):null,
// The native Manage ERPX owner-write axum command executes this version step
// in Rust and commits its receipt/audit/outbox in the same unit. The returned
// statement is an inert placeholder; no write SQL remains in TypeScript.
versionStep:async(targetType:string,targetId:string,expectedVersion:number,now:number)=>{
  if(targetType!=='company_provisioning')return null;
  try{return await nativeOwnerWrite('me_attachment_version_step',[now,targetId,expectedVersion],`me-attach:${targetId}:${expectedVersion}`,{companyId:targetId,now},()=>db.prepare(NATIVE_OWNER_WRITE_NOOP_SQL));}
  catch(error){if(error instanceof NativeOwnerWriteError)throw new Error(`NATIVE_MANAGE_ERPX_OWNER_WRITE_UNAVAILABLE:${error.status??503}`);throw error;}
}
};}
