import { afterEach } from "vitest";
import { reviewedFinanceGrant } from "../../support/helpers/finance-purchase-fixture";
import type { FinanceRow } from "@erpx/server-finance/public/finance";
import { beforeAll,beforeEach,describe,expect,it,vi } from "vitest";
import { applyMigrations,env } from "../../support/runtime-env";
import { expenseDb as db,expenseFixture,key } from "../../support/helpers/expense-fixture";
import { prepareAccountingProfile,decideAccountingProfile } from "@erpx/server-finance/public/accounting-profiles";
import { beginClose,calculateCloseReconciliations,closePeriod,cancelClose,createAccount,createInvoice,createParty,invoiceSchema,partySchema,postInvoice } from "@erpx/server-finance/public/finance";
import { prepareVatEntry,reviewVatEntry,vatEntrySchema,prepareVatCorrection,reviewVatCorrection,vatCorrectionSchema,prepareVatPackage,approveVatPackage,exportVatWorkpaper,recordVatFiling,supersedeVatPackage,vatPackageDetail,vatPackageExport,vatWorkspace,taxEntryDetail,vatCorrectionDetail } from "@erpx/server-finance/public/finance-tax-register";
import { assignFlowPrimary,resolveFlow } from "@erpx/server-workflow/public/flow-routing";

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,outputAccount:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
  f=await expenseFixture();for(const [actor,role] of [[f.reviewer,"finance_preparer"],[f.manager,"finance_approver"]] as const){actor.role=role;actor.branchId=null;actor.departmentId=null;actor.assignmentId=await f.grant(actor,role);}
  outputAccount=String((await createAccount(f.reviewer,{chartId:f.chart,code:"2300",name:"Output VAT",category:"liability",controlKind:"none",isControl:false},key())).id);
  const p=await prepareAccountingProfile(f.reviewer,{statutoryChartId:f.chart,standard:"TFRS for NPAEs",fiscalStartMonth:1,effectiveDate:"2026-01-01",eligibilityEvidence:"Fixture eligible entity without public accountability",policyEvidence:"Reviewed THB VAT account mappings and Company policies",taxBasis:"current_tax",inventoryPolicy:"moving_average",depreciationPolicy:"straight_line"},key());
  await decideAccountingProfile(f.manager,p.id,"approve",{expectedVersion:1,reason:"Independent eligibility and statutory ledger review"},key());
});
async function invoice(kind:"receivable"|"payable"="receivable",subtotal="100",tax="7"){
  const party=await createParty(f.reviewer,partySchema.parse({kind:kind==="receivable"?"customer":"supplier",code:key(),name:"คู่ค้าทดสอบ VAT",currency:"THB",taxRegistration:"0105559999999"}),key());
  const i=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party.id,kind,invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-30",currency:"THB",subtotal,tax,taxAccountId:kind==="receivable"?outputAccount:f.accounts.tax,debitAccountId:kind==="receivable"?f.accounts.cash:f.accounts.expense,creditAccountId:kind==="receivable"?f.accounts.revenue:f.accounts.payable}),key());
  const flow=await resolveFlow(db,f.owner,"invoice",i.id);await assignFlowPrimary(db,f.owner,"invoice",i.id,{stepKey:"draft",userId:f.manager.userId,reason:"Independent invoice posting fixture",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  await postInvoice(f.manager,i.id,{expectedVersion:1,periodId:f.period,entryDate:"2026-09-02"},key());return i.id;
}
const facts=(id:string,extra:Record<string,unknown>={})=>vatEntrySchema.parse({sourceType:"invoice",sourceId:id,expectedSourceVersion:2,taxPointDate:"2026-09-01",claimMonth:"2026-09",documentReference:`VAT-${id}`,treatment:"standard",deductibleBps:10000,legalBasis:"Reviewed Revenue Code VAT point and document eligibility",taxPointEvidence:"Reviewed full goods delivery and full document VAT on September 1",documentEvidence:"Original tax invoice and branch identity retained with the source",...extra});
async function approvedEntry(id:string,extra:Record<string,unknown>={}){
  const e=await prepareVatEntry(f.reviewer,facts(id,extra),key());await reviewVatEntry(f.manager,e.id,"approve",{expectedVersion:1,reason:"Independent tax-point, document and GL review"},key());return e.id;
}
const packageInput=()=>({chartId:f.chart,filingMonth:"2026-09",filingKind:"original" as const,reason:"Prepared the complete independently reviewed monthly VAT register"});

describe("Source-derived Thai VAT register and filing preparation",()=>{
  it("derives tax from posted documents, keeps separate dates and independently reviews deductions",async()=>{
    const output=await invoice(),input=await invoice("payable");
    const prepared=await prepareVatEntry(f.reviewer,facts(output),key());expect(prepared).toMatchObject({taxMinor:700,deductibleMinor:700});
    await expect(reviewVatEntry({...f.reviewer,role:"finance_approver"},prepared.id,"approve",{expectedVersion:1,reason:"Cannot approve one's own tax facts"},key())).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
    await reviewVatEntry(f.manager,prepared.id,"approve",{expectedVersion:1,reason:"Reviewed original VAT invoice and goods delivery evidence"},key());
    await approvedEntry(input,{treatment:"allocated",deductibleBps:5000,claimMonth:"2026-10"});
    const september=await vatWorkspace(f.manager,{filingMonth:"2026-09"});expect(september.entries).toHaveLength(1);expect(september.reconciliation).toMatchObject({ready:true,missing:0});
    const october=await vatWorkspace(f.manager,{filingMonth:"2026-10"});expect(october.entries[0]).toMatchObject({document_date:"2026-09-01",accounting_date:"2026-09-02",tax_point_date:"2026-09-01",claim_month:"2026-10",tax_minor:700,deductible_minor:350});
    await expect(db.prepare("UPDATE finance_tax_entries SET tax_minor=0 WHERE id=?").bind(prepared.id).run()).rejects.toThrow("REVIEWED_TAX_ENTRY_IMMUTABLE");
  });
  it("rejects cross-Company records, premature claims, prohibited output deductions and stale sources",async()=>{
    const id=await invoice();await expect(prepareVatEntry({...f.reviewer,companyId:"other"},facts(id),key())).rejects.toMatchObject({code:"POSTED_TAX_SOURCE_REQUIRED"});
    await expect(prepareVatEntry(f.reviewer,facts(id,{claimMonth:"2026-08"}),key())).rejects.toMatchObject({code:"VAT_CLAIM_MONTH_INVALID"});
    await expect(prepareVatEntry(f.reviewer,facts(id,{treatment:"disallowed",deductibleBps:0}),key())).rejects.toMatchObject({code:"OUTPUT_VAT_TREATMENT_INVALID"});
    const entry=await prepareVatEntry(f.reviewer,facts(id),key());await db.prepare("UPDATE finance_invoices SET version=version+1 WHERE id=?").bind(id).run();
    await expect(reviewVatEntry(f.manager,entry.id,"approve",{expectedVersion:1,reason:"Reject source changes after preparation"},key())).rejects.toMatchObject({code:"TAX_SOURCE_CHANGED"});
    await reviewVatEntry(f.manager,entry.id,"reject",{expectedVersion:1,reason:"Source changed; retain old draft and prepare latest version"},key());
    const fresh=await prepareVatEntry(f.reviewer,facts(id,{expectedSourceVersion:3}),key());expect(fresh.status).toBe("prepared");
  });
  it("prevents incomplete and stale packages and retains superseded versions",async()=>{
    const id=await invoice();await expect(prepareVatPackage(f.reviewer,packageInput(),key())).rejects.toMatchObject({code:"VAT_GL_RECONCILIATION_REQUIRED"});
    await expect(beginClose(f.reviewer,f.period,key())).rejects.toMatchObject({code:"CLOSE_NOT_READY"});
    await approvedEntry(id);const p=await prepareVatPackage(f.reviewer,packageInput(),key());
    await expect(approveVatPackage({...f.reviewer,role:"finance_approver"},p.id,{expectedVersion:1,reason:"Self approval is prohibited"},key())).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
    await approvedEntry(await invoice("receivable","200","14"));
    await expect(approveVatPackage(f.manager,p.id,{expectedVersion:1,reason:"The new register entry must stale this package"},key())).rejects.toMatchObject({code:"VAT_PACKAGE_STALE"});
    await supersedeVatPackage(f.reviewer,p.id,{expectedVersion:1,reason:"Replace the stale preparation while preserving its source snapshot"},key());
    const replacement=await prepareVatPackage(f.reviewer,packageInput(),key());expect(replacement.result).toMatchObject({outputVatMinor:2100});
    expect((await vatPackageDetail(f.manager,p.id)).snapshot.result.outputVatMinor).toBe(700);
    await expect(db.prepare("UPDATE finance_statutory_packages SET snapshot_json='{}' WHERE id=?").bind(p.id).run()).rejects.toThrow("STATUTORY_SNAPSHOT_IMMUTABLE");
  });
  it("exports replayable Thai workpapers, records external filing and links additional returns",async()=>{
    await approvedEntry(await invoice(),{documentReference:'=HYPERLINK("https://example.invalid")'});const p=await prepareVatPackage(f.reviewer,packageInput(),key());
    await approveVatPackage(f.manager,p.id,{expectedVersion:1,reason:"Reviewed all source amounts, tax points and VAT to GL reconciliation"},key());
    const command=key(),input={expectedVersion:2,reason:"Export the exact reviewed Thai preparation workpaper"},[first,retry]=await Promise.all([exportVatWorkpaper(f.reviewer,p.id,input,command),exportVatWorkpaper(f.reviewer,p.id,input,command)]);
    expect(first).toEqual(retry);expect(first.content).toContain("ภ.พ.30");expect(first.content).toContain("' =".replace(" ",""));expect(first.content).toContain("7.00");expect(first.content).toContain("\r\n");
    expect(await vatPackageExport(f.manager,p.id)).toMatchObject({content:first.content,exportHash:first.exportHash});
    const filed=await recordVatFiling(f.reviewer,p.id,{expectedVersion:3,reason:"Recorded fictional receipt for isolated testing only",filingReference:"RD-FIXTURE-ONLY",filedDate:"2026-10-15",receiptEvidence:"Fictional local test receipt; no official submission performed"},key());expect(filed.status).toBe("filed");
    await expect(supersedeVatPackage(f.reviewer,p.id,{expectedVersion:4,reason:"Filed package cannot be overwritten"},key())).rejects.toMatchObject({code:"VERSION_CONFLICT"});
    await approvedEntry(await invoice("receivable","10","0.70"));
    const additional=await prepareVatPackage(f.reviewer,{...packageInput(),filingKind:"additional",originalPackageId:p.id},key());
    const detail=await vatPackageDetail(f.manager,additional.id);expect(detail.package.original_package_id).toBe(p.id);expect(detail.snapshot.priorFiledResult.outputVatMinor).toBe(700);expect(detail.snapshot.result.outputVatMinor).toBe(770);
    expect((await vatPackageDetail(f.manager,p.id)).events.map(e=>e.action)).toEqual(["prepare","approve","export","file"]);
  });
  it("prepares a quiet month and keeps a net VAT credit as a credit",async()=>{
    const quiet=await prepareVatPackage(f.reviewer,{...packageInput(),filingMonth:"2026-08"},key());expect(quiet.result).toMatchObject({netMinor:0});
    await approvedEntry(await invoice("payable"),{treatment:"allocated",deductibleBps:5000});
    const p=await prepareVatPackage(f.reviewer,packageInput(),key());expect(p.result).toMatchObject({inputVatMinor:700,deductibleInputMinor:350,payableMinor:0,creditMinor:350,netMinor:-350});
  });
  it("retains VAT close evidence, refuses late GL and rejects pending corrections or unmatched tax movement",async()=>{
    const source=await invoice(),entry=await approvedEntry(source);
    const prepared=await beginClose(f.reviewer,f.period,key());
    const evidence=JSON.parse(String(await db.prepare("SELECT detail_json FROM finance_close_checks WHERE period_id=? AND check_key='statutory-vat'").bind(f.period).first("detail_json")));
    expect(evidence).toMatchObject({ready:true,rows:[expect.objectContaining({ledger_minor:700,register_minor:700})]});
    const journal=`late-vat-${key()}`;
    const lateSteps=[
      db.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,memo,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'2026-09-30','THB','manual','Unmatched tax evidence fixture','draft',?,1,1)").bind(journal,f.company,f.chart,f.period,journal,f.reviewer.userId),
      db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,100,0,100,0,0),(?,?,?,0,100,0,100,1)").bind(key(),journal,f.accounts.cash,key(),journal,outputAccount),
      db.prepare("UPDATE finance_journals SET status='posted',posted_by=?,posted_at=1 WHERE id=?").bind(f.manager.userId,journal),
    ];
    await expect(db.batch(lateSteps)).rejects.toThrow("PERIOD_CLOSED");
    expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE id=?").bind(journal).first("n"))).toBe(0);
    const correction=await prepareVatCorrection(f.reviewer,entry,vatCorrectionSchema.parse({...facts(source,{documentReference:"Corrected supporting tax invoice reference"}),expectedEntryVersion:2,reason:"Review newly received document evidence without changing retained tax amounts"}),key());
    const period=await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first<FinanceRow>();
    expect(await calculateCloseReconciliations(db,f.manager,period!)).toMatchObject({ready:false,vat:{ready:false,pendingCorrections:1,rows:[expect.objectContaining({ledger_minor:700,register_minor:700})]}});
    await expect(closePeriod(f.manager,f.period,{expectedVersion:Number(prepared.version),proof:"Reject unmatched VAT source evidence"},key())).rejects.toMatchObject({code:"CLOSE_RECONCILIATION_STALE"});
    await cancelClose(f.reviewer,f.period,{expectedVersion:Number(prepared.version),reason:"Resolve changed source evidence before preparing the close again"},key());
    await reviewVatCorrection(f.manager,correction.id,"reject",{expectedVersion:1,reason:"Retain independently reviewed original tax reference"},key());
    await db.batch(lateSteps);
    const openPeriod=await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first<FinanceRow>();
    expect(await calculateCloseReconciliations(db,f.manager,openPeriod!)).toMatchObject({ready:false,vat:{ready:false,rows:[expect.objectContaining({ledger_minor:800,register_minor:700})]}});
    await expect(beginClose(f.reviewer,f.period,key())).rejects.toMatchObject({code:"CLOSE_VAT_RECONCILIATION_FAILED"});
    expect(JSON.parse(String(await db.prepare("SELECT detail_json FROM finance_close_checks WHERE period_id=? AND check_key='statutory-vat'").bind(f.period).first("detail_json"))).rows[0].ledger_minor).toBe(700);
  });
  it("appends independently reviewed corrections while preserving filed figures and original export bytes",async()=>{
    const source=await invoice("payable"),originalId=await approvedEntry(source),original=await db.prepare("SELECT * FROM finance_tax_entries WHERE id=?").bind(originalId).first();
    const p=await prepareVatPackage(f.reviewer,packageInput(),key());await approveVatPackage(f.manager,p.id,{expectedVersion:1,reason:"Independent original monthly input-tax review"},key());
    const exported=await exportVatWorkpaper(f.reviewer,p.id,{expectedVersion:2,reason:"Retain the approved original input-tax workpaper"},key());
    await recordVatFiling(f.reviewer,p.id,{expectedVersion:3,reason:"Isolated fixture recording only; no external submission",filingReference:"CORRECTION-ORIGINAL-FIXTURE",filedDate:"2026-10-15",receiptEvidence:"Fictional original receipt preserved for correction testing"},key());
    const input=vatCorrectionSchema.parse({...facts(source),expectedEntryVersion:2,treatment:"allocated",deductibleBps:5000,reason:"Reviewed evidence requires only fifty percent input-tax deduction"}),command=key();
    const [first,retry]=await Promise.all([prepareVatCorrection(f.reviewer,originalId,input,command),prepareVatCorrection(f.reviewer,originalId,input,command)]);expect(first).toEqual(retry);
    const replacement=await db.prepare("SELECT * FROM finance_tax_entries WHERE correction_id=?").bind(first.id).first<Record<string,string|number|null>>();
    expect((await vatWorkspace(f.manager,{filingMonth:"2026-09"})).reconciliation).toMatchObject({ready:false,pendingCorrections:1});
    await expect(beginClose(f.reviewer,f.period,key())).rejects.toMatchObject({code:"CLOSE_NOT_READY"});
    await expect(reviewVatEntry(f.manager,String(replacement!.id),"approve",{expectedVersion:1,reason:"Single replacement cannot bypass joint review"},key())).rejects.toMatchObject({code:"VAT_CORRECTION_WORKFLOW_REQUIRED"});
    await expect(db.prepare("UPDATE finance_tax_entries SET status='approved',version=2,reviewed_by=? WHERE id=?").bind(f.manager.userId,replacement!.id).run()).rejects.toThrow("VAT_CORRECTION_NOT_ATOMIC");
    await expect(reviewVatCorrection({...f.reviewer,role:"finance_approver"},first.id,"approve",{expectedVersion:1,reason:"Self review of tax corrections is forbidden"},key())).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
    await expect(reviewVatCorrection({...f.manager,companyId:"other-company"},first.id,"approve",{expectedVersion:1,reason:"Cross Company correction review is forbidden"},key())).rejects.toMatchObject({code:"VAT_CORRECTION_NOT_FOUND"});
    const reviewKey=key(),decision={expectedVersion:1,reason:"Independent review of corrected deduction and retained source amount"};
    expect(await Promise.all([reviewVatCorrection(f.manager,first.id,"approve",decision,reviewKey),reviewVatCorrection(f.manager,first.id,"approve",decision,reviewKey)])).toEqual([expect.objectContaining({status:"approved"}),expect.objectContaining({status:"approved"})]);
    expect(await db.prepare("SELECT * FROM finance_tax_entries WHERE id=?").bind(originalId).first()).toEqual(original);
    expect(await vatPackageExport(f.manager,p.id)).toMatchObject({content:exported.content,exportHash:exported.exportHash});
    const additional=await prepareVatPackage(f.reviewer,{...packageInput(),filingKind:"additional",originalPackageId:p.id},key());
    expect(additional.result).toMatchObject({inputVatMinor:700,deductibleInputMinor:350});
    expect((await vatPackageDetail(f.manager,p.id)).snapshot.result.deductibleInputMinor).toBe(700);
    expect((await vatPackageDetail(f.manager,additional.id)).snapshot.priorFiledResult.deductibleInputMinor).toBe(700);
    await expect(prepareVatCorrection(f.reviewer,originalId,input,key())).rejects.toMatchObject({code:"CURRENT_APPROVED_TAX_ENTRY_REQUIRED"});
  });
  it("rejects stale corrections, retains rejection evidence and invalidates unfiled preparation when a claim moves month",async()=>{
    const source=await invoice("payable"),entry=await approvedEntry(source),p=await prepareVatPackage(f.reviewer,packageInput(),key());
    const input=vatCorrectionSchema.parse({...facts(source),expectedEntryVersion:2,claimMonth:"2026-10",reason:"Original input-tax evidence supports a later claim month"});
    const stale=await prepareVatCorrection(f.reviewer,entry,input,key());
    await db.prepare("UPDATE finance_invoices SET version=version+1 WHERE id=?").bind(source).run();
    await expect(reviewVatCorrection(f.manager,stale.id,"approve",{expectedVersion:1,reason:"The source version changed after correction preparation"},key())).rejects.toMatchObject({code:"TAX_SOURCE_CHANGED"});
    await reviewVatCorrection(f.manager,stale.id,"reject",{expectedVersion:1,reason:"Retain this stale correction and reprepare current source evidence"},key());
    const fresh=await prepareVatCorrection(f.reviewer,entry,{...input,expectedSourceVersion:3},key());
    await reviewVatCorrection(f.manager,fresh.id,"approve",{expectedVersion:1,reason:"Reviewed October claim month without changing original financial amounts"},key());
    await expect(approveVatPackage(f.manager,p.id,{expectedVersion:1,reason:"The September register changed after preparation"},key())).rejects.toMatchObject({code:"VAT_PACKAGE_STALE"});
    await supersedeVatPackage(f.reviewer,p.id,{expectedVersion:1,reason:"Preserve the old September preparation and build a replacement"},key());
    expect((await prepareVatPackage(f.reviewer,packageInput(),key())).result).toMatchObject({inputVatMinor:0,deductibleInputMinor:0});
    expect((await prepareVatPackage(f.reviewer,{...packageInput(),filingMonth:"2026-10"},key())).result).toMatchObject({inputVatMinor:700,deductibleInputMinor:700});
    expect((await vatPackageDetail(f.manager,p.id)).snapshot.result.inputVatMinor).toBe(700);
  });
  it("invalidates prepared close evidence when only reviewed VAT facts change and permits an independently refreshed close",async()=>{
    const source=await invoice("payable"),entry=await approvedEntry(source),prepared=await beginClose(f.reviewer,f.period,key());
    const original=String(await db.prepare("SELECT detail_json FROM finance_close_checks WHERE period_id=? AND check_key='statutory-vat' ").bind(f.period).first("detail_json"));
    const group=await prepareVatCorrection(f.reviewer,entry,vatCorrectionSchema.parse({...facts(source),expectedEntryVersion:2,documentReference:"Corrected tax-invoice reference only",reason:"Correct a reference without changing GL or input deduction"}),key());
    await reviewVatCorrection(f.manager,group.id,"approve",{expectedVersion:1,reason:"Checked replacement evidence; financial amounts remain unchanged"},key());
    const workspace=await vatWorkspace(f.manager,{filingMonth:"2026-09"});expect(workspace.reconciliation).toMatchObject({ready:true,rows:[expect.objectContaining({ledger_minor:700,register_minor:700})]});
    await expect(closePeriod(f.manager,f.period,{expectedVersion:Number(prepared.version),proof:"Amounts match but the VAT evidence was replaced"},key())).rejects.toMatchObject({code:"CLOSE_RECONCILIATION_STALE"});
    expect(String(await db.prepare("SELECT detail_json FROM finance_close_checks WHERE period_id=? AND check_key='statutory-vat' ").bind(f.period).first("detail_json"))).toBe(original);
    await cancelClose(f.reviewer,f.period,{expectedVersion:Number(prepared.version),reason:"Retain old VAT support and prepare the corrected register"},key());
    const refreshed=await beginClose(f.reviewer,f.period,key());
    await closePeriod(f.manager,f.period,{expectedVersion:Number(refreshed.version),proof:"Independently reviewed the replacement tax evidence and unchanged GL"},key());
    expect(await db.prepare("SELECT state FROM finance_periods WHERE id=?").bind(f.period).first("state")).toBe("closed");
  });
  it("freezes correction drafts, denies revoked reviewers, scopes history and reports package freshness",async()=>{
    const source=await invoice("payable"),entry=await approvedEntry(source),p=await prepareVatPackage(f.reviewer,packageInput(),key());
    expect(await vatPackageDetail(f.manager,p.id)).toMatchObject({currentRegisterMatches:true,pendingCorrections:0});
    const input=vatCorrectionSchema.parse({...facts(source),expectedEntryVersion:2,documentReference:"Updated original document evidence",reason:"Preserve the old reference with a reviewed replacement"});
    const group=await prepareVatCorrection(f.reviewer,entry,input,key()),detail=await vatCorrectionDetail(f.manager,group.id),replacement=String(detail.entries[0].replacement.id);
    expect(await vatPackageDetail(f.manager,p.id)).toMatchObject({currentRegisterMatches:true,pendingCorrections:1});
    await expect(prepareVatCorrection(f.reviewer,entry,input,key())).rejects.toMatchObject({code:"VAT_CORRECTION_PENDING"});
    await expect(db.prepare("UPDATE finance_tax_entries SET document_reference='overwritten' WHERE id=?").bind(replacement).run()).rejects.toThrow("TAX_ENTRY_EVIDENCE_IMMUTABLE");
    await expect(taxEntryDetail({...f.manager,companyId:"other"},entry)).rejects.toMatchObject({code:"TAX_ENTRY_NOT_FOUND"});
    await expect(vatCorrectionDetail({...f.manager,companyId:"other"},group.id)).rejects.toMatchObject({code:"VAT_CORRECTION_NOT_FOUND"});
    await db.prepare("UPDATE assignments SET valid_until=1 WHERE id=?").bind(f.manager.assignmentId).run();
    await expect(reviewVatCorrection(f.manager,group.id,"approve",{expectedVersion:1,reason:"This assignment was revoked after the draft was prepared"},key())).rejects.toMatchObject({code:"CONFLICT"});
    expect((await vatCorrectionDetail(f.reviewer,group.id)).correction.status).toBe("prepared");
    f.manager.assignmentId=await f.grant(f.manager,"finance_approver");
    await reviewVatCorrection(f.manager,group.id,"approve",{expectedVersion:1,reason:"A currently authorized independent reviewer checked the correction"},key());
    expect(await taxEntryDetail(f.manager,entry)).toMatchObject({currentId:replacement,history:[expect.objectContaining({id:entry}),expect.objectContaining({id:replacement})]});
    expect(await vatPackageDetail(f.manager,p.id)).toMatchObject({currentRegisterMatches:false,pendingCorrections:0});
  });

  it("rolls back a reviewed correction if final outbox fails and retries with one immutable history revision",async()=>{
    const source=await invoice("payable"),entry=await approvedEntry(source),original=await db.prepare("SELECT * FROM finance_tax_entries WHERE id=?").bind(entry).first(),group=await prepareVatCorrection(f.reviewer,entry,vatCorrectionSchema.parse({...facts(source),expectedEntryVersion:2,treatment:"allocated",deductibleBps:5000,reason:"Reviewed proportionate deduction for original tax evidence"}),key());
    const retained=await vatCorrectionDetail(f.manager,group.id),replacement=String(retained.entries[0].replacement.id),command=key(),input={expectedVersion:1,reason:"Independently reviewed original evidence and proportionate deduction"},fault=`vat_review_fault_${crypto.randomUUID().replaceAll("-","")}`;
    await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'VAT_REVIEW_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.vat-correction.approve') EXECUTE FUNCTION ${fault}();`);
    try{
      await expect(reviewVatCorrection(f.manager,group.id,"approve",input,command)).rejects.toThrow();
      expect(await vatCorrectionDetail(f.manager,group.id)).toEqual(retained);
      expect(await db.prepare("SELECT * FROM finance_tax_entries WHERE id=?").bind(entry).first()).toEqual(original);
      expect(await db.prepare("SELECT id FROM finance_current_tax_entries WHERE company_id=? AND source_type='invoice' AND source_id=? AND status='approved'").bind(f.company,source).first("id")).toBe(entry);
      for(const table of ["command_receipts","decision_evidence","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(0);
    }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
    const result=await reviewVatCorrection(f.manager,group.id,"approve",input,command);expect(await reviewVatCorrection(f.manager,group.id,"approve",input,command)).toEqual(result);
    expect(result).toMatchObject({id:group.id,status:"approved",version:2,entryCount:1});
    expect(await taxEntryDetail(f.manager,entry)).toMatchObject({currentId:replacement,history:[expect.objectContaining({id:entry}),expect.objectContaining({id:replacement})]});
    expect(await db.prepare("SELECT * FROM finance_tax_entries WHERE id=?").bind(entry).first()).toEqual(original);
    for(const table of ["command_receipts","decision_evidence","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(1);
  });
});

afterEach(()=>vi.unstubAllEnvs());

it("purchased VAT rejects an unpurchased register and command while shadow retains source policy",async()=>{
 const id=await invoice();vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 await expect(vatWorkspace(f.reviewer,{})).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(prepareVatEntry(f.reviewer,facts(id),key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","shadow");const telemetry=vi.spyOn(console,"info").mockImplementation(()=>{});try{expect(await prepareVatEntry(f.reviewer,facts(id),key())).toMatchObject({status:"prepared",taxMinor:700});}finally{telemetry.mockRestore();}
});
it("Finance-only draining reviews existing tax, exports exact VAT bytes and records the retained filing with current authority",async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const id=await invoice();await reviewedFinanceGrant(db,f.company,"finance","draining");
 const entry=await prepareVatEntry(f.reviewer,facts(id),key());await reviewVatEntry(f.manager,entry.id,"approve",{expectedVersion:1,reason:"Independent retained tax point review"},key());
 const pack=await prepareVatPackage(f.reviewer,packageInput(),key());await approveVatPackage(f.manager,pack.id,{expectedVersion:1,reason:"Independent exact VAT and GL reconciliation"},key());
 const request={expectedVersion:2,reason:"Export original reviewed workpaper"},command=key(),exported=await exportVatWorkpaper(f.reviewer,pack.id,request,command);expect(await exportVatWorkpaper(f.reviewer,pack.id,request,command)).toEqual(exported);
 await recordVatFiling(f.reviewer,pack.id,{expectedVersion:3,reason:"Retain isolated filing proof without external submission",filingReference:"ISOLATED-NO-SUBMISSION",filedDate:"2026-10-15",receiptEvidence:"Synthetic retained receipt in a disposable database only"},key());
 expect(await vatPackageExport(f.manager,pack.id)).toMatchObject({content:exported.content,exportHash:exported.exportHash});
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.manager.assignmentId).run();await expect(vatPackageExport(f.manager,pack.id)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});
 expect(await db.prepare("SELECT count(*) n FROM command_assertions").first("n")).toBe(0);
});
