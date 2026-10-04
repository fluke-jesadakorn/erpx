import { expect, it, vi } from "vitest";
import { cp, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { env, applyMigrations } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { executeCommand } from "../src/server/commands";
import { snapshotMoney } from "../src/server/company-money";
import { legalCommand, detail } from "../src/server/legal/service";
import { uploadDocument, documentAccess, documentBytes } from "../src/server/legal/documents";
import { queueInspection } from "../src/server/legal/processing";
import { LocalFileStore } from "../src/server/runtime/filesystem";
import { PDFDocument, StandardFonts } from "pdf-lib";
vi.mock("../src/server/runtime", () => ({runtimeEnv:async()=>env,holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES})}));
// Immutable Legal recovery uses a declared scanner fixture. Compiled browser UAT
// exercises the actual scanner/OCR binaries before production acceptance.
vi.mock("../src/server/attachments/scanner",()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:"recovery scanner fixture",definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:"recovery scanner fixture",definitionUpdatedAt:Date.now()})}));
vi.mock("../src/server/attachments/preview",()=>({attachmentPreviewReadiness:async()=>({ready:true,error:null}),detectedAttachmentMediaType:(_bytes:Uint8Array,type:string)=>type,sanitizedAttachmentPreview:async()=>null}));

const execute = promisify(execFile);
const hash = (bytes:string|Buffer) => createHash("sha256").update(bytes).digest("hex");
const tracks = { CONTROL_DB:"control",CONTROL_READ_DB:"control-read",H001_DB:"holding-write",H001_READ_DB:"holding-read",H002_DB:"holding-write",H002_READ_DB:"holding-read",H003_DB:"holding-write",H003_READ_DB:"holding-read" } as const;

async function tableDigests() {
  const digests: Record<string,{count:number;sha256:string}> = {};
  for(const binding of Object.keys(tracks) as (keyof typeof tracks)[]){
    const db=env[binding],tables=(await db.pool.query("SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename")).rows as {tablename:string}[];
    for (const {tablename} of tables) {
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(tablename)) throw new Error("UNSAFE_TABLE_NAME");
      const rows = (await db.pool.query(`SELECT row_to_json(record)::text payload FROM "${tablename}" record ORDER BY row_to_json(record)::text`)).rows;
      digests[`${binding}/${tablename}`] = {count:rows.length,sha256:hash(JSON.stringify(rows))};
    }
  }
  return digests;
}
async function fileDigests(root:string){
  const digests:Record<string,string>={};
  for(const name of (await readdir(root,{recursive:true,withFileTypes:true})).filter(entry=>entry.isFile())){
    const path=join(name.parentPath,name.name);digests[path.slice(root.length+1)]=hash(await readFile(path));
  }
  return Object.fromEntries(Object.entries(digests).sort(([a],[b])=>a.localeCompare(b)));
}

it("encrypts a complete isolated snapshot, restores immutable money and files, and rolls back only compatible releases", async () => {
  for(const [binding,track] of Object.entries(tracks)){
    const db=env[binding as keyof typeof tracks];await applyMigrations(db,[{name:"0001_baseline",track}]);
    await db.exec("CREATE TABLE engineering_recovery_track_evidence(binding TEXT PRIMARY KEY,payload TEXT NOT NULL)");
    await db.prepare("INSERT INTO engineering_recovery_track_evidence VALUES(?,?)").bind(binding,`Real snapshot data for ${binding}`).run();
  }
  const secondary:Record<string,{companyId:string;sourceId:string;objectKey:string;checksum:string}>={};
  for(const holding of ["H002","H003"] as const){
    const db=env[`${holding}_DB`],files=env[`${holding}_FILES`],fixture=await seedExpenseCompany(db,files),sourceId=crypto.randomUUID(),objectKey=`recovery/${fixture.company}/${sourceId}`,bytes=`Immutable ${holding} Company original`;
    await files.put(objectKey,bytes,{onlyIf:{etagDoesNotMatch:"*"}});
    const money=await snapshotMoney(db,fixture.owner,{sourceType:"engineering_recovery",sourceId,amountKind:"total",currency:"THB",transactionDate:"2026-10-03",amountUnits:"67890"});
    await executeCommand(db,fixture.owner,sourceId,"engineering.recovery-secondary",{objectKey},sourceId,1,{id:sourceId,version:1},[money.statement]);
    secondary[holding]={companyId:fixture.company,sourceId,objectKey,checksum:hash(bytes)};
  }
  const f = await seedExpenseCompany(env.H001_DB,env.H001_FILES), key = crypto.randomUUID();
  await env.H001_DB.exec("CREATE TABLE engineering_recovery_evidence(id TEXT PRIMARY KEY,version INTEGER NOT NULL,object_key TEXT NOT NULL)");
  const objectKey = `recovery/${f.company}/${key}`, originalBytes = "Source revision one; synthetic recovery evidence";
  await env.H001_FILES.put(objectKey,originalBytes,{onlyIf:{etagDoesNotMatch:"*"}});
  const money = await snapshotMoney(env.H001_DB,f.owner,{sourceType:"engineering_recovery",sourceId:key,amountKind:"total",currency:"THB",transactionDate:"2026-10-03",amountUnits:"12345"});
  await executeCommand(env.H001_DB,f.owner,key,"engineering.recovery-record",{objectKey},key,1,{id:key,version:1},[
    env.H001_DB.prepare("INSERT INTO engineering_recovery_evidence VALUES(?,1,?)").bind(key,objectKey),money.statement,
  ]);
  const legalOwner=await f.member("recovery-legal-owner","legal_manager"),legalReviewer=await f.member("recovery-legal-reviewer","legal_counsel"),legalOutsider=await f.member("recovery-legal-outsider","legal_counsel");
  await legalCommand(env.H001_DB,legalOwner,{action:"settings",enabled:true,aiEnabled:false},crypto.randomUUID());
  const legalInput={kind:"matter",title:"Immutable Legal recovery fixture",classification:"confidential",ownerId:legalOwner.userId,data:{parties:"Fictional parties; no real legal conclusion"}},legalKey=crypto.randomUUID(),legalBody={action:"create",item:legalInput};
  const legal=await legalCommand(env.H001_DB,legalOwner,legalBody,legalKey);
  const legalAct=async(id:string,action:string,extra:Record<string,unknown>={})=>legalCommand(env.H001_DB,legalOwner,{id,action,expectedVersion:(await detail(env.H001_DB,legalOwner,id)).item.version,...extra},crypto.randomUUID());
  await legalAct(legal.id,"member",{userId:legalReviewer.userId,access:"read"});
  const pdf=await PDFDocument.create(),font=await pdf.embedFont(StandardFonts.Helvetica),page=pdf.addPage();page.drawText("Synthetic original retained for Legal recovery",{x:40,y:700,font});const legalBytes=await pdf.save();
  await uploadDocument(env.H001_DB,env.H001_FILES,legalOwner,legal.id,(await detail(env.H001_DB,legalOwner,legal.id)).item.version,"Legal recovery original.pdf","application/pdf",legalBytes,crypto.randomUUID());
  const originalDocument=(await detail(env.H001_DB,legalOwner,legal.id)).documents[0],processing=await queueInspection(env.H001_DB,legalOwner,originalDocument.id,"ocr",crypto.randomUUID());
  const related=await legalCommand(env.H001_DB,legalOwner,{action:"create",item:{...legalInput,title:"Related preserved recovery evidence"}},crypto.randomUUID());
  await legalAct(legal.id,"link",{targetId:related.id,relation:"evidence"});await legalAct(related.id,"link",{targetId:legal.id,relation:"derived"});
  await legalAct(legal.id,"hold.create",{reason:"First recovered preservation notice"});await legalAct(related.id,"hold.create",{reason:"Second recovered preservation notice"});
  const legalSnapshot=await detail(env.H001_DB,legalOwner,legal.id);
  expect(legalSnapshot.holds).toHaveLength(2);expect(legalSnapshot.documents[0].checksum).toBe(hash(Buffer.from(legalBytes)));
  await expect(detail(env.H001_DB,legalOutsider,legal.id)).rejects.toMatchObject({code:"LEGAL_NOT_FOUND"});
  const immutableCommandCounts=await env.H001_DB.prepare("SELECT (SELECT count(*) FROM legal_events WHERE item_id=?) history,(SELECT count(*) FROM command_receipts WHERE record_id=?) receipts,(SELECT count(*) FROM audit_events WHERE record_id=?) audit,(SELECT count(*) FROM outbox_events WHERE aggregate_id=?) outbox").bind(legal.id,legal.id,legal.id,legal.id).first();
  const before = await tableDigests(), home = await mkdtemp(resolve(".runtime/tests/encrypted-recovery-"));
  try {
    for (const name of ["config","data/config","runtime","releases","offsite"])
      await mkdir(join(home,name),{recursive:true,mode:0o700});
    for(const holding of ["H001","H002","H003"] as const)await cp(env[`${holding}_FILES`].root,join(home,"data/files",holding.toLowerCase()),{recursive:true,mode:constants.COPYFILE_FICLONE});
    const beforeFiles=await fileDigests(join(home,"data/files"));
    const admin = new URL(process.env.ERPX_TEST_ADMIN_URL!);
    await writeFile(join(home,"config/admin.json"),JSON.stringify({host:admin.hostname,port:Number(admin.port),user:admin.username,password:admin.password}),{mode:0o600});
    await writeFile(join(home,"config/production.env"),"BETTER_AUTH_SECRET=synthetic-recovery-auth-secret\n",{mode:0o600});
    const databases = Object.fromEntries(Object.entries(env).filter(([,db])=>typeof db==="object"&&db!==null&&"pool" in db).map(([name,db])=>[name,(db as typeof env.H001_DB).pool.options.connectionString]));
    expect(Object.keys(databases).sort()).toEqual(Object.keys(tracks).sort());
    await writeFile(join(home,"data/config/databases.json"),JSON.stringify({databases}),{mode:0o600});
    await symlink(resolve(process.env.ERPX_TEST_PG_BIN??"/opt/homebrew/opt/postgresql@18/bin",".."),join(home,"runtime/postgres"));
    const releaseId="2026-10-03T00-00-00-000Z", previousId="2026-10-02T00-00-00-000Z", incompatibleId="2026-10-01T00-00-00-000Z";
    const release = join(home,"releases",releaseId), supplied = process.env.ERPX_ENGINEERING_RELEASE_DIR;
    let build: Record<string,unknown> = {id:releaseId,schemaSha256:"isolated-controller-fixture",softwareVersion:"isolated-controller-fixture"};
    if (supplied) {
      const path = resolve(supplied);
      if (!path.startsWith(resolve(".runtime/releases")+"/")) throw new Error("LOCAL_RELEASE_REQUIRED");
      build = JSON.parse(await readFile(join(path,"release.json"),"utf8"));
      if (build.sourceFingerprint!==process.env.ERPX_ENGINEERING_SOURCE_FINGERPRINT || build.node!=="24.20.0") throw new Error("RECOVERY_RELEASE_SOURCE_MISMATCH");
      const bytes=await readFile(join(path,"release-files.json"));
      if (hash(bytes)!==build.fileManifestSha256) throw new Error("RECOVERY_BUILD_MANIFEST_MISMATCH");
      const files=JSON.parse(bytes.toString()) as Record<string,string>;
      for (const [name,digest] of Object.entries(files)) {
        const file=resolve(path,name);
        if (!file.startsWith(path+"/") || hash(await readFile(file))!==digest) throw new Error("RECOVERY_BUILD_CHECKSUM_MISMATCH");
      }
      await cp(path,release,{recursive:true,mode:constants.COPYFILE_FICLONE});
    } else {
      await mkdir(release,{recursive:true});
      await writeFile(join(release,"server.js"),"// Isolated controller fixture; release verification supplies the compiled candidate.\n");
    }
    await writeFile(join(release,"release.json"),JSON.stringify({...build,id:releaseId}));
    await cp(release,join(home,"releases",previousId),{recursive:true,mode:constants.COPYFILE_FICLONE});
    await writeFile(join(home,"releases",previousId,"release.json"),JSON.stringify({...build,id:previousId}));
    await cp(release,join(home,"releases",incompatibleId),{recursive:true,mode:constants.COPYFILE_FICLONE});
    await writeFile(join(home,"releases",incompatibleId,"release.json"),JSON.stringify({...build,id:incompatibleId,schemaSha256:"incompatible-schema-fixture"}));
    await symlink(release,join(home,"current"));
    const operator=supplied?join(release,"ops/macos/erpxctl.py"):"ops/macos/erpxctl.py";
    const args=[operator,"--home",home,"--offline-test"];
    const backup=await execute("python3",[...args,"backup"],{timeout:120_000}), snapshot=backup.stdout.trim().replace("Backup: ","");
    const identity=join(home,"age-identity.txt");
    await execute("/opt/homebrew/bin/age-keygen",["-o",identity]);
    const recipient=(await execute("/opt/homebrew/bin/age-keygen",["-y",identity])).stdout.trim();
    await execute("python3",[...args,"--destination",join(home,"offsite"),"--recipient",recipient,"export",snapshot],{timeout:120_000});
    const snapshotId=snapshot.split("/").at(-1)!, archive=join(home,"offsite",`${snapshotId}.tar.age`), ciphertext=await readFile(archive);
    expect(ciphertext.subarray(0,22).toString()).toContain("age-encryption.org/v1");
    expect(ciphertext.includes(Buffer.from("synthetic-recovery-auth-secret"))).toBe(false);
    const expectedSnapshotManifest=hash(await readFile(join(snapshot,"checksums.json")));
    await rm(snapshot,{recursive:true});
    await execute("python3",[...args,"--identity",identity,"import",archive],{timeout:120_000});
    expect(hash(await readFile(join(snapshot,"checksums.json")))).toBe(expectedSnapshotManifest);
    await env.H001_DB.prepare("UPDATE engineering_recovery_evidence SET version=2 WHERE id=?").bind(key).run();
    await env.H001_DB.prepare("DELETE FROM engineering_recovery_evidence WHERE id=?").bind(key).run();
    await env.H001_DB.prepare("UPDATE legal_items SET title='Changed after backup',version=version+1 WHERE id=?").bind(legal.id).run();
    await env.H001_DB.prepare("DELETE FROM legal_members WHERE item_id=? AND user_id=?").bind(legal.id,legalReviewer.userId).run();
    await env.H001_DB.prepare("UPDATE legal_holds SET state='released' WHERE item_id IN (?,?)").bind(legal.id,related.id).run();
    await env.H001_DB.prepare("UPDATE legal_processing_jobs SET state='cancelled' WHERE id=?").bind(processing.jobId).run();
    for(const binding of Object.keys(tracks) as (keyof typeof tracks)[])await env[binding].prepare("UPDATE engineering_recovery_track_evidence SET payload='Changed after backup' WHERE binding=?").bind(binding).run();
    for(const holding of ["H002","H003"] as const)await env[`${holding}_DB`].prepare("UPDATE organization_nodes SET display_name='Changed after backup' WHERE id=?").bind(secondary[holding].companyId).run();
    for(const holding of ["h001","h002","h003"])await writeFile(join(home,"data/files",holding,"mutated-after-backup"),"Revision two");
    await writeFile(join(home,"data/files/mutated-after-backup"),"Revision two");
    await execute("python3",[...args,"restore",snapshot],{timeout:120_000});
    expect(await tableDigests()).toEqual(before);
    expect(await fileDigests(join(home,"data/files"))).toEqual(beforeFiles);
    const fileManifest=await readFile(join(home,"data/files/h001",createHash("sha256").update(objectKey).digest("hex"),"current.json"),"utf8");
    const stored=JSON.parse(fileManifest) as {version:string;etag:string};
    expect(await readFile(join(home,"data/files/h001",createHash("sha256").update(objectKey).digest("hex"),stored.version),"utf8")).toBe(originalBytes);
    expect(stored.etag).toBe(hash(originalBytes));
    const restoredLegal=await detail(env.H001_DB,legalReviewer,legal.id);
    expect(restoredLegal.item.title).toBe(legalInput.title);expect(restoredLegal.item.version).toBe(legalSnapshot.item.version);expect(restoredLegal.events).toEqual(legalSnapshot.events);expect(restoredLegal.holds.map(hold=>hold.id).sort()).toEqual(legalSnapshot.holds.map(hold=>hold.id).sort());expect(restoredLegal.members).toEqual(legalSnapshot.members);
    await expect(detail(env.H001_DB,legalOutsider,legal.id)).rejects.toMatchObject({code:"LEGAL_NOT_FOUND"});
    const restoredOriginal=await documentAccess(env.H001_DB,legalReviewer,originalDocument.id),restoredFiles=new LocalFileStore(join(home,"data/files/h001"),env.H001_DB);
    expect(hash(Buffer.from(await documentBytes(restoredFiles,restoredOriginal)))).toBe(originalDocument.checksum);
    const restoredJob=await env.H001_DB.prepare("SELECT state,checksum,attempts FROM legal_processing_jobs WHERE id=?").bind(processing.jobId).first();expect(restoredJob).toMatchObject({state:"queued",checksum:originalDocument.checksum,attempts:0});
    expect(await legalCommand(env.H001_DB,legalOwner,legalBody,legalKey)).toEqual(legal);
    expect(await env.H001_DB.prepare("SELECT (SELECT count(*) FROM legal_events WHERE item_id=?) history,(SELECT count(*) FROM command_receipts WHERE record_id=?) receipts,(SELECT count(*) FROM audit_events WHERE record_id=?) audit,(SELECT count(*) FROM outbox_events WHERE aggregate_id=?) outbox").bind(legal.id,legal.id,legal.id,legal.id).first()).toEqual(immutableCommandCounts);
    const legalRecovery={originalSha256:originalDocument.checksum,originalBytesRetained:true,historyRetained:true,aclRetained:true,unassignedReaderRefused:true,overlappingHoldsRetained:true,queuedJobRetained:true,commandReplayWithoutDuplicateEffects:true,scanner:"declared scanner fixture; actual scanner is covered by compiled browser UAT"};
    const beforeRollback=await tableDigests();
    for(const holding of ["H002","H003"] as const){
      const db=env[`${holding}_DB`],receipt=secondary[holding],store=new LocalFileStore(join(home,"data/files",holding.toLowerCase()),db);
      expect(hash(Buffer.from(await (await store.get(receipt.objectKey))!.arrayBuffer()))).toBe(receipt.checksum);
      expect(await db.prepare("SELECT amount_units::text amount,currency FROM money_transaction_snapshots WHERE company_id=? AND source_id=?").bind(receipt.companyId,receipt.sourceId).first()).toEqual({amount:"67890",currency:"THB"});
      expect(await db.prepare("SELECT count(*) n FROM command_receipts WHERE record_id=?").bind(receipt.sourceId).first("n")).toBe(1);
    }
    await execute("python3",[...args,"rollback",previousId]);
    expect((JSON.parse(await readFile(join(home,"current/release.json"),"utf8")) as {id:string}).id).toBe(previousId);
    expect(await tableDigests()).toEqual(beforeRollback);
    expect(await fileDigests(join(home,"data/files"))).toEqual(beforeFiles);
    await expect(execute("python3",[...args,"rollback",incompatibleId])).rejects.toThrow("Schema differs");
    expect((JSON.parse(await readFile(join(home,"current/release.json"),"utf8")) as {id:string}).id).toBe(previousId);
    await writeFile(archive,Buffer.concat([ciphertext,Buffer.from("tampered")]));
    await expect(execute("python3",[...args,"--identity",identity,"import",archive])).rejects.toThrow("OFFSITE_CIPHERTEXT_MISMATCH");
    const output=process.env.ERPX_ENGINEERING_EVIDENCE_DIR;
    if (output) await writeFile(join(output,"encrypted-recovery.json"),JSON.stringify({sourceFingerprint:build.sourceFingerprint??null,
      softwareVersion:build.softwareVersion,compiledCandidate:Boolean(supplied),fileManifestSha256:build.fileManifestSha256??null,
      databaseCount:Object.keys(databases).length,tracks:Object.keys(tracks),holdingFileStores:3,allHoldingFilesSha256:hash(JSON.stringify(beforeFiles)),tableCount:Object.keys(before).length,tableDigestSha256:hash(JSON.stringify(before)),
      ciphertextSha256:hash(ciphertext),sourceBytesSha256:stored.etag,restored:true,compatibleRollback:true,incompatibleRollbackRejected:true,
      ciphertextTamperingRejected:true,legal:legalRecovery,scope:"Isolated PostgreSQL18.6 and local FileStore; age encryption; no installed environment or external device"},null,2)+"\n",{mode:0o600});
  } finally { await rm(home,{recursive:true,force:true}); }
},180_000);
