import {withLegalPurchasedCommand,inLegacyLegalOperationsUnit} from '@erpx/server-legal/adapters/purchases';
import {legalPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {ApiError} from "@erpx/server-runtime/public/http";
import {access,guards} from "@erpx/server-legal/adapters/legal/access";
import {documentAccess} from "@erpx/server-legal/public/legal/documents";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand} from "@erpx/server-workflow/public/commands";
import {createLegalProcessingApplication,type LegalProcessingKind,type LegalProcessingStatus} from "@erpx/server-legal/application/processing";
function application(db:SqlDatabase,a:WorkContext){return createLegalProcessingApplication({actor:()=>a,access:(actor,id)=>access(db,actor,id),documents:{access:(actor,id)=>documentAccess(db,actor,id)},clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
 queue:(d,item,kind,key,rateSince,created)=>withLegalPurchasedCommand(db,a,{key,id:item.id,operation:`legal.processing.${kind}`,purpose:"new-work",permission:"legal.read",additional:["legal.documents"]},owner=>executeCommand(owner,a,key,`legal.processing.${kind}`,{documentId:d.id,kind},item.id,item.version,{id:item.id,jobId:key,state:"queued"},[
 ...guards(owner,a,item.id,item.version,key),assertion(owner,key,"processing-rate","(SELECT count(*) FROM legal_processing_jobs WHERE company_id=? AND created_at>?)<20",[a.companyId,rateSince]),
 owner.prepare("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,actor_json,created_by,created_at) VALUES(?,?,?,?,?,?,'queued',?,?,?)").bind(key,a.companyId,item.id,d.id,d.checksum,kind,JSON.stringify(a),a.userId,created)])),
 status:id=>db.prepare("SELECT id,item_id,document_id,state,result_json,error,attempts FROM legal_processing_jobs WHERE id=? AND company_id=?").bind(id,a.companyId).first<LegalProcessingStatus>(),
 }});}
// Used by the existing document-inspection transport until the native command
// accepts a document ID and resolves its item inside the authoritative unit.
// This lookup selects a candidate; it does not grant access to the document.
export function findInspectionSource(db:SqlDatabase,companyId:string,documentId:string){
 return db.prepare("SELECT i.id,i.version::int version FROM legal_items i JOIN legal_documents d ON d.item_id=i.id WHERE d.id=? AND i.company_id=?").bind(documentId,companyId).first<{id:string;version:number}>();
}
export async function queueInspection(db:SqlDatabase,a:WorkContext,documentId:string,kind:LegalProcessingKind,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).queueInspection(documentId,kind,key));
 return application(db,a).queueInspection(documentId,kind,key);
}
export function processingStatus(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).processingStatus(id);}
