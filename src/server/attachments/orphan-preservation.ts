import "server-only";
import { createHash, randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, open, readFile, rename } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { LocalFileStore } from "../runtime/filesystem";
import { isTransactionalSqlDatabase, type SqlDatabase } from "../runtime/types";
import type { FileStore, FileMetadata } from "../runtime/types";
import { attachmentUploadsFrozen, attachmentsV2Enabled } from "./feature";

const digest = (value:string|Buffer) => createHash("sha256").update(value).digest("hex");
// An inspection report mentioning an unknown key does not assign a record
// owner. Including that diagnostic would make the first audit prevent every
// later preservation attempt. Owner records, command/audit payloads and holds
// remain part of the reference search; these append-only receipts stay intact.
const omitted = new Set(["attachment_orphan_preservations","attachment_orphan_preservation_events","attachment_reconciliation_runs"]);
const referenceProof=(key:string)=>digest(JSON.stringify({key,references:[],diagnosticTablesExcluded:[...omitted].sort()}));
export const preservationMetadataDigest=(head:FileMetadata)=>digest(JSON.stringify({key:head.key,size:head.size,etag:head.etag,uploaded:head.uploaded.toISOString(),httpMetadata:head.httpMetadata,customMetadata:head.customMetadata}));
type Reference = { table:string; column:string; rows:number };
export async function orphanObjectReferences(db:SqlDatabase,keys:string[]) {
  const references = new Map(keys.map(key => [key, [] as Reference[]]));
  if(!keys.length)return references;
  const columns = (await db.prepare(`SELECT table_name,column_name FROM information_schema.columns
    WHERE table_schema='public' AND (data_type IN ('text','character varying','json','jsonb') OR udt_name IN ('_text','_varchar'))
      AND table_name NOT LIKE 'pg_%' ORDER BY table_name,column_name`).all<{table_name:string;column_name:string}>()).results;
  for (const {table_name:table,column_name:column} of columns) {
    if (omitted.has(table)) continue;
    if (!/^[a-z_][a-z0-9_]*$/i.test(table) || !/^[a-z_][a-z0-9_]*$/i.test(column)) throw new Error("UNSAFE_PROVENANCE_IDENTIFIER");
    const hits = await db.prepare(`SELECT candidate.key,COUNT(*)::int rows FROM "${table}" source
      JOIN jsonb_array_elements_text(?::jsonb) candidate(key) ON strpos(source."${column}"::text,candidate.key)>0
      GROUP BY candidate.key`).bind(JSON.stringify(keys)).all<{key:string;rows:number}>();
    for (const row of hits.results) references.get(row.key)?.push({table,column,rows:row.rows});
  }
  return references;
}
export async function verifiedPreservedDirectory(directory:string,key:string) {
  const manifestBytes = await readFile(join(directory,"current.json")), manifest = JSON.parse(manifestBytes.toString()) as {key:string;version:string;size:number;etag:string};
  if (manifest.key!==key || !/^[a-f0-9-]{36}$/.test(manifest.version) || !/^[a-f0-9]{64}$/.test(manifest.etag) || !Number.isSafeInteger(manifest.size)) throw new Error("ORPHAN_MANIFEST_INVALID");
  const bytes = await readFile(join(directory,manifest.version));
  if (bytes.length!==manifest.size || digest(bytes)!==manifest.etag) throw new Error("ORPHAN_BYTES_CHECKSUM_MISMATCH");
  return {bytes:manifest.size,objectSha256:manifest.etag,manifestSha256:digest(manifestBytes)};
}
async function syncDirectory(path:string) { const handle=await open(path,"r");try{await handle.sync();}finally{await handle.close();} }
/** Keep a provider object intact and sealed; its bytes are tracked outside Company quota. */
export async function sealUnreferencedStoredObjects(input:{db:SqlDatabase;files:FileStore;objectKeys:string[];storeId:string;operatorId:string;reason:string;environment?:NodeJS.ProcessEnv}) {
  const environment=input.environment??process.env;
  if(environment.ERPX_ENVIRONMENT!=="local"||!attachmentUploadsFrozen(environment)||attachmentsV2Enabled(environment))throw new Error("PRESERVATION_REQUIRES_LOCAL_FROZEN_GATE_DISABLED");
  if(!isTransactionalSqlDatabase(input.db)||!input.objectKeys.length||new Set(input.objectKeys).size!==input.objectKeys.length||!/^h[a-z0-9]+$/.test(input.storeId)||!input.operatorId.trim()||input.reason.trim().length<20||input.objectKeys.some(key=>!key||key.startsWith("legal/")))throw new Error("EXPLICIT_ORPHAN_PRESERVATION_REQUIRED");
  const references=await orphanObjectReferences(input.db,input.objectKeys);
  if([...references.values()].some(refs=>refs.length))throw new Error("ORPHAN_HAS_PERSISTED_REFERENCE");
  const results=[];
  for(const objectKey of input.objectKeys) {
    const head=await input.files.head(objectKey),body=await input.files.get(objectKey);
    if(!head||!body)throw new Error("ORPHAN_OBJECT_MISSING");
    const bytes=Buffer.from(await body.arrayBuffer());if(bytes.length!==head.size||digest(bytes)!==head.etag)throw new Error("ORPHAN_BYTES_CHECKSUM_MISMATCH");
    const manifestSha256=preservationMetadataDigest(head),id=`orphan-preservation-${randomUUID()}`;
    await input.db.prepare(`INSERT INTO attachment_orphan_preservations(id,object_key,object_sha256,manifest_sha256,bytes,preserved_directory,reference_proof_sha256,operator_id,reason,created_at,preservation_mode)
      VALUES(?,?,?,?,?,?,?,?,?,?,'file-store-sealed') ON CONFLICT(object_key) DO NOTHING`).bind(id,objectKey,head.etag,manifestSha256,head.size,`holding-store:${input.storeId}/${digest(objectKey)}`,referenceProof(objectKey),input.operatorId,input.reason,Date.now()).run();
    const saved=await input.db.prepare("SELECT id,object_sha256,manifest_sha256,bytes,preservation_mode FROM attachment_orphan_preservations WHERE object_key=?").bind(objectKey).first<{id:string;object_sha256:string;manifest_sha256:string;bytes:number;preservation_mode:string}>();
    if(!saved||saved.preservation_mode!=="file-store-sealed"||saved.object_sha256!==head.etag||saved.manifest_sha256!==manifestSha256||Number(saved.bytes)!==head.size)throw new Error("PRESERVATION_SOURCE_CHANGED");
    const current=await input.files.head(objectKey);if(!current||preservationMetadataDigest(current)!==manifestSha256)throw new Error("ORPHAN_CHANGED_AFTER_PROOF");
    await input.db.prepare("INSERT INTO attachment_orphan_preservation_events(id,preservation_id,event_kind,operator_id,recorded_at) VALUES(?,?,'preserved',?,?) ON CONFLICT(preservation_id,event_kind) DO NOTHING").bind(`preservation-event-${randomUUID()}`,saved.id,input.operatorId,Date.now()).run();
    results.push({id:saved.id,objectKey,bytes:head.size,sha256:head.etag,state:"sealed-pending-owner" as const});
  }
  return results;
}
/** Move an entire version directory intact; do not make an Attachment or grant access. */
type PreservationInput={db:SqlDatabase;files:LocalFileStore;objectKeys:string[];preservationRoot:string;operatorId:string;reason:string;environment?:NodeJS.ProcessEnv};
export async function preserveUnreferencedFiles(input:PreservationInput) {
  const environment=input.environment??process.env;
  if (environment.ERPX_ENVIRONMENT!=="local" || !attachmentUploadsFrozen(environment) || attachmentsV2Enabled(environment)) throw new Error("PRESERVATION_REQUIRES_LOCAL_FROZEN_GATE_DISABLED");
  if (!isTransactionalSqlDatabase(input.db)) throw new Error("POSTGRES_PRESERVATION_REQUIRED");
  if (!input.objectKeys.length || new Set(input.objectKeys).size!==input.objectKeys.length) throw new Error("EXACT_DISTINCT_ORPHAN_KEYS_REQUIRED");
  const references=await orphanObjectReferences(input.db,input.objectKeys);
  if([...references.values()].some(refs=>refs.length)) throw new Error("ORPHAN_HAS_PERSISTED_REFERENCE");
  const records=[];
  for(const objectKey of input.objectKeys) records.push(await preparePreservation({...input,objectKey}));
  // Check every text/JSON reference again after durable intents and before any
  // directory moves. Frozen upload entry points cannot bind these orphan keys.
  const currentReferences=await orphanObjectReferences(input.db,input.objectKeys);
  if([...currentReferences.values()].some(refs=>refs.length)) throw new Error("ORPHAN_HAS_PERSISTED_REFERENCE");
  const results=[];
  for(const record of records) results.push(await finishPreservation(input,record));
  return results;
}
async function preparePreservation(input:PreservationInput&{objectKey:string}) {
  if (!isTransactionalSqlDatabase(input.db)) throw new Error("POSTGRES_PRESERVATION_REQUIRED");
  const {objectKey,files}=input;
  if (!objectKey || objectKey.startsWith("legal/") || !input.operatorId.trim() || input.reason.trim().length<20) throw new Error("EXPLICIT_ORPHAN_PRESERVATION_REQUIRED");
  const root=resolve(input.preservationRoot),canonicalRoot=resolve(files.root);
  if (root===canonicalRoot || root.startsWith(canonicalRoot+"/") || dirname(root)!==dirname(canonicalRoot)) throw new Error("PRESERVATION_MUST_BE_NON_SERVING_SIBLING_STORE");
  const objectDirectory=join(canonicalRoot,digest(objectKey));
  const record=await input.db.transaction(async tx=>{
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`orphan-preservation:${objectKey}`).run();
    const prior=await tx.prepare("SELECT * FROM attachment_orphan_preservations WHERE object_key=?").bind(objectKey).first<{
      id:string;object_key:string;object_sha256:string;manifest_sha256:string;bytes:number;preserved_directory:string;
    }>();
    if(prior) return prior;
    const proof=await verifiedPreservedDirectory(objectDirectory,objectKey),id=`orphan-preservation-${randomUUID()}`,destination=join(root,id);
    await tx.prepare(`INSERT INTO attachment_orphan_preservations(id,object_key,object_sha256,manifest_sha256,bytes,preserved_directory,reference_proof_sha256,operator_id,reason,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?)`).bind(id,objectKey,proof.objectSha256,proof.manifestSha256,proof.bytes,destination,referenceProof(objectKey),input.operatorId,input.reason,Date.now()).run();
    return {id,object_key:objectKey,object_sha256:proof.objectSha256,manifest_sha256:proof.manifestSha256,bytes:proof.bytes,preserved_directory:destination};
  });
  return record;
}
async function finishPreservation(input:PreservationInput,record:{id:string;object_key:string;object_sha256:string;manifest_sha256:string;bytes:number;preserved_directory:string}) {
  const objectKey=record.object_key,root=resolve(input.preservationRoot),canonicalRoot=resolve(input.files.root),objectDirectory=join(canonicalRoot,digest(objectKey));
  if (!record.preserved_directory.startsWith(root+"/")) throw new Error("PRESERVATION_ROOT_CHANGED");
  if (await input.db.prepare("SELECT 1 FROM attachment_orphan_preservation_events WHERE preservation_id=? AND event_kind='restored'").bind(record.id).first()) throw new Error("PRESERVATION_ALREADY_RESTORED");
  if (existsSync(objectDirectory)) {
    if(existsSync(record.preserved_directory)) throw new Error("PRESERVATION_DESTINATION_EXISTS");
    const proof=await verifiedPreservedDirectory(objectDirectory,objectKey);
    if(proof.objectSha256!==record.object_sha256 || proof.manifestSha256!==record.manifest_sha256) throw new Error("ORPHAN_CHANGED_AFTER_PROOF");
    await mkdir(root,{recursive:true,mode:0o700});
    await rename(objectDirectory,record.preserved_directory);
    await syncDirectory(canonicalRoot);await syncDirectory(root);
  }
  const preserved=await verifiedPreservedDirectory(record.preserved_directory,objectKey);
  if(preserved.objectSha256!==record.object_sha256 || preserved.manifestSha256!==record.manifest_sha256 || preserved.bytes!==Number(record.bytes)) throw new Error("PRESERVATION_CHECKSUM_MISMATCH");
  await input.db.prepare("INSERT INTO attachment_orphan_preservation_events(id,preservation_id,event_kind,operator_id,recorded_at) VALUES(?,?,'preserved',?,?) ON CONFLICT(preservation_id,event_kind) DO NOTHING")
    .bind(`preservation-event-${randomUUID()}`,record.id,input.operatorId,Date.now()).run();
  return {id:record.id,objectKey,bytes:preserved.bytes,sha256:preserved.objectSha256,preservedDirectory:record.preserved_directory,state:"preserved-pending-owner" as const};
}
