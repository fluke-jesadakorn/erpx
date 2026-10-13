import { beforeAll, expect, it } from "vitest";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { createHash } from "node:crypto";
import { env, applyMigrations } from "../../support/runtime-env";
import { preserveUnreferencedFiles, orphanObjectReferences, sealUnreferencedStoredObjects } from "../../../src/server/modules/files/public/attachments/orphan-preservation";
import { reconcileAttachmentStorage } from "../../../src/server/modules/files/public/attachments/migration";
import { LocalFileStore } from "../../../src/server/modules/runtime/adapters/runtime/filesystem";

beforeAll(async()=>{await applyMigrations(env.H001_DB,env.STAFF_TEST_MIGRATIONS);});
const environment:NodeJS.ProcessEnv={NODE_ENV:'test',ERPX_ENVIRONMENT:"local",ERPX_ATTACHMENTS_V2:"false",ERPX_ATTACHMENT_UPLOAD_FREEZE:"true"};
const db=env.H001_DB,files=env.H001_FILES;
it("preserves an orphan's complete version directory outside serving/quota with immutable replay evidence",async()=>{
  const objectKey=`unknown-owner/${crypto.randomUUID()}`,root=join(dirname(files.root),"preserved-orphans-h001"),bytes="Historical bytes; ownership deliberately unknown";
  const metadata=await files.put(objectKey,bytes,{customMetadata:{legacy:"preserved exactly"}});
  const input={db,files,objectKeys:[objectKey],preservationRoot:root,operatorId:"synthetic-storage-operator",reason:"Byte-preserving adjudication pending owner confirmation",environment};
  try {
    const [result]=await preserveUnreferencedFiles(input);
    expect(result).toMatchObject({objectKey,sha256:metadata!.etag,bytes:Buffer.byteLength(bytes),state:"preserved-pending-owner"});
    expect(await files.get(objectKey)).toBeNull();
    const manifest=JSON.parse(await readFile(join(result.preservedDirectory,"current.json"),"utf8"));
    expect(manifest.customMetadata).toEqual({legacy:"preserved exactly"});
    expect((await readFile(join(result.preservedDirectory,manifest.version))).toString()).toBe(bytes);
    expect(await preserveUnreferencedFiles(input)).toEqual([result]);
    expect(await db.prepare("SELECT COUNT(*)::int count FROM attachment_orphan_preservation_events WHERE preservation_id=?").bind(result.id).first("count")).toBe(1);
    expect(await db.prepare("SELECT COUNT(*)::int count FROM attachments WHERE object_key=?").bind(objectKey).first("count")).toBe(0);
    expect(await db.prepare("SELECT COUNT(*)::int count FROM attachment_storage_ledger WHERE object_key=?").bind(objectKey).first("count")).toBe(0);
    await expect(db.prepare("UPDATE attachment_orphan_preservations SET operator_id='changed' WHERE id=?").bind(result.id).run()).rejects.toMatchObject({code:"23514"});
  } finally { /* The isolated database/file workspace owns cleanup after all assertions. */ }
},60_000);
it("seals provider bytes without assigning an owner and verifies their integrity outside Company quota",async()=>{
  const objectKey=`unknown-provider/${crypto.randomUUID()}`,guarded=new LocalFileStore(files.root,db),metadata=await guarded.put(objectKey,"Provider historical original");
  const input={db,files:guarded,objectKeys:[objectKey],storeId:"h001",operatorId:"synthetic-storage-operator",reason:"Preserve provider original outside Company quota pending owner decision",environment};
  const reportId=`orphan-inspection-${crypto.randomUUID()}`;
  await db.prepare("INSERT INTO attachment_reconciliation_runs(id,mode,upload_frozen,checksums_verified,ready,report_json,created_at) VALUES(?,'dry_run',false,true,false,?,?)")
    .bind(reportId,JSON.stringify({orphanObjects:[objectKey]}),Date.now()).run();
  expect((await orphanObjectReferences(db,[objectKey])).get(objectKey)).toEqual([]);
  const [sealed]=await sealUnreferencedStoredObjects(input);
  expect(sealed).toMatchObject({objectKey,sha256:metadata!.etag,state:"sealed-pending-owner"});
  expect(await (await guarded.get(objectKey))!.text()).toBe("Provider historical original");
  expect(await sealUnreferencedStoredObjects(input)).toEqual([sealed]);
  expect(JSON.parse(String(await db.prepare("SELECT report_json FROM attachment_reconciliation_runs WHERE id=?").bind(reportId).first("report_json"))).orphanObjects).toEqual([objectKey]);
  const report=await reconcileAttachmentStorage(db,guarded,{verifyChecksums:true});
  expect(report.orphanObjects).not.toContain(objectKey);
  expect(report.preservedPendingOwner).toBeGreaterThan(0);expect(report.preservedUnownedBytes).toBeGreaterThan(0);expect(report.quotaDeltaBytes).toBe(0);
  await expect(guarded.put(objectKey,"Replacement without owner decision")).rejects.toThrow("FILE_PRESERVED_PENDING_OWNER");
  await expect(guarded.delete(objectKey)).rejects.toThrow("FILE_PRESERVED_PENDING_OWNER");
  const hash=createHash("sha256").update(objectKey).digest("hex"),manifest=JSON.parse(await readFile(join(files.root,hash,"current.json"),"utf8"));
  const {writeFile}=await import("node:fs/promises");await writeFile(join(files.root,hash,manifest.version),"Corruption");
  expect((await reconcileAttachmentStorage(db,guarded,{verifyChecksums:true})).checksumMismatches).toContain(`${sealed.id}:preserved_unowned_bytes`);
},60_000);
it("rejects referenced text/JSON/array objects, Legal keys, changed bytes and an unfrozen gate",async()=>{
  const objectKey=`unknown-owner/${crypto.randomUUID()}`,root=join(dirname(files.root),"preserved-orphans-h001"),input={db,files,objectKeys:[objectKey],preservationRoot:root,operatorId:"synthetic-storage-operator",reason:"Byte-preserving adjudication pending owner confirmation",environment};
  const original=await files.put(objectKey,"Original byte revision");
  await db.exec("CREATE TABLE preservation_reference_probe(id TEXT PRIMARY KEY,data JSONB,keys TEXT[])");
  await db.prepare("INSERT INTO preservation_reference_probe VALUES('probe',?::jsonb,ARRAY(SELECT jsonb_array_elements_text(?::jsonb)))").bind(JSON.stringify({objectKey}),JSON.stringify([objectKey])).run();
  expect((await orphanObjectReferences(db,[objectKey])).get(objectKey)!.map(ref=>ref.column)).toEqual(expect.arrayContaining(["data","keys"]));
  await expect(preserveUnreferencedFiles(input)).rejects.toThrow("ORPHAN_HAS_PERSISTED_REFERENCE");
  expect((await files.head(objectKey))!.etag).toBe(original!.etag);
  await db.prepare("DELETE FROM preservation_reference_probe").run();
  await expect(preserveUnreferencedFiles({...input,environment:{...environment,ERPX_ATTACHMENT_UPLOAD_FREEZE:"false"}})).rejects.toThrow("PRESERVATION_REQUIRES_LOCAL_FROZEN_GATE_DISABLED");
  await expect(preserveUnreferencedFiles({...input,objectKeys:["legal/unowned"]})).rejects.toThrow("EXPLICIT_ORPHAN_PRESERVATION_REQUIRED");
  const hash=createHash("sha256").update(objectKey).digest("hex"),manifest=JSON.parse(await readFile(join(files.root,hash,"current.json"),"utf8"));
  const {writeFile}=await import("node:fs/promises");await writeFile(join(files.root,hash,manifest.version),"Changed without an authorized source revision");
  await expect(preserveUnreferencedFiles(input)).rejects.toThrow("ORPHAN_BYTES_CHECKSUM_MISMATCH");
  expect(await db.prepare("SELECT COUNT(*)::int count FROM attachment_orphan_preservations WHERE object_key=?").bind(objectKey).first("count")).toBe(0);
},60_000);
