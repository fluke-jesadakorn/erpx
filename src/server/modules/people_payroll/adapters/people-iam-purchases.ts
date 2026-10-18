import "server-only";
import {randomUUID} from "node:crypto";
import type {PoolClient} from "pg";
import type {IamActor} from "../../identity_access/public/iam-context";
import {assertIamOwnerTransaction,deferIamOwnerCheck} from "../../identity_access/public/iam-transactions";
import {transactionDatabase} from "../../runtime/public/runtime/postgres";
import {ApiError} from "../../runtime/public/http";
import {flushFinanceDeferredEffects} from "../../finance/public/finance-transactions";
import {peoplePackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from "../../commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "../../commercial/public/rules/packages";

/** IAM recipes and position bindings remain shared identity foundation. Only an
 * actual employment lifecycle case consumes People lifecycle authority. */
async function prepare(client:PoolClient,actor:IamActor,key:string,capability:string,purpose:CapabilityPurpose,source:()=>Promise<unknown>,identity?:()=>Promise<unknown>,finalAuthority?:()=>Promise<void>){
 const mode=await peoplePackageEnforcementMode();if(mode==="disabled")return;
 assertIamOwnerTransaction(client);
 if(actor.platform||actor.scopeKind!=="holding")throw new ApiError(403,"COMPANY_SCOPE_REQUIRED","Choose current Company authority for this work.");
 const database=transactionDatabase(client),checkKey=`people-iam:${randomUUID()}`;
 await database.batch(companyPurchaseReadLocks(database,actor.companyId));
 const replay=Boolean((await client.query("SELECT 1 FROM iam_receipts WHERE id=$1 AND company_id=$2 AND actor_id=$3",[key,actor.companyId,actor.userId])).rowCount),scope={companyId:actor.companyId,capability,purpose:replay?"history" as const:purpose},boundary=purchaseCommandBoundary(database,mode,checkKey,scope)!;
 // The selected assignment, identity, role publication and work context remain
 // exactly those that authorized the command; no alternate account role helps.
 const authority=async()=>{
  const result=await client.query(`SELECT to_jsonb(a) assignment,a.version selected_version,r.published_revision,m.version identity_version,m.status identity_status,m.login_policy,m.identity_id,
   w.assignment_id,w.assignment_version,w.revision,w.expires_at,
   iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
   AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions s WHERE s.session_id=$4 AND s.company_id=$2 AND s.user_id=$3) active
   FROM assignments a JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active'
   LEFT JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id
   JOIN active_work_contexts w ON w.session_id=$4 AND w.company_id=a.company_id
   WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 AND a.role=$5`,[actor.assignmentId,actor.companyId,actor.userId,actor.sessionId,actor.role]);
  const row=result.rows[0];
  if(!row?.active||row.assignment_id!==actor.assignmentId||Number(row.assignment_version)!==Number(row.selected_version)||Number(row.expires_at)<=Date.now()||actor.authority&&Number(row.revision)!==actor.authority.revision||actor.iam&&(Number(row.published_revision)!==actor.iam.roleRevision||Number(row.identity_version??0)!==actor.iam.identityVersion))throw new ApiError(403,"PEOPLE_IAM_AUTHORITY_CHANGED","Choose your current authorized work role again.");
  return JSON.stringify(row);
 };
 const originalAuthority=await authority(),originalIdentity=identity?JSON.stringify(await identity()):undefined;
 await checkPurchasedCapability(database,mode,scope);await database.batch(boundary.before);
 let expected=JSON.stringify(await source());
 deferIamOwnerCheck(client,async()=>{
  try{await flushFinanceDeferredEffects(database);
   if(!replay)await finalAuthority?.();
   if(await authority()!==originalAuthority)throw new ApiError(403,"PEOPLE_IAM_AUTHORITY_CHANGED","Your selected authority changed before completion.");
   if(identity&&JSON.stringify(await identity())!==originalIdentity||JSON.stringify(await source())!==expected)throw new ApiError(409,"PEOPLE_IAM_SOURCE_CHANGED","The original People or identity source changed before completion.");
   const observed=await database.batch([...boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(checkKey)]);boundary.observe?.(observed);
  }catch(error){throw purchaseCommandFailure(error)??error;}
 });
 return async()=>{if(identity&&JSON.stringify(await identity())!==originalIdentity)throw new ApiError(409,"PEOPLE_IAM_SOURCE_CHANGED","The original People source changed.");expected=JSON.stringify(await source());};
}
function lifecycleSource(client:PoolClient,companyId:string,caseId:string){
 const identity=async()=>(await client.query("SELECT to_jsonb(c)-'status'-'version'-'updated_at'-'last_error'-'attempts'-'next_attempt_at' source FROM people_lifecycle_cases c WHERE c.company_id=$1 AND c.id=$2",[companyId,caseId])).rows[0]?.source??null;
 const source=async()=>({case:await identity(),grants:(await client.query("SELECT to_jsonb(g) source FROM people_lifecycle_grants g WHERE g.company_id=$1 AND g.case_id=$2 ORDER BY g.id",[companyId,caseId])).rows.map(r=>r.source)});
 return {identity,source};
}
export async function preparePeopleIamCommand(client:PoolClient,actor:IamActor,key:string,operation:string,input:unknown){
 if(await peoplePackageEnforcementMode()==="disabled")return;
 const database=transactionDatabase(client);await database.batch(companyPurchaseReadLocks(database,actor.companyId));
 const value=input as Record<string,unknown>;
 if(operation.startsWith("people.package.")){
  const packageInput=value.input as Record<string,unknown>,id=String(packageInput.id??`package-${key}`);
  return prepare(client,actor,key,"foundation.identity","new-work",async()=>({package:(await client.query("SELECT * FROM people_access_packages WHERE company_id=$1 AND id=$2",[actor.companyId,id])).rows[0]??null,revisions:(await client.query("SELECT * FROM people_package_revisions WHERE company_id=$1 AND package_id=$2 ORDER BY revision",[actor.companyId,id])).rows}));
 }
 if(operation.startsWith("people.position.")){
  const id=String(value.positionId);
  return prepare(client,actor,key,"foundation.identity",operation.endsWith("preview")?"history":"new-work",async()=>({position:(await client.query("SELECT * FROM organization_positions WHERE company_id=$1 AND id=$2",[actor.companyId,id])).rows[0]??null,binding:(await client.query("SELECT * FROM people_position_packages WHERE company_id=$1 AND position_id=$2",[actor.companyId,id])).rows[0]??null,package:(await client.query("SELECT * FROM people_package_revisions WHERE company_id=$1 AND package_id=$2 AND revision=$3",[actor.companyId,value.packageId,value.revision])).rows[0]??null}));
 }
 if(operation==="people.lifecycle.request-access"){
  const id=String(value.id),row=(await client.query("SELECT kind FROM people_lifecycle_cases WHERE company_id=$1 AND id=$2",[actor.companyId,id])).rows[0];
  if(!row)throw new ApiError(404,"PEOPLE_SOURCE_NOT_FOUND","The lifecycle source was not found in this Company.");
  const source=lifecycleSource(client,actor.companyId,id);
  return prepare(client,actor,key,"people.lifecycle",["offboarding","suspend"].includes(row.kind)?"settle-existing":"new-work",source.source,source.identity);
 }
}
/** Called by the original IAM review owner before reading its review/receipt.
 * Classification uses persisted review and case rows, never caller labels. */
export async function preparePeopleLifecycleIamReview(client:PoolClient,actor:IamActor,reviewId:string,decision:"approve"|"reject",key:string){
 if(await peoplePackageEnforcementMode()==="disabled"||actor.platform)return;
 const database=transactionDatabase(client);await database.batch(companyPurchaseReadLocks(database,actor.companyId));
 const row=(await client.query("SELECT id,kind,payload,created_by,created_person FROM iam_reviews WHERE id=$1 AND company_id=$2",[reviewId,actor.companyId])).rows[0];
 if(!row||!String(row.kind).startsWith("people.lifecycle."))return;
 const caseId=String(row.payload.caseId),actual=(await client.query("SELECT kind FROM people_lifecycle_cases WHERE id=$1 AND company_id=$2",[caseId,actor.companyId])).rows[0];
 if(!actual)throw new ApiError(404,"PEOPLE_SOURCE_NOT_FOUND","The lifecycle source was not found in this Company.");
 const original=lifecycleSource(client,actor.companyId,caseId),review=async()=>(await client.query("SELECT id,kind,payload,created_by,created_person FROM iam_reviews WHERE id=$1 AND company_id=$2",[reviewId,actor.companyId])).rows[0]??null;
 return prepare(client,actor,key,"people.lifecycle",decision==="reject"||["offboarding","suspend"].includes(actual.kind)?"settle-existing":"new-work",async()=>({review:await review(),...await original.source()}),async()=>({case:await original.identity(),review:await review()}),decision!=="approve"?undefined:async()=>{
  const current=await client.query(`SELECT 1 FROM assignments a WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'iam.manage')`,[row.payload.authorizingAssignmentId,actor.companyId,row.created_by]);
  if(!current.rowCount)throw new ApiError(403,"PEOPLE_IAM_PROPOSER_CHANGED","The original requester's authority changed before completion.");
 });
}
