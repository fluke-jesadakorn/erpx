import { expect, it, vi } from "vitest";
import { cp, mkdir, mkdtemp, readFile, readdir, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { executeCommand } from "@erpx/server-workflow/public/commands";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import { legalCommand, detail } from "@erpx/server-legal/public/legal/service";
import { uploadDocument, documentAccess, documentBytes } from "@erpx/server-legal/public/legal/documents";
import { queueInspection } from "@erpx/server-legal/public/legal/processing";
import { LocalFileStore } from "@erpx/server-runtime/adapters/runtime/filesystem";
import { PDFDocument, StandardFonts } from "pdf-lib";
import {seedFinanceExpansionRehearsal} from "../../../fixtures/finance/expansion";
import {exportFinancialReport} from "@erpx/server-finance/public/finance-planning";
import {groupReportEditionExport} from "@erpx/server-finance/public/finance-group-editions";
import {pmsCommand,loadPms} from "../../../packages/server-pms/src/public/pms";
import {pmsAccountKeys,records,record,type Config} from "@erpx/server-pms/public/rules/pms";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({runtimeEnv:async()=>env,holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES})}));
// Immutable Legal recovery uses a declared scanner fixture. Compiled browser UAT
// exercises the actual scanner/OCR binaries before production acceptance.
vi.mock("@erpx/server-files/public/attachments/scanner",()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:"recovery scanner fixture",definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:"recovery scanner fixture",definitionUpdatedAt:Date.now()})}));
vi.mock("@erpx/server-files/public/attachments/preview",()=>({attachmentPreviewReadiness:async()=>({ready:true,error:null}),detectedAttachmentMediaType:(_bytes:Uint8Array,type:string)=>type,sanitizedAttachmentPreview:async()=>null}));

const execute = promisify(execFile);
const hash = (bytes:string|Buffer) => createHash("sha256").update(bytes).digest("hex");
const tracks = { CONTROL_DB:"control",CONTROL_READ_DB:"control-read",H001_DB:"holding-write",H001_READ_DB:"holding-read",H002_DB:"holding-write",H002_READ_DB:"holding-read",H003_DB:"holding-write",H003_READ_DB:"holding-read" } as const;
async function rollbackArtifact(directory:string, kind:string) {
  const releaseBytes=await readFile(join(directory,"release.json")),serverBytes=await readFile(join(directory,"server.js"));
  const releaseJsonSha256=hash(releaseBytes),serverSha256=hash(serverBytes);
  return {sha256:hash(JSON.stringify({kind,releaseJsonSha256,serverSha256})),releaseJsonSha256,serverSha256};
}

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
  const financeRehearsal=await seedFinanceExpansionRehearsal(env.H001_DB,env.H001_FILES);
  expect(financeRehearsal.yearProfit).toBe(-112300);expect(financeRehearsal.reconciliation.ready).toBe(true);
  // A real PMS command-owned fixture makes populated hotel recovery explicit.
  const hotel = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  const hotelOwner = hotel.owner;
  const hotelFinanceReviewer = await hotel.member('recovery-hotel-finance', 'finance_approver');
  const hotelInspector = await hotel.member('recovery-hotel-inspector', 'pms_supervisor');
  const hotelPropertyId = (await pmsCommand(env.H001_DB, hotelOwner, {
    operation: 'property.create', input: { name: 'Fictional recovery hotel', branchId: hotel.branch, businessDate: '2026-09-01' },
  }, crypto.randomUUID())).id;
  const hotelSnapshot = () => loadPms(env.H001_DB, hotelOwner, hotelPropertyId);
  const hotelCommand = async (operation: string, input: unknown = {}, actor = hotelOwner) => {
    const current = await hotelSnapshot();
    return pmsCommand(env.H001_DB, actor, { operation, propertyId: hotelPropertyId, expectedVersion: current.property.version, input }, crypto.randomUUID());
  };
  const hotelAccounts = {} as Config['accounts'];
  for (const [ordinal, account] of pmsAccountKeys.entries()) {
    const id = crypto.randomUUID();
    hotelAccounts[account] = id;
    const category = ['revenue', 'service'].includes(account) ? 'revenue'
      : ['advance', 'deposit', 'tax'].includes(account) ? 'liability'
        : ['consumption', 'commission', 'cashDifference'].includes(account) ? 'expense' : 'asset';
    await env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)")
      .bind(id, hotel.chart, hotel.company, `RECOVERY-PMS-${ordinal}`, account, category).run();
  }
  await hotelCommand('accounting.save', { chartId: hotel.chart, accounts: hotelAccounts, taxBps: 700, serviceBps: 0, taxInclusive: false, advanceTax: true });
  await hotelCommand('accounting.approve', {}, hotelFinanceReviewer);
  const hotelTypeId = (await hotelCommand('room_type.save', { name: 'Recovery Standard', capacity: 2, amenities: 'Queen bed and shower' })).id;
  const hotelRoomId = (await hotelCommand('room.save', { number: 'R-101', typeId: hotelTypeId, active: true })).id;
  const hotelRateId = (await hotelCommand('rate.save', { name: 'Recovery Flexible', typeId: hotelTypeId, from: '2026-09-01', to: '2026-12-31', price: 10000 })).id;
  await hotelCommand('property.enable');
  const hotelGuestId = (await hotelCommand('guest.save', { name: 'Fictional recovery guest' })).id;
  const hotelReservationId = (await hotelCommand('reservation.create', {
    guestId: hotelGuestId, source: 'direct', rooms: [{ typeId: hotelTypeId, rateId: hotelRateId, arrival: '2026-09-01', departure: '2026-09-02', adults: 2, quantity: 1 }],
  })).id;
  const hotelStay = records(await hotelSnapshot(), 'stay').find(stay => stay.data.reservationId === hotelReservationId)!;
  expect(hotelStay).toBeTruthy();
  const hotelTaskId = (await hotelCommand('task.create', { roomId: hotelRoomId, type: 'arrival', date: '2026-09-01' })).id;
  await hotelCommand('task.complete', { id: hotelTaskId, checked: record(await hotelSnapshot(), 'task', hotelTaskId).data.checklist });
  await hotelCommand('task.inspect', { id: hotelTaskId }, hotelInspector);
  await hotelCommand('stay.assign', { id: hotelStay.id, roomId: hotelRoomId });
  await hotelCommand('stay.check_in', { id: hotelStay.id });
  const hotelShiftId = (await hotelCommand('shift.open', { name: 'Recovery desk', opening: 0 })).id;
  const hotelPaymentKey = crypto.randomUUID();
  const hotelPaymentBody = { operation: 'payment.receive', propertyId: hotelPropertyId, expectedVersion: (await hotelSnapshot()).property.version,
    input: { guestAccountId: hotelStay.data.guestAccountId, amount: 10700, method: 'cash', shiftId: hotelShiftId } };
  const hotelPaymentResult = await pmsCommand(env.H001_DB, hotelOwner, hotelPaymentBody, hotelPaymentKey);
  await hotelCommand('payment.deposit', { guestAccountId: hotelStay.data.guestAccountId, amount: 5000, method: 'cash', shiftId: hotelShiftId });
  const hotelOriginal = await hotelSnapshot();
  expect(record(hotelOriginal, 'stay', hotelStay.id).data.state).toBe('in_house');
  expect(record(hotelOriginal, 'task', hotelTaskId).data.state).toBe('done');
  expect(record(hotelOriginal, 'room', hotelRoomId).data.cleanliness).toBe('inspected');
  const hotelDocuments = () => env.H001_DB.prepare('SELECT id,property_id,guest_account_id,kind,snapshot_json,created_at FROM pms_documents WHERE property_id=? ORDER BY id').bind(hotelPropertyId).all();
  const hotelDocumentsBefore = (await hotelDocuments()).results;
  expect(hotelDocumentsBefore.length).toBeGreaterThanOrEqual(2);
  const hotelLedger = () => env.H001_DB.prepare('SELECT count(DISTINCT j.id)::int journals,COALESCE(SUM(l.debit_minor),0)::text debit,COALESCE(SUM(l.credit_minor),0)::text credit FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.company_id=?').bind(hotel.company).first<{journals:number;debit:string;credit:string}>();
  const hotelLedgerBefore = (await hotelLedger())!;
  expect(hotelLedgerBefore.journals).toBeGreaterThan(0);
  expect(hotelLedgerBefore.debit).toBe(hotelLedgerBefore.credit);
  const hotelObjectKey = `recovery/${hotel.company}/${hotelReservationId}/confirmation.txt`;
  const hotelFileBytes = `Fictional PMS recovery original: ${hotelReservationId}`;
  await env.H001_FILES.put(hotelObjectKey, hotelFileBytes, { onlyIf: { etagDoesNotMatch: '*' } });
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
      if (build.sourceFingerprint!==process.env.ERPX_ENGINEERING_SOURCE_FINGERPRINT || build.node!=="26.11.1") throw new Error("RECOVERY_RELEASE_SOURCE_MISMATCH");
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
    const previousRelease=join(home,"releases",previousId);
    await mkdir(previousRelease,{recursive:true});
    await writeFile(join(previousRelease,"server.js"),"process.stdout.write('synthetic compatible prior release fixture\\n');\n");
    await writeFile(join(previousRelease,"release.json"),JSON.stringify({artifactKind:"synthetic-compatible-prior-fixture",id:previousId,
      schemaSha256:build.schemaSha256??"isolated-controller-fixture",softwareVersion:"synthetic-prior-release-fixture"}));
    await cp(release,join(home,"releases",incompatibleId),{recursive:true,mode:constants.COPYFILE_FICLONE});
    await writeFile(join(home,"releases",incompatibleId,"release.json"),JSON.stringify({...build,id:incompatibleId,schemaSha256:"incompatible-schema-fixture"}));
    await symlink(release,join(home,"current"));
    const operator=supplied?join(release,"ops/macos/erpxctl.py"):"ops/macos/erpxctl.py";
    const args=[operator,"--home",home,"--offline-test"];
    const backup=await execute("python3",[...args,"backup"],{timeout:120_000}), snapshot=backup.stdout.trim().replace("Backup: ","");
    const identity=join(home,"age-identity.txt");
    const ageKeygen=process.env.ERPX_AGE_KEYGEN_BIN??"age-keygen";
    await execute(ageKeygen,["-o",identity]);
    const recipient=(await execute(ageKeygen,["-y",identity])).stdout.trim();
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
    await env.H001_DB.prepare("UPDATE pms_properties SET name='Changed after backup',version=version+1 WHERE id=?").bind(hotelPropertyId).run();
    await env.H001_DB.prepare("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{name}','\"Changed after backup\"'::jsonb)::text,version=version+1 WHERE id=? AND kind='guest'").bind(hotelGuestId).run();
    await execute("python3",[...args,"restore",snapshot],{timeout:120_000});
    expect(await tableDigests()).toEqual(before);
    expect(await fileDigests(join(home,"data/files"))).toEqual(beforeFiles);
    expect(await hotelSnapshot()).toEqual(hotelOriginal);
    expect((await hotelDocuments()).results).toEqual(hotelDocumentsBefore);
    expect(await hotelLedger()).toEqual(hotelLedgerBefore);
    const restoredHotelFiles = new LocalFileStore(join(home,'data/files/h001'),env.H001_DB);
    expect(await (await restoredHotelFiles.get(hotelObjectKey))!.text()).toBe(hotelFileBytes);
    expect(await pmsCommand(env.H001_DB,hotelOwner,hotelPaymentBody,hotelPaymentKey)).toEqual(hotelPaymentResult);
    expect(await hotelSnapshot()).toEqual(hotelOriginal);
    expect((await hotelDocuments()).results).toEqual(hotelDocumentsBefore);
    expect(await hotelLedger()).toEqual(hotelLedgerBefore);

    expect(await exportFinancialReport(financeRehearsal.preparer,financeRehearsal.snapshotId)).toEqual(financeRehearsal.reportExport);
    expect(await groupReportEditionExport(financeRehearsal.f.owner,financeRehearsal.groupId)).toEqual(financeRehearsal.groupExport);
    expect(await env.H001_DB.prepare("SELECT status FROM finance_billing_corrections WHERE id=?").bind(financeRehearsal.correctionId).first("status")).toBe("posted");
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
    const candidateArtifact=await rollbackArtifact(release,"compiled-candidate-fixture"),previousArtifact=await rollbackArtifact(previousRelease,"synthetic-compatible-prior-fixture");
    expect(previousArtifact.serverSha256).not.toBe(candidateArtifact.serverSha256);
    expect(previousArtifact.sha256).not.toBe(candidateArtifact.sha256);
    const databaseStateBeforeSha256=hash(JSON.stringify(beforeRollback)),privateFilesStateBeforeSha256=hash(JSON.stringify(beforeFiles));
    await execute("python3",[...args,"rollback",previousId]);
    const selectedRelease=await realpath(join(home,"current")),expectedPreviousRelease=await realpath(previousRelease);
    expect(selectedRelease).toBe(expectedPreviousRelease);
    expect((JSON.parse(await readFile(join(home,"current/release.json"),"utf8")) as {id:string}).id).toBe(previousId);
    expect(hash(await readFile(join(home,"current/server.js")))).toBe(previousArtifact.serverSha256);
    const databaseStateAfter=await tableDigests(),privateFilesStateAfter=await fileDigests(join(home,"data/files"));
    expect(databaseStateAfter).toEqual(beforeRollback);
    expect(privateFilesStateAfter).toEqual(beforeFiles);
    const databaseStateAfterSha256=hash(JSON.stringify(databaseStateAfter)),privateFilesStateAfterSha256=hash(JSON.stringify(privateFilesStateAfter));
    expect(databaseStateAfterSha256).toBe(databaseStateBeforeSha256);
    expect(privateFilesStateAfterSha256).toBe(privateFilesStateBeforeSha256);
    await expect(execute("python3",[...args,"rollback",incompatibleId])).rejects.toThrow("Schema differs");
    expect((JSON.parse(await readFile(join(home,"current/release.json"),"utf8")) as {id:string}).id).toBe(previousId);
    expect(await realpath(join(home,"current"))).toBe(expectedPreviousRelease);
    const rollbackEvidence={scope:"isolated-synthetic-prior-release",candidateReleaseId:releaseId,previousReleaseId:previousId,
      candidateArtifactSha256:candidateArtifact.sha256,previousArtifactSha256:previousArtifact.sha256,
      candidateServerSha256:candidateArtifact.serverSha256,previousServerSha256:previousArtifact.serverSha256,
      switchedToPrevious:true,databaseAndFilesUnchanged:true,databaseStateBeforeSha256,databaseStateAfterSha256,
      privateFilesStateBeforeSha256,privateFilesStateAfterSha256};
    await writeFile(archive,Buffer.concat([ciphertext,Buffer.from("tampered")]));
    await expect(execute("python3",[...args,"--identity",identity,"import",archive])).rejects.toThrow("OFFSITE_CIPHERTEXT_MISMATCH");
    const output=process.env.ERPX_ENGINEERING_EVIDENCE_DIR;
    if (output) await writeFile(join(output,"encrypted-recovery.json"),JSON.stringify({sourceFingerprint:build.sourceFingerprint??null,
      softwareVersion:build.softwareVersion,compiledCandidate:Boolean(supplied),candidateReleaseId:releaseId,fileManifestSha256:build.fileManifestSha256??null,
      databaseCount:Object.keys(databases).length,tracks:Object.keys(tracks),holdingFileStores:3,allHoldingFilesSha256:hash(JSON.stringify(beforeFiles)),tableCount:Object.keys(before).length,tableDigestSha256:hash(JSON.stringify(before)),
      ciphertextSha256:hash(ciphertext),sourceBytesSha256:stored.etag,restored:true,compatibleRollback:true,incompatibleRollbackRejected:true,
      rollback:rollbackEvidence,
      pms:{propertyId:hotelPropertyId,reservationId:hotelReservationId,stayId:hotelStay.id,guestAccountId:hotelStay.data.guestAccountId,roomId:hotelRoomId,taskId:hotelTaskId,issuedDocuments:hotelDocumentsBefore.length,documentsSHA256:hash(JSON.stringify(hotelDocumentsBefore)),ledger:hotelLedgerBefore,privateFileSHA256:hash(hotelFileBytes),mutatedGuestAndHotelRestored:true,identicalPaymentReplay:true,originalPmsSnapshotRetained:true},
      ciphertextTamperingRejected:true,legal:legalRecovery,scope:"Isolated PostgreSQL18.6 and local FileStore; age encryption; no installed environment or external device"},null,2)+"\n",{mode:0o600});
  } finally { await rm(home,{recursive:true,force:true}); }
},180_000);
