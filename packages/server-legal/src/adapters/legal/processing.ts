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
import {createLegalProcessingApplication,type LegalProcessingKind} from "@erpx/server-legal/application/processing";
import { legalTrustProcessing } from '@erpx/server-collaboration/adapters/native-legal-trust-processing';
import { legalRateCountPrefix } from '../legal-read-tail-r19-sources';
import { nativeInspectionSourceRead } from '../native-legal-auxiliary';
function application(db:SqlDatabase,a:WorkContext){return createLegalProcessingApplication({actor:()=>a,access:(actor,id)=>access(db,actor,id),documents:{access:(actor,id)=>documentAccess(db,actor,id)},clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
 queue:(d,item,kind,key,rateSince,created)=>withLegalPurchasedCommand(db,a,{key,id:item.id,operation:`legal.processing.${kind}`,purpose:"new-work",permission:"legal.read",additional:["legal.documents"]},async owner=>executeCommand(owner,a,key,`legal.processing.${kind}`,{documentId:d.id,kind},item.id,item.version,{id:item.id,jobId:key,state:"queued"},[
 ...guards(owner,a,item.id,item.version,key),assertion(owner,key,"processing-rate",`${legalRateCountPrefix}processing_jobs WHERE company_id=? AND created_at>?)<20`,[a.companyId,rateSince]),
 await legalTrustProcessing(owner,a.companyId,a.userId).job({ key, itemId: item.id, documentId: d.id, checksum: d.checksum, kind, actorJson: JSON.stringify(a), created })])),
 }});}
// Native owner read: the axum/sqlx port owns the document-to-item SQL and binds
// the document id and selected Company. No TypeScript SQL remains here and the
// transport fails closed; this lookup selects a candidate, not document access.
export function findInspectionSource(db:SqlDatabase,companyId:string,documentId:string){
 return nativeInspectionSourceRead<{id:string;version:number}>(companyId,documentId);
}
export async function queueInspection(db:SqlDatabase,a:WorkContext,documentId:string,kind:LegalProcessingKind,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).queueInspection(documentId,kind,key));
 return application(db,a).queueInspection(documentId,kind,key);
}
