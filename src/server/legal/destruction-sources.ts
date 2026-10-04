import "server-only";
import type {SqlDatabase} from "../runtime/types";

type Document = {id:string;checksum:string;object_key:string|null;attachment_source_id:string|null;attachment_link_id:string|null};
type Upload = {id:string;checksum:string;object_key:string;attachment_source_id:string|null;attachment_link_id:string|null};
type CanonicalSource = {id:string;objectKey:string;checksum:string;linkIds:string[];ledger:{id:string;objectKey:string;bytes:number;variant:string}[]};
export type LegalDestructionManifest = {documents:Document[];uploads:Upload[];keys:string[];canonicalSources:CanonicalSource[]};

/** Snapshot the existing File Sources records; this is the Legal owner adapter. */
export async function legalDestructionManifest(db:SqlDatabase,itemId:string,companyId:string):Promise<LegalDestructionManifest> {
 const documents=(await db.prepare("SELECT id,checksum,object_key,attachment_source_id,attachment_link_id FROM legal_documents WHERE item_id=? AND NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=legal_documents.id)").bind(itemId).all<Document>()).results;
 const uploads=(await db.prepare("SELECT upload.id,upload.object_key,upload.checksum,upload.attachment_source_id,upload.attachment_link_id FROM legal_external_uploads upload JOIN legal_shares share ON share.id=upload.share_id WHERE share.item_id=?").bind(itemId).all<Upload>()).results;
 const canonicalSources:CanonicalSource[]=[];
 for(const id of new Set([...documents,...uploads].map(row=>row.attachment_source_id).filter((id):id is string=>Boolean(id)))){
  const source=await db.prepare("SELECT id,object_key,checksum_sha256,source_state,legal_hold FROM attachments WHERE id=? AND company_id=? AND owner_type='legal_item' AND owner_id=?").bind(id,companyId,itemId).first<{id:string;object_key:string;checksum_sha256:string;source_state:string;legal_hold:number}>();
  if(!source||source.source_state!=="available"||Number(source.legal_hold)!==0)throw new Error("LEGAL_DESTRUCTION_SOURCE_UNAVAILABLE");
  const references=[...documents,...uploads].filter(row=>row.attachment_source_id===id);
  if(references.some(row=>row.object_key!==source.object_key||row.checksum!==source.checksum_sha256||!row.attachment_link_id))throw new Error("LEGAL_DESTRUCTION_SOURCE_MISMATCH");
  const ledger=(await db.prepare("SELECT id,object_key,bytes,variant FROM attachment_storage_ledger WHERE attachment_id=? AND state='stored' ORDER BY id").bind(id).all<{id:string;object_key:string;bytes:number;variant:string}>()).results;
  if(!ledger.some(row=>row.variant==="original"&&row.object_key===source.object_key))throw new Error("LEGAL_DESTRUCTION_LEDGER_MISSING");
  canonicalSources.push({id,objectKey:source.object_key,checksum:source.checksum_sha256,linkIds:references.map(row=>row.attachment_link_id!),ledger:ledger.map(row=>({id:row.id,objectKey:row.object_key,bytes:Number(row.bytes),variant:row.variant}))});
 }
 const keys=[...new Set([...documents,...uploads].map(row=>row.object_key).filter((key):key is string=>Boolean(key)).concat(canonicalSources.flatMap(source=>source.ledger.map(row=>row.objectKey))))];
 const manifest={documents,uploads,keys,canonicalSources};
 await validateLegalDestructionSources(db,itemId,companyId,manifest);
 return manifest;
}

/** Re-check exact immutable references and shared ownership before byte deletion. */
export async function validateLegalDestructionSources(db:SqlDatabase,itemId:string,companyId:string,manifest:LegalDestructionManifest) {
 for(const document of manifest.documents){
  if(!await db.prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=? AND checksum=? AND object_key IS NOT DISTINCT FROM ?").bind(document.id,itemId,document.checksum,document.object_key).first())throw new Error("LEGAL_DESTRUCTION_DOCUMENT_CHANGED");
 }
 for(const upload of manifest.uploads){
  if(!await db.prepare("SELECT 1 FROM legal_external_uploads upload JOIN legal_shares share ON share.id=upload.share_id WHERE upload.id=? AND share.item_id=? AND upload.checksum=? AND upload.object_key=?").bind(upload.id,itemId,upload.checksum,upload.object_key).first())throw new Error("LEGAL_DESTRUCTION_UPLOAD_CHANGED");
 }
 for(const source of manifest.canonicalSources??[]){
  const current=await db.prepare("SELECT object_key,checksum_sha256,source_state,legal_hold FROM attachments WHERE id=? AND company_id=? AND owner_type='legal_item' AND owner_id=? FOR UPDATE").bind(source.id,companyId,itemId).first<{object_key:string;checksum_sha256:string;source_state:string;legal_hold:number}>();
  if(!current||current.object_key!==source.objectKey||current.checksum_sha256!==source.checksum||!["available","destroyed"].includes(current.source_state)||Number(current.legal_hold)!==0)throw new Error("LEGAL_DESTRUCTION_SOURCE_CHANGED");
  for(const linkId of source.linkIds){
   if(!await db.prepare("SELECT 1 FROM attachment_links link JOIN attachment_sets owner ON owner.id=link.set_id WHERE link.id=? AND link.attachment_id=? AND owner.company_id=? AND owner.target_type='legal_item' AND owner.target_id=?").bind(linkId,source.id,companyId,itemId).first())throw new Error("LEGAL_DESTRUCTION_LINK_CHANGED");
  }
  const shared=await db.prepare(`SELECT 1 WHERE EXISTS(SELECT 1 FROM attachment_links link JOIN attachment_sets owner ON owner.id=link.set_id WHERE link.attachment_id=? AND link.state IN ('draft','bound') AND (owner.target_type<>'legal_item' OR owner.target_id<>? OR owner.company_id<>?))
   OR EXISTS(SELECT 1 FROM legal_file_preservations preservation WHERE preservation.source_type='attachment' AND preservation.source_id=? AND preservation.item_id<>?)
   OR EXISTS(SELECT 1 FROM legal_documents document WHERE document.attachment_source_id=? AND document.item_id<>?)
   OR EXISTS(SELECT 1 FROM attachment_publications publication JOIN attachment_links link ON link.id=publication.source_link_id WHERE link.attachment_id=? AND publication.state='approved')`).bind(source.id,itemId,companyId,source.id,itemId,source.id,itemId,source.id).first();
  if(shared)throw new Error("LEGAL_DESTRUCTION_SOURCE_SHARED");
  const currentLedger=(await db.prepare("SELECT id,object_key,bytes,variant FROM attachment_storage_ledger WHERE attachment_id=? AND state IN ('stored','destroyed') ORDER BY id").bind(source.id).all<{id:string;object_key:string;bytes:number;variant:string}>()).results.map(row=>({id:row.id,objectKey:row.object_key,bytes:Number(row.bytes),variant:row.variant}));
  const expectedLedger=source.ledger.map(row=>({id:row.id,objectKey:row.objectKey,bytes:Number(row.bytes),variant:row.variant}));
  if(JSON.stringify(currentLedger)!==JSON.stringify(expectedLedger))throw new Error("LEGAL_DESTRUCTION_LEDGER_CHANGED");
  for(const object of source.ledger){
   if(!manifest.keys.includes(object.objectKey)||await db.prepare("SELECT 1 FROM attachment_storage_ledger WHERE object_key=? AND attachment_id<>? AND state='stored'").bind(object.objectKey,source.id).first())throw new Error("LEGAL_DESTRUCTION_OBJECT_SHARED");
  }
 }
 for(const key of manifest.keys){
  const canonical=(manifest.canonicalSources??[]).some(source=>source.ledger.some(row=>row.objectKey===key));
  const original=[...manifest.documents,...manifest.uploads].some(row=>row.object_key===key&&!row.attachment_source_id);
  if(!canonical&&(!original||!key.startsWith(`legal/${companyId}/${itemId}/`)))throw new Error("LEGAL_DESTRUCTION_SCOPE");
 }
}

/** Retain immutable source/ledger history while retiring availability and quota. */
export async function sealLegalDestructionSources(db:SqlDatabase,companyId:string,destructionId:string,manifest:LegalDestructionManifest) {
 const now=Date.now();
 for(const source of manifest.canonicalSources??[]){
  await db.prepare("UPDATE attachments SET source_state='destroyed',destroyed_at=? WHERE id=? AND company_id=? AND source_state='available' AND legal_hold=0").bind(now,source.id,companyId).run();
  await db.prepare("UPDATE attachment_storage_ledger SET state='destroyed',destroyed_at=? WHERE attachment_id=? AND state='stored'").bind(now,source.id).run();
  for(const action of ["source.destroyed","source.bytes_deleted"]){
   await db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES(?,?,?,'system:legal',?,?,?) ON CONFLICT(id) DO NOTHING").bind(`legal-destruction-${destructionId}-${source.id}-${action}`,companyId,source.id,action,JSON.stringify({destructionId,checksum:source.checksum}),now).run();
  }
 }
}
