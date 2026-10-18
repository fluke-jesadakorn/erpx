import {legalQueuedPurchasePredicate,withLegalQueuedCompletion} from '../queued-purchases';
import {withLegalPurchasedCommand,inLegacyLegalOperationsUnit} from '../purchases';
import {legalPackageEnforcementMode} from '../../../commercial/public/purchase-command-boundary';
import "server-only";
import type {SqlDatabase,FileStore} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {legalProcessingAssignmentCurrent,legalProcessingAuthority} from "../../../identity_access/public/legal-processing-authority";
import {ApiError} from "../../../runtime/public/http";
import {access,guards} from "./access";
import {documentAccess,inspectDocument} from "@/server/modules/legal/public/legal/documents";
import {legalRuntimePredicate,legalCompanyCapabilityReady} from "./runtime-gate";
import {executeCommand,assertion} from "../../../workflow/public/commands";
import {createLegalProcessingApplication,type LegalProcessingKind,type LegalProcessingJob,type LegalProcessingStatus} from "../../application/processing";
function application(db:SqlDatabase,a:WorkContext|undefined,files?:FileStore){return createLegalProcessingApplication({actor:()=>{if(!a)throw new Error("LEGAL_PROCESSING_ACTOR_REQUIRED");return a;},access:(actor,id)=>access(db,actor,id),documents:{access:(actor,id)=>documentAccess(db,actor,id),inspect:(actor,id,kind)=>{if(!files)throw new Error("LEGAL_PROCESSING_FILES_REQUIRED");return inspectDocument(db,files,actor,id,kind);}},companyReady:id=>legalCompanyCapabilityReady(db,id,"documents"),assignmentCurrent:(actor,from,until)=>legalProcessingAssignmentCurrent(db,actor,from,until),refreshAuthority:actor=>legalProcessingAuthority(db,actor),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
 queue:(d,item,kind,key,rateSince,created)=>withLegalPurchasedCommand(db,a!,{key,id:item.id,operation:`legal.processing.${kind}`,purpose:"new-work",permission:"legal.read",additional:["legal.documents"]},owner=>executeCommand(owner,a!,key,`legal.processing.${kind}`,{documentId:d.id,kind},item.id,item.version,{id:item.id,jobId:key,state:"queued"},[
 ...guards(owner,a!,item.id,item.version,key),assertion(owner,key,"processing-rate","(SELECT count(*) FROM legal_processing_jobs WHERE company_id=? AND created_at>?)<20",[a!.companyId,rateSince]),
 owner.prepare("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,actor_json,created_by,created_at) VALUES(?,?,?,?,?,?,'queued',?,?,?)").bind(key,a!.companyId,item.id,d.id,d.checksum,kind,JSON.stringify(a),a!.userId,created)])),
 status:id=>db.prepare("SELECT id,item_id,document_id,state,result_json,error,attempts FROM legal_processing_jobs WHERE id=? AND company_id=?").bind(id,a!.companyId).first<LegalProcessingStatus>(),
 recover:async now=>{await db.prepare("UPDATE legal_processing_jobs SET state=CASE WHEN attempts<3 THEN 'queued' ELSE 'failed' END,error='PROCESSING_INTERRUPTED',lease_token=NULL,lease_until=NULL WHERE state='running' AND lease_until<?").bind(now).run();},
 claim:async(token,until)=>db.prepare(`UPDATE legal_processing_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=? WHERE id=(SELECT j.id FROM legal_processing_jobs j WHERE j.state='queued' AND ${legalRuntimePredicate('j.company_id','documents')} AND EXISTS(SELECT 1 FROM legal_items i JOIN legal_documents d ON d.item_id=i.id WHERE i.id=j.item_id AND i.company_id=j.company_id AND d.id=j.document_id AND d.checksum=j.checksum AND ${await legalQueuedPurchasePredicate(db,'i','documents')}) ORDER BY j.created_at FOR UPDATE OF j SKIP LOCKED LIMIT 1) RETURNING *`).bind(token,until).first<LegalProcessingJob>(),
 complete:async(id,token,result)=>withLegalQueuedCompletion(db,"legal_processing_jobs",id,async owner=>{await owner.prepare("UPDATE legal_processing_jobs SET state='completed',result_json=?,error=NULL,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=? AND state='running'").bind(JSON.stringify(result),id,token).run();}),
 failed:async(id,token,code)=>{await db.prepare("UPDATE legal_processing_jobs SET state='failed',error=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?").bind(code,id,token).run();},
 }});}
export async function queueInspection(db:SqlDatabase,a:WorkContext,documentId:string,kind:LegalProcessingKind,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).queueInspection(documentId,kind,key));
 return application(db,a).queueInspection(documentId,kind,key);
}
export function processingStatus(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).processingStatus(id);}
export function drainLegalProcessing(db:SqlDatabase,files:FileStore){return application(db,undefined,files).drainLegalProcessing();}
