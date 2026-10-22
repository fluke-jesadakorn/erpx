import type {Clock} from "@erpx/server-shared/application/ports";
import type {DestructionDocument,DestructionUpload,CanonicalSource,LegalDestructionManifest} from "@erpx/server-legal/domain/destruction-sources";
import type {LegalRows} from "@erpx/server-legal/application/service";
export type LegalDestructionRepository={
documents(facts:{itemId:string;}):Promise<LegalRows<DestructionDocument>>;
uploads(facts:{itemId:string;}):Promise<LegalRows<DestructionUpload>>;
canonicalSource(facts:{id:string;companyId:string;itemId:string;}):Promise<{ id: string; object_key: string; checksum_sha256: string; source_state: string; legal_hold: number; } | null>;
storedLedger(facts:{id:string;}):Promise<LegalRows<{ id: string; object_key: string; bytes: number; variant: string; }>>;
documentCurrent(facts:{id:string;itemId:string;checksum:string;object_key:string | null;}):Promise<Record<string, unknown> | null>;
uploadCurrent(facts:{id:string;itemId:string;checksum:string;object_key:string;}):Promise<Record<string, unknown> | null>;
lockedSource(facts:{id:string;companyId:string;itemId:string;}):Promise<{ object_key: string; checksum_sha256: string; source_state: string; legal_hold: number; } | null>;
linkCurrent(facts:{linkId:string;id:string;companyId:string;itemId:string;}):Promise<Record<string, unknown> | null>;
sourceShared(facts:{id:string;itemId:string;companyId:string;id2:string;itemId2:string;id3:string;itemId3:string;id4:string;}):Promise<Record<string, unknown> | null>;
retainedLedger(facts:{id:string;}):Promise<LegalRows<{ id: string; object_key: string; bytes: number; variant: string; }>>;
objectShared(facts:{objectKey:string;id:string;}):Promise<Record<string,unknown> | null>;
retireSource(facts:{now:number;id:string;companyId:string;}):Promise<void>;
retireLedger(facts:{now:number;id:string;}):Promise<void>;
recordRetirement(facts:{fact0:string;companyId:string;id:string;action:string;fact4:string;now:number;}):Promise<void>;
};
export function createLegalDestructionApplication(ports:{repository:LegalDestructionRepository;clock:Clock}){
/** Snapshot the existing File Sources records; this is the Legal owner adapter. */
async function legalDestructionManifest(itemId:string,companyId:string):Promise<LegalDestructionManifest> {
 const documents=(await ports.repository.documents({itemId:itemId})).results;
 const uploads=(await ports.repository.uploads({itemId:itemId})).results;
 const canonicalSources:CanonicalSource[]=[];
 for(const id of new Set([...documents,...uploads].map(row=>row.attachment_source_id).filter((id):id is string=>Boolean(id)))){
  const source=await ports.repository.canonicalSource({id:id,companyId:companyId,itemId:itemId});
  if(!source||source.source_state!=="available"||Number(source.legal_hold)!==0)throw new Error("LEGAL_DESTRUCTION_SOURCE_UNAVAILABLE");
  const references=[...documents,...uploads].filter(row=>row.attachment_source_id===id);
  if(references.some(row=>row.object_key!==source.object_key||row.checksum!==source.checksum_sha256||!row.attachment_link_id))throw new Error("LEGAL_DESTRUCTION_SOURCE_MISMATCH");
  const ledger=(await ports.repository.storedLedger({id:id})).results;
  if(!ledger.some(row=>row.variant==="original"&&row.object_key===source.object_key))throw new Error("LEGAL_DESTRUCTION_LEDGER_MISSING");
  canonicalSources.push({id,objectKey:source.object_key,checksum:source.checksum_sha256,linkIds:references.map(row=>row.attachment_link_id!),ledger:ledger.map(row=>({id:row.id,objectKey:row.object_key,bytes:Number(row.bytes),variant:row.variant}))});
 }
 const keys=[...new Set([...documents,...uploads].map(row=>row.object_key).filter((key):key is string=>Boolean(key)).concat(canonicalSources.flatMap(source=>source.ledger.map(row=>row.objectKey))))];
 const manifest={documents,uploads,keys,canonicalSources};
 await validateLegalDestructionSources(itemId,companyId,manifest);
 return manifest;
}

/** Re-check exact immutable references and shared ownership before byte deletion. */
async function validateLegalDestructionSources(itemId:string,companyId:string,manifest:LegalDestructionManifest) {
 for(const document of manifest.documents){
  if(!await ports.repository.documentCurrent({id:document.id,itemId:itemId,checksum:document.checksum,object_key:document.object_key}))throw new Error("LEGAL_DESTRUCTION_DOCUMENT_CHANGED");
 }
 for(const upload of manifest.uploads){
  if(!await ports.repository.uploadCurrent({id:upload.id,itemId:itemId,checksum:upload.checksum,object_key:upload.object_key}))throw new Error("LEGAL_DESTRUCTION_UPLOAD_CHANGED");
 }
 for(const source of manifest.canonicalSources??[]){
  const current=await ports.repository.lockedSource({id:source.id,companyId:companyId,itemId:itemId});
  if(!current||current.object_key!==source.objectKey||current.checksum_sha256!==source.checksum||!["available","destroyed"].includes(current.source_state)||Number(current.legal_hold)!==0)throw new Error("LEGAL_DESTRUCTION_SOURCE_CHANGED");
  for(const linkId of source.linkIds){
   if(!await ports.repository.linkCurrent({linkId:linkId,id:source.id,companyId:companyId,itemId:itemId}))throw new Error("LEGAL_DESTRUCTION_LINK_CHANGED");
  }
  const shared=await ports.repository.sourceShared({id:source.id,itemId:itemId,companyId:companyId,id2:source.id,itemId2:itemId,id3:source.id,itemId3:itemId,id4:source.id});
  if(shared)throw new Error("LEGAL_DESTRUCTION_SOURCE_SHARED");
  const currentLedger=(await ports.repository.retainedLedger({id:source.id})).results.map(row=>({id:row.id,objectKey:row.object_key,bytes:Number(row.bytes),variant:row.variant}));
  const expectedLedger=source.ledger.map(row=>({id:row.id,objectKey:row.objectKey,bytes:Number(row.bytes),variant:row.variant}));
  if(JSON.stringify(currentLedger)!==JSON.stringify(expectedLedger))throw new Error("LEGAL_DESTRUCTION_LEDGER_CHANGED");
  for(const object of source.ledger){
   if(!manifest.keys.includes(object.objectKey)||await ports.repository.objectShared({objectKey:object.objectKey,id:source.id}))throw new Error("LEGAL_DESTRUCTION_OBJECT_SHARED");
  }
 }
 for(const key of manifest.keys){
  const canonical=(manifest.canonicalSources??[]).some(source=>source.ledger.some(row=>row.objectKey===key));
  const original=[...manifest.documents,...manifest.uploads].some(row=>row.object_key===key&&!row.attachment_source_id);
  if(!canonical&&(!original||!key.startsWith(`legal/${companyId}/${itemId}/`)))throw new Error("LEGAL_DESTRUCTION_SCOPE");
 }
}

/** Retain immutable source/ledger history while retiring availability and quota. */
async function sealLegalDestructionSources(companyId:string,destructionId:string,manifest:LegalDestructionManifest) {
 const now=ports.clock.now();
 for(const source of manifest.canonicalSources??[]){
  await ports.repository.retireSource({now:now,id:source.id,companyId:companyId});
  await ports.repository.retireLedger({now:now,id:source.id});
  for(const action of ["source.destroyed","source.bytes_deleted"]){
   await ports.repository.recordRetirement({fact0:`legal-destruction-${destructionId}-${source.id}-${action}`,companyId:companyId,id:source.id,action:action,fact4:JSON.stringify({destructionId,checksum:source.checksum}),now:now});
  }
 }
}

return {legalDestructionManifest,validateLegalDestructionSources,sealLegalDestructionSources};}
