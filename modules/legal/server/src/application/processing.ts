import type {Clock} from "@erpx/platform-shared-server/application/ports";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {DocumentVersion,LegalItem} from "@erpx/legal-server/domain/model";
export type LegalProcessingKind="ocr"|"verify";
export function createLegalProcessingApplication(ports:{
 repository:{queue(document:DocumentVersion,item:LegalItem,kind:LegalProcessingKind,key:string,rateSince:number,created:number):Promise<{id:string;jobId:string;state:string}>};
 documents:{access(actor:WorkContext,id:string):Promise<DocumentVersion>}; access(actor:WorkContext,id:string):Promise<LegalItem>;
 actor():WorkContext; clock:Clock; error(status:number,code:string,message:string):Error;
}){
 const source=ports.repository;function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
 async function queueInspection(documentId:string,kind:LegalProcessingKind,key:string){const document=await ports.documents.access(ports.actor(),documentId),item=await ports.access(ports.actor(),document.item_id);return source.queue(document,item,kind,key,ports.clock.now()-60000,ports.clock.now());}
 return {queueInspection};
}
