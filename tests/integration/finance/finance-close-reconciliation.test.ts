import { beforeAll, afterEach, describe, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {calculateCloseReconciliations} from '@erpx/server-finance/public/finance';

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const db=env.H001_DB;
afterEach(()=>vi.unstubAllEnvs());
const preparer={userId:"close-preparer",assignmentId:"close-preparer-a",role:"finance_preparer",companyId:"close-company",holdingId:"h001",branchId:null,departmentId:null} as WorkContext;
const approver={...preparer,userId:"close-approver",assignmentId:"close-approver-a",role:"finance_approver"} as WorkContext;

beforeAll(async()=>{
  await applyMigrations(db,[...env.STAFF_TEST_MIGRATIONS,{name:"0254_finance_close_reconciliations",track:"holding-write"}]);
  await db.exec(`
    INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('close-company','company','close-company','Close company','close-company');
    INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES
      ('close-preparer-a','close-preparer','finance_preparer','["finance"]','close-company',0,9999999999999,'holding'),
      ('close-approver-a','close-approver','finance_approver','["finance"]','close-company',0,9999999999999,'holding');
    INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES('close-approver-p','close-approver','close-company','Close Approver','TH','active','close-preparer',1,1);
    INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('close-approver-e','close-approver-p','close-company','employee','salaried','active','2026-01-01','Controller','THB','close-preparer',1,1);
    INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('close-chart','close-company','Close chart','THB','IFRS','active','close-preparer',1,1);
    INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES
      ('close-bank','close-chart','close-company','1100','Operating bank','asset','THB',1,'bank',1,1),
      ('close-ar','close-chart','close-company','1200','Trade receivables','asset','THB',1,'receivable',1,1),
      ('close-ap','close-chart','close-company','2100','Trade payables','liability','THB',1,'payable',1,1),
      ('close-equity','close-chart','close-company','3000','Opening equity','equity','THB',0,'none',1,1);
    INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES
      ('close-jan','close-company','January 2026','2026-01-01','2026-01-31',1),
      ('close-feb','close-company','February 2026','2026-02-01','2026-02-28',1),
      ('close-mar','close-company','March 2026','2026-03-01','2026-03-31',1);
    INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES
      ('close-customer','close-company','customer','C001','Customer','THB','active',1,1),
      ('close-supplier','close-company','supplier','S001','Supplier','THB','active',1,1);
    INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,memo,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES('close-opening','close-company','close-chart','close-jan','OPEN-1','2026-01-01','THB',1000000,'opening_import','Opening','posted','close-preparer','close-approver',2,1,2);
    INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES
      ('close-l1','close-opening','close-bank',100000,0,100000,0,0),('close-l2','close-opening','close-ar',30000,0,30000,0,1),
      ('close-l3','close-opening','close-ap',0,20000,0,20000,2),('close-l4','close-opening','close-equity',0,110000,0,110000,3);
    INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,posted_at,created_at,updated_at,journal_id,source_type,source_id) VALUES
      ('close-ar-invoice','close-company','close-customer','receivable','OPEN-AR','2026-01-01','2026-01-31','THB',30000,30000,30000,'close-ar','close-equity','posted','close-preparer',2,1,2,'close-opening','opening_import','close-opening'),
      ('close-ap-invoice','close-company','close-supplier','payable','OPEN-AP','2026-01-01','2026-01-31','THB',20000,20000,20000,'close-equity','close-ap','posted','close-preparer',2,1,2,'close-opening','opening_import','close-opening');
    INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,status,source_hash,imported_by,created_at) VALUES
      ('close-bank-jan','close-company','close-bank','JAN-2026','2026-01-31',100000,100000,'reconciled','close-bank-jan-hash','close-preparer',1),
      ('close-bank-feb','close-company','close-bank','FEB-2026','2026-02-28',100000,100000,'reconciled','close-bank-feb-hash','close-preparer',2),
      ('close-bank-mar','close-company','close-bank','MAR-2026','2026-03-31',100000,100000,'reconciled','close-bank-mar-hash','close-preparer',3);
  `);
});

async function seedClose(periodId:string,version:number,closed:boolean){
  const period=await db.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId,preparer.companyId).first<Record<string,string|number|null>>();
  if(!period)throw Error("CLOSE_PERIOD_FIXTURE_MISSING");
  const reconciliation=await calculateCloseReconciliations(db,preparer,period);
  expect(reconciliation.ready).toBe(true);
  for(const row of reconciliation.rows)await db.prepare("INSERT INTO finance_close_reconciliations(id,company_id,period_id,close_version,kind,account_id,currency,ledger_minor,source_minor,difference_minor,source_json,checked_at,checked_by) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),preparer.companyId,periodId,version,row.kind,row.accountId,row.currency,row.ledgerMinor,row.sourceMinor,row.differenceMinor,JSON.stringify(row.source),Date.now(),preparer.userId).run();
  const next=await db.prepare("UPDATE finance_periods SET state=?,close_started_by=?,closed_by=?,closed_at=?,close_version=?,reconciliation_hash=? WHERE id=? AND company_id=?")
    .bind(closed?"closed":"closing",preparer.userId,closed?approver.userId:null,closed?Date.now():null,closed?version+1:version,reconciliation.reconciliationHash,periodId,preparer.companyId).run();
  expect(next.meta.changes).toBe(1);
  return reconciliation;
}
describe("finance close reconciliation",()=>{
  it("proves AR, AP, and bank to GL and completes two consecutive independent closes",async()=>{
    const janPeriod=await db.prepare("SELECT * FROM finance_periods WHERE id='close-jan'").first<Record<string,string|number|null>>();
    const preview=await calculateCloseReconciliations(db,preparer,janPeriod!);
    expect(preview.ready).toBe(true);
    expect(preview.rows.map(row=>[row.kind,row.differenceMinor])).toEqual([["bank",0],["receivable",0],["payable",0]]);
    // Native period-close owns the transition; seed the persisted close evidence
    // and reload it through the retained reconciliation read.
    await seedClose("close-jan",1,true);
    await seedClose("close-feb",1,true);
    expect(await db.prepare("SELECT count(*) n FROM finance_periods WHERE company_id='close-company' AND state='closed'").first("n")).toBe(2);
    expect(await db.prepare("SELECT count(*) n FROM finance_close_reconciliations WHERE company_id='close-company' AND difference_minor=0").first("n")).toBe(6);
  });

  it("rejects changed evidence after close preparation",async()=>{
    const prepared=await seedClose("close-mar",1,false);
    const lateJournal="INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,memo,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES('close-late','close-company','close-chart','close-mar','LATE-1','2026-03-31','THB',1000000,'manual','Late change','posted','close-preparer','close-approver',4,4,4)";
    await expect(db.exec(lateJournal)).rejects.toThrow("PERIOD_CLOSED");
    expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE id='close-late'").first("n"))).toBe(0);
    // A bank evidence reference can change without admitting any closing-date GL.
    await db.exec("UPDATE finance_bank_statements SET statement_number='MAR-2026-CORRECTED' WHERE id='close-bank-mar'");
    const march=await db.prepare("SELECT * FROM finance_periods WHERE id='close-mar'").first<Record<string,string|number|null>>();
    expect((await calculateCloseReconciliations(db,preparer,march!)).reconciliationHash).not.toBe(prepared.reconciliationHash);
    // Native cancel_period_close owns this transition; seed its persisted state so the
    // retained reconciliation oracle can resubmit corrected evidence without the retired TS command.
    await db.prepare("UPDATE finance_periods SET state='open',close_started_by=NULL,reconciliation_hash=NULL,close_version=close_version+1 WHERE id='close-mar' AND company_id='close-company' AND close_version=1").run();
    const cancelled=await db.prepare("SELECT state,close_version FROM finance_periods WHERE id='close-mar'").first<{state:string;close_version:number}>();
    expect(cancelled).toMatchObject({state:"open",close_version:2});
    await db.exec(lateJournal+"; INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES('close-late-1','close-late','close-bank',100,0,100,0,0),('close-late-2','close-late','close-equity',0,100,0,100,1)");
    await db.exec("UPDATE finance_bank_statements SET closing_minor=100100,base_closing_minor=100100 WHERE id='close-bank-mar'");
    const changed=await db.prepare("SELECT * FROM finance_periods WHERE id='close-mar'").first<Record<string,string|number|null>>();
    expect((await calculateCloseReconciliations(db,preparer,changed!)).reconciliationHash).not.toBe(prepared.reconciliationHash);
    expect(await db.prepare("SELECT state,close_version FROM finance_periods WHERE id='close-mar'").first()).toMatchObject({state:"open",close_version:2});
    const refreshed=await seedClose("close-mar",3,true);
    expect(refreshed.reconciliationHash).not.toBe(prepared.reconciliationHash);
    expect(await db.prepare("SELECT state,close_version FROM finance_periods WHERE id='close-mar'").first()).toMatchObject({state:"closed",close_version:4});
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_close_reconciliations WHERE period_id='close-mar'").first("n")).toBe(6);
  });
});
