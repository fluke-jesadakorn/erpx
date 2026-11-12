import type {Clock} from "@erpx/server-shared/application/ports";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {DocumentVersion,LegalItem} from "@erpx/server-legal/domain/model";
export type LegalProcessingKind="ocr"|"verify";
export type LegalProcessingStatus={id:string;item_id:string;document_id:string;state:string;result_json:string|null;error:string|null;attempts:number};
export function createLegalProcessingApplication(ports:{
 repository:{queue(document:DocumentVersion,item:LegalItem,kind:LegalProcessingKind,key:string,rateSince:number,created:number):Promise<{id:string;jobId:string;state:string}>;status(id:string):Promise<LegalProcessingStatus|null>};
 documents:{access(actor:WorkContext,id:string):Promise<DocumentVersion>}; access(actor:WorkContext,id:string):Promise<LegalItem>;
 actor():WorkContext; clock:Clock; error(status:number,code:string,message:string):Error;
}){
 const source=ports.repository;function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
 async function queueInspection(documentId:string,kind:LegalProcessingKind,key:string){const document=await ports.documents.access(ports.actor(),documentId),item=await ports.access(ports.actor(),document.item_id);return source.queue(document,item,kind,key,ports.clock.now()-60000,ports.clock.now());}
 async function processingStatus(id:string){const row=await source.status(id);if(!row)return fail("Processing job unavailable.","LEGAL_NOT_FOUND",404);await ports.access(ports.actor(),row.item_id);return {...row,jobId:row.id,result:row.result_json?JSON.parse(row.result_json):null,result_json:undefined};}
 return {queueInspection,processingStatus};
}
