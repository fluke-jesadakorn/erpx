import type { Clock, IdentifierSource } from "@erpx/server-shared/application/ports";
import type { LegalItem } from "@erpx/server-legal/domain/model";
import { legalReferenceInput,referenceTarget,type LegalReferenceInput,type LegalReferenceKind } from "@erpx/server-legal/domain/references";
export function createLegalReferencesApplication(ports:{
 access(id:string,permission?:string):Promise<LegalItem>; requirePeople():void; target(kind:LegalReferenceKind,id:string):Promise<Record<string,unknown>>;
 repository:{list(id:string):Promise<{target_module:LegalReferenceKind;target_id:string}[]>;replay(key:string,itemId:string,raw:unknown):Promise<Record<string,unknown>|null>;commit(input:LegalReferenceInput,item:LegalItem,raw:unknown,key:string,times:{created:number;updated:number;event:number},eventId:string):Promise<{id:string;version:number}>};clock:Clock;identifiers:IdentifierSource;
}){
 async function target(kind:LegalReferenceKind,id:string){if(kind==="person")ports.requirePeople();return referenceTarget(kind,id,await ports.target(kind,id));}
 async function legalReferences(id:string){await ports.access(id);const visible=[];for(const row of await ports.repository.list(id)){try{visible.push(await target(row.target_module,row.target_id));}catch{/* An inaccessible target does not reveal its existence or label. */}}return visible;}
 async function addLegalReference(raw:unknown,key:string){const input=legalReferenceInput.parse(raw),item=await ports.access(input.id,"legal.edit");const prior=await ports.repository.replay(key,item.id,raw);if(prior)return prior;await target(input.targetKind,input.targetId);return ports.repository.commit(input,item,raw,key,{created:ports.clock.now(),updated:ports.clock.now(),event:ports.clock.now()},ports.identifiers.next());}
 return {legalReferences,addLegalReference};
}
