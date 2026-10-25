import { canonicalGuestAccount, legacyGuestAccount, parseGuestAccountJson } from "../src/lib/guest-account-compat";
import { expect, it, vi } from "vitest";
import { cp, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import sharp from "sharp";
import { env } from "./runtime-env";
import { migrateDatabase } from "../src/server/runtime/migrations";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { backfillSpecializedAttachments, reconcileAttachmentStorage } from "../src/server/attachments/migration";
import { scanAttachmentNow } from "../src/server/attachments/service";
import { defaultConfig } from "../src/lib/pms";
import { snapshotMoneyCarry } from "../src/server/company-money";

vi.mock("../src/server/modules/files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "Upgrade interface fixture", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: true, signature: null, engine: "Upgrade interface fixture", definitionUpdatedAt: Date.now() }),
}));
const stableHistory = (value: unknown): unknown => Array.isArray(value) ? value.map(stableHistory) : value && typeof value === "object" ? Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([key,item])=>[key,stableHistory(item)])) : value;
const hash = (value: string|Buffer) => createHash("sha256").update(value).digest("hex");
it("upgrades populated pre-completion databases without changing posted money or files and replays canonical backfill once", async () => {
  const supplied = process.env.ERPX_ENGINEERING_RELEASE_DIR;
  const current = supplied ? resolve(supplied, "migrations/postgres") : resolve("migrations/postgres");
  if (supplied && !resolve(supplied).startsWith(resolve(".runtime/releases") + "/")) throw new Error("LOCAL_UPGRADE_RELEASE_REQUIRED");
  const oldRoot = await mkdtemp(resolve(".runtime/tests/upgrade-baseline-"));
  const tracks = { CONTROL_DB: "control", CONTROL_READ_DB: "control-read", H001_DB: "holding-write", H001_READ_DB: "holding-read", H002_DB: "holding-write", H002_READ_DB: "holding-read", H003_DB: "holding-write", H003_READ_DB: "holding-read" } as const;
  try {
    for (const track of new Set(Object.values(tracks))) {
      await mkdir(join(oldRoot, track), { recursive: true });
      for (const name of (await readdir(join(current, track))).filter(name => name.endsWith(".sql") && Number(name.slice(0, 4)) <= 920))
        await cp(join(current, track, name), join(oldRoot, track, name));
    }
    for (const [binding, track] of Object.entries(tracks)) await migrateDatabase(env[binding as keyof typeof tracks], track, oldRoot);
    const db = env.H001_DB, files = env.H001_FILES, f = await seedExpenseCompany(db, files);
    const moneyId = `upgrade-money-${crypto.randomUUID()}`, journalId = `upgrade-journal-${crypto.randomUUID()}`;
    await db.prepare(`INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
      VALUES(?,'upgrade_sale',?,'total','2026-09-30','THB',2,360000,10000,?,false,1000)`).bind(f.company, moneyId, `${f.company}-thb-rate`).run();
    await db.batch([
      db.prepare(`INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at)
        VALUES(?,?,?,?,?,'2026-09-30','THB',1000000,'upgrade_sale',?,'Historical posted source','posted',?,?,1000,1000,1000)`).bind(journalId,f.company,f.chart,f.period,journalId,moneyId,f.reviewer.userId,f.manager.userId),
      db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,360000,0,360000,0,0)").bind(`${journalId}-debit`,journalId,f.accounts.cash),
      db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,0,360000,0,360000,1)").bind(`${journalId}-credit`,journalId,f.accounts.revenue),
    ]);
    const propertyId=`upgrade-hotel-${crypto.randomUUID()}`,reservationId=`upgrade-reservation-${crypto.randomUUID()}`,stayId=`upgrade-stay-${crypto.randomUUID()}`,unquotedStayId=`upgrade-unquoted-stay-${crypto.randomUUID()}`;
    const historicalAccountId=`upgrade-account-${crypto.randomUUID()}`,documentId=`upgrade-document-${crypto.randomUUID()}`,receiptId=`upgrade-receipt-${crypto.randomUUID()}`;
    const historicStay={reservationId,guestId:"historical-guest",typeId:"historical-type",rateId:"historical-rate",roomId:"",arrival:"2026-09-30",departure:"2026-10-01",adults:1,state:"reserved",guestAccountId:historicalAccountId,prices:[{date:"2026-09-30",net:10000,service:0,tax:700,total:10700}],segments:[]};
    await db.batch([
      db.prepare("INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at) VALUES(?,?,?,'Historical hotel','2026-09-30',?,?,1000)").bind(propertyId,f.company,f.branch,JSON.stringify(defaultConfig),f.owner.userId),
      db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'stay',?,3,1000)").bind(stayId,propertyId,JSON.stringify(legacyGuestAccount(historicStay))),
      db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'stay',?,1,1000)").bind(unquotedStayId,propertyId,JSON.stringify(legacyGuestAccount({...historicStay,reservationId:"no-historical-quote"}))),
      db.prepare(`INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
        VALUES(?,'pms_reservation',?,'room_total','2026-09-30','THB',2,10700,297,?,false,1000)`).bind(f.company,reservationId,`${f.company}-thb-rate`),
    ]);
    const historicalKind=legacyGuestAccount({kind:"guest_account"}).kind;
    const historicalColumn=Object.keys(legacyGuestAccount({guest_account_id:historicalAccountId}))[0];
    const frozen=JSON.stringify(legacyGuestAccount({guestAccount:{id:historicalAccountId,due:10700,deposit:3000},entries:[{guestAccountId:historicalAccountId,total:10700}]}));
    await db.batch([
      db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,4,1000)").bind(historicalAccountId,propertyId,historicalKind,JSON.stringify({name:"Historical payer",due:10700,advance:5000,deposit:3000,state:"open"})),
      db.prepare(`INSERT INTO pms_documents(id,property_id,${historicalColumn},kind,snapshot_json,created_at) VALUES(?,?,?,'invoice',?,1000)`).bind(documentId,propertyId,historicalAccountId,frozen),
      db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES(?,?,?,?,?,?,1000)").bind(receiptId,legacyGuestAccount({operation:"pms.guest_account.close"}).operation,f.owner.userId,"historical-command-hash",historicalAccountId,frozen),
      db.prepare("INSERT INTO iam_roles(company_id,id,name,published_revision,built_in) VALUES(?,'hospitality-custom','Reviewed mixed role',1,false)").bind(f.company),
      db.prepare("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES(?,'hospitality-custom',1,'[\"pos.configure\",\"pms.cashier\"]','[]','[\"holding\",\"branch\"]',?,1000)").bind(f.company,f.owner.userId),
    ]);
    const productId = `upgrade-product-${crypto.randomUUID()}`, imageId = `upgrade-image-${crypto.randomUUID()}`;
    const bytes = await sharp({ create: { width: 24, height: 12, channels: 4, background: "#006c70" } }).png().toBuffer();
    const thumbnail = await sharp(bytes).rotate().resize(480,480,{fit:"cover"}).webp({quality:82}).toBuffer(), checksum = hash(bytes);
    const objectKey = `pos/${f.company}/${productId}/${checksum}/original`, thumbnailKey = `pos/${f.company}/${productId}/${checksum}/thumbnail.webp`;
    await files.put(objectKey, bytes, { httpMetadata: { contentType: "image/png" } });
    await files.put(thumbnailKey, thumbnail, { httpMetadata: { contentType: "image/webp" } });
    await db.batch([
      db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,360000)").bind(productId,f.company,productId,"Historical POS product"),
      db.prepare(`INSERT INTO pos_product_images(id,company_id,product_id,object_key,thumbnail_key,sha256,width,height,mime_type,source,created_by,created_at)
        VALUES(?,?,?,?,?,?,24,12,'image/png','generated',?,1000)`).bind(imageId,f.company,productId,objectKey,thumbnailKey,checksum,f.owner.userId),
    ]);
    const history = async () => ({
      money: await db.prepare("SELECT company_id,source_type,source_id,amount_kind,transaction_date::text,currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback,captured_at FROM money_transaction_snapshots WHERE source_id=?").bind(moneyId).first(),
      journal: await db.prepare("SELECT * FROM finance_journals WHERE id=?").bind(journalId).first(),
      lines: (await db.prepare("SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal").bind(journalId).all()).results,
      postedEvidence: await db.prepare("SELECT * FROM finance_financial_events WHERE journal_id=?").bind(journalId).first(),
      immutableDocument:await db.prepare("SELECT snapshot_json FROM pms_documents WHERE id=?").bind(documentId).first(),
      immutableReceipt:await db.prepare("SELECT * FROM command_receipts WHERE command_id=?").bind(receiptId).first(),
      historicBooking:await db.prepare("SELECT company_id,source_type,source_id,amount_kind,transaction_date::text,currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback,captured_at FROM money_transaction_snapshots WHERE source_type='pms_reservation' AND source_id=?").bind(reservationId).first(),
      historicStays:(await db.prepare("SELECT * FROM pms_records WHERE property_id=? ORDER BY id").bind(propertyId).all<{kind:string;data_json:string}>()).results.map(row=>canonicalGuestAccount({...row,data_json:parseGuestAccountJson(row.data_json)})),
    });
    const before = await history(), beforeSha256 = hash(JSON.stringify(stableHistory(before)));
    const receipts = (await db.prepare("SELECT name,checksum FROM erpx_migrations ORDER BY name").all()).results;
    for (const [binding, track] of Object.entries(tracks)) await migrateDatabase(env[binding as keyof typeof tracks], track, current);
    expect(await history()).toEqual(before);
    expect(await db.prepare("SELECT guest_account_id FROM pms_documents WHERE id=?").bind(documentId).first()).toEqual({guest_account_id:historicalAccountId});
    const custom=await db.prepare("SELECT after_permissions FROM hospitality_permission_changes WHERE company_id=? AND role_id='hospitality-custom'").bind(f.company).first<{after_permissions:string[]}>();
    expect(typeof custom?.after_permissions === "string" ? JSON.parse(custom.after_permissions) : custom?.after_permissions).toEqual(expect.arrayContaining(["pos.configure","pms.cashier","pos.accounting.prepare","pos.accounting.approve"]));
    expect((await db.prepare("SELECT name,checksum FROM erpx_migrations WHERE name<= '0920zz' ORDER BY name").all()).results).toEqual(receipts);
    expect(await db.prepare("SELECT original_source_id FROM money_transaction_snapshots WHERE source_id=?").bind(moneyId).first()).toEqual({ original_source_id: null });
    const preservedPrice=await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id,transaction_date::text,carried_source_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_stay_price' AND source_id=?").bind(f.company,`${stayId}:3`).first();
    expect(preservedPrice).toEqual({amount_units:"10700",usd_units:"297",fx_rate_id:`${f.company}-thb-rate`,transaction_date:"2026-09-30",carried_source_id:reservationId});
    expect(await db.prepare("SELECT COUNT(*)::int n FROM money_transaction_snapshots WHERE source_type='pms_stay_price' AND source_id LIKE ?").bind(`${unquotedStayId}:%`).first("n")).toBe(0);
    const inherited=await snapshotMoneyCarry(db,{companyId:f.company},{sourceType:"pms_entry",sourceId:"upgraded-night-charge",amountKind:"total",transactionDate:"2026-10-01",amountUnits:"10700",original:{sourceType:"pms_stay_price",sourceId:`${stayId}:3`,amountKind:"2026-09-30"}});
    await inherited.statement.run();
    expect(inherited).toMatchObject({amountUnits:"10700",usdUnits:"297",rateId:`${f.company}-thb-rate`});
    await expect(db.prepare("UPDATE money_transaction_snapshots SET amount_units=1 WHERE source_id=?").bind(moneyId).run()).rejects.toMatchObject({ code: "23514" });
    await expect(db.prepare("UPDATE finance_journal_lines SET debit_minor=1 WHERE journal_id=? AND ordinal=0").bind(journalId).run()).rejects.toMatchObject({ code: "23514" });
    const imported = await backfillSpecializedAttachments(db, { apply: true, files });
    expect(imported).toMatchObject({ ready: 1, backfilled: 1, unknown: [] });
    const pointer = await db.prepare("SELECT attachment_source_id,thumbnail_checksum_sha256 FROM pos_product_images WHERE id=?").bind(imageId).first<{ attachment_source_id: string; thumbnail_checksum_sha256: string }>();
    expect(pointer!.thumbnail_checksum_sha256).toBe(hash(thumbnail));
    expect(await scanAttachmentNow(db,files,pointer!.attachment_source_id)).toBe("available");
    const reconciliation = await reconcileAttachmentStorage(db,files,{verifyChecksums:true});
    expect(reconciliation).toMatchObject({unknownOwnership:0,unavailable:0,orphanObjects:[],missingLedger:[],checksumMismatches:[],quotaDeltaBytes:0,crossCompanyStorage:[]});
    expect(hash(Buffer.from(await (await files.get(objectKey))!.arrayBuffer()))).toBe(checksum);
    expect(hash(Buffer.from(await (await files.get(thumbnailKey))!.arrayBuffer()))).toBe(hash(thumbnail));
    const appliedCount = await db.prepare("SELECT COUNT(*)::int count FROM erpx_migrations").first("count");
    for (const [binding, track] of Object.entries(tracks)) await migrateDatabase(env[binding as keyof typeof tracks],track,current);
    expect(await db.prepare("SELECT COUNT(*)::int count FROM erpx_migrations").first("count")).toBe(appliedCount);
    expect(await db.prepare("SELECT COUNT(*)::int n FROM money_transaction_snapshots WHERE source_type='pms_stay_price' AND source_id=?").bind(`${stayId}:3`).first("n")).toBe(1);
    expect(await backfillSpecializedAttachments(db,{apply:true,files})).toMatchObject({ready:0,backfilled:0,unknown:[]});
    const afterSha256 = hash(JSON.stringify(stableHistory(await history())));
    expect(afterSha256).toBe(beforeSha256);
    if (process.env.ERPX_ENGINEERING_EVIDENCE_DIR) {
      const output = resolve(process.env.ERPX_ENGINEERING_EVIDENCE_DIR);
      if (!output.startsWith(resolve(".scratch/dev/engineering-completion/runs")+"/")) throw new Error("PRIVATE_UPGRADE_EVIDENCE_DIRECTORY_REQUIRED");
      const build = supplied ? JSON.parse(await readFile(resolve(supplied,"release.json"),"utf8")) : null;
      await writeFile(join(output,"migration-upgrade.json"),JSON.stringify({version:1,sourceFingerprint:process.env.ERPX_ENGINEERING_SOURCE_FINGERPRINT,
        compiledCandidate:!!supplied,softwareVersion:build?.softwareVersion,fileManifestSha256:build?.fileManifestSha256,tracks:8,
        baselineBoundary:"0920",beforeSha256,afterSha256,priorReceiptsPreserved:true,backfillCount:1,replayBackfillCount:0,
        originalSha256:checksum,thumbnailSha256:hash(thumbnail),postedEvidenceImmutable:true,checksumsVerified:true,quotaDeltaBytes:reconciliation.quotaDeltaBytes,
        pmsPriceBackfill:{count:1,originalQuoteRetained:true,unquotedNotInvented:true,ownerHistoryRetained:true,replayCount:0},
        scanner:"declared clean interface fixture; real scanner is verified separately"},null,2)+"\n",{mode:0o600});
    }
  } finally { await rm(oldRoot,{recursive:true,force:true}); }
},90_000);
