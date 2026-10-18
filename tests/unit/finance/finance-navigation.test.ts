import { describe, expect, it } from "vitest";
import { financeAreaItems, financeContextHref, financeNavigation } from "../../../src/lib/finance/finance-navigation";
import { resolveNavigation, workplaceNavigation, type NavigationContext } from "../../../src/lib/work_projection/workplace-navigation";

const base = "/holdings/acme";
const context: NavigationContext = { workspaceBase: base, role: "holding_owner", scopeKind: "holding", financeWorkspaceV2: true };

describe("Finance area navigation", () => {
  it("keeps discovery and existing detail URLs while highlighting one of six areas", () => {
    const model = workplaceNavigation(context);
    expect(model.categories.find(category => category.id === "finance")?.apps.map(item => item.label)).toEqual(["Today", "Documents", "Cash", "Accounting", "Reports", "Setup"]);
    for (const [route, destination, sidebar] of [
      ["finance/sales", "finance-sales", "finance-sales"],
      ["finance/invoices/invoice-1?kind=payable", "finance-payables", "finance-sales"],
      ["finance/payments/payment-1", "finance-payments", "finance-payments"],
      ["finance/supplier-batches/batch-1", "finance-supplier-batches", "finance-payments"],
      ["finance/refunds/refund-1", "finance-refunds", "finance-payments"],
      ["finance/journals/journal-1", "finance-ledger", "finance-ledger"],
      ["finance/assets/asset-1", "finance-ledger", "finance-ledger"],
      ["finance/tax/filing-1", "finance-tax", "finance-ledger"],
      ["finance/engines", "finance-engines", "finance-ledger"],
      ["finance/group", "finance-group", "finance-reports"],
      ["expenses/expense-1", "finance-expenses", "finance-sales"],
    ]) {
      const url = new URL(`${base}/${route}`, "https://test.invalid");
      expect(resolveNavigation(model, base, url.pathname, url.searchParams), route).toMatchObject({ activeId: destination, sidebarId: sidebar });
      expect(model.search.some(item => item.id === destination), destination).toBe(true);
    }
    const items = financeNavigation(context);
    for (const area of ["documents", "cash", "accounting", "reports"] as const) expect(financeAreaItems(items, area).length).toBeLessThanOrEqual(area === "documents" ? 4 : 3);
  });

  it("retains role, scope and release gates without exposing hidden tools in search", () => {
    expect(financeNavigation({ ...context, role: "employee" })).toEqual([]);
    expect(financeNavigation({ ...context, beta: true })).toEqual([]);
    expect(financeNavigation({ ...context, availableAppIds: ["people"] })).toEqual([]);
    const production = financeNavigation({ ...context, availableAppIds: ["finance"] });
    expect(production.some(item => ["finance-engines", "finance-group"].includes(item.id))).toBe(false);
    const branch = financeNavigation({ ...context, scopeKind: "branch" });
    expect(branch.some(item => item.id === "finance-group")).toBe(false);
    const reader = financeNavigation({ ...context, role: "employee", permissions: ["finance.read"] });
    expect(reader.some(item => item.id === "finance-hotels")).toBe(false);
    expect(reader.some(item => item.id === "finance-expenses")).toBe(false);
  });

  it("carries financial context without moving list cursors or changing the destination kind", () => {
    const search = new URLSearchParams({ chart: "chart-2", periodId: "period-2", startDate: "2026-09-01", asOf: "2026-09-30", cursor: "other-record", status: "draft" });
    const payable = new URL(financeContextHref(`${base}/finance/invoices?kind=payable&queue=open`, search), "https://test.invalid");
    expect(Object.fromEntries(payable.searchParams)).toEqual({ kind: "payable", queue: "open", chartId: "chart-2", asOf: "2026-09-30" });
    const reports = new URL(financeContextHref(`${base}/finance/reports`, search), "https://test.invalid");
    expect(Object.fromEntries(reports.searchParams)).toEqual({ chart: "chart-2", periodId: "period-2", startDate: "2026-09-01" });
    expect(financeContextHref(`${base}/expenses`, search)).toBe(`${base}/expenses`);
  });
});

import {afterEach,vi} from "vitest";
import {createElement,type ComponentType} from "react";
import {renderToStaticMarkup} from "react-dom/server";
import {financeDocumentCommandAllowed,financeDocumentCommandContext,financeDocumentFormUnchanged} from "../../../src/lib/finance/document-post-control";
import {FinancePurchaseProvider} from "../../../src/components/finance/purchase-context";
import {InvoiceActions} from "../../../src/components/finance/invoice-controls";
import {JournalAction} from "../../../src/components/finance/ledger-controls";
import type {FinancePurchaseControls} from "../../../src/server/modules/finance/public/rules/purchase-controls";
vi.mock("next/navigation",()=>({forbidden:()=>{throw Error("FORBIDDEN");},useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn()}),usePathname:()=>"/holdings/acme/finance/invoices/invoice",useSearchParams:()=>new URLSearchParams()}));
afterEach(()=>vi.unstubAllGlobals());
const sourceOnly:FinancePurchaseControls={"finance.accounting":{newWork:false,settleExisting:false,history:false,revision:"opaque"}};
function renderFinancePost<Props extends object>(Component:ComponentType<Props>,props:Props,purchases:FinancePurchaseControls|undefined|null=sourceOnly){const provider={purchases:purchases ?? undefined,sourceHistory:true,children:createElement(Component,props)};return renderToStaticMarkup(createElement(FinancePurchaseProvider,provider));}
it("limits a source posting inclusion to the exact document post path",()=>{
 const post={path:"invoices/invoice/post",canPost:true};expect(financeDocumentCommandAllowed(false,post.path,post)).toBe(true);
 for(const path of ["invoices/other/post","invoices/invoice/void","invoices","journals/journal/reverse","payments"])expect(financeDocumentCommandAllowed(false,path,post)).toBe(false);
 expect(financeDocumentCommandAllowed(true,post.path,{...post,canPost:false})).toBe(false);expect(financeDocumentCommandAllowed(true,post.path,{path:post.path})).toBe(true);
});
it("retains posting edits made after submission and rejects disconnected forms",()=>{
 let edit="submitted",connected=true;const form={get isConnected(){return connected;},getAttribute:()=>edit} as unknown as HTMLFormElement,completion={form,formEditVersion:edit};expect(financeDocumentFormUnchanged(completion)).toBe(true);edit="later-date";expect(financeDocumentFormUnchanged(completion)).toBe(false);edit="submitted";connected=false;expect(financeDocumentFormUnchanged(completion)).toBe(false);expect(financeDocumentFormUnchanged(undefined)).toBe(true);
});
it("separates posting retry identities when rendered Company or assignment changes",()=>{
 const dataset={workUser:"person",workRevision:"1",workCompany:"A",workAssignment:"reviewer-A"};vi.stubGlobal("document",{querySelector:()=>({dataset})});const first=financeDocumentCommandContext();dataset.workCompany="B";expect(financeDocumentCommandContext()).not.toBe(first);dataset.workCompany="A";dataset.workAssignment="reviewer-B";expect(financeDocumentCommandContext()).not.toBe(first);
});
it("shows source-only invoice posting through its drawer only with original reviewer authority",()=>{
 const props={base,invoice:{id:"invoice",source_type:"commerce_fulfillment",status:"draft",kind:"receivable",version:1,invoice_date:"2026-10-15"},periods:[],canPost:true,sourceCanPost:true};
 expect(renderFinancePost(InvoiceActions,props)).toContain("Review invoice posting");expect(renderFinancePost(InvoiceActions,{...props,sourceCanPost:false})).not.toContain("Review invoice posting");expect(renderFinancePost(InvoiceActions,{...props,canPost:false})).not.toContain("Review invoice posting");expect(renderFinancePost(InvoiceActions,{...props,sourceCanPost:undefined},null)).toContain("Review invoice posting");
 const protectedDraft=renderFinancePost(InvoiceActions,{...props,invoice:{...props.invoice,source_type:"standalone"},canVoid:false});expect(protectedDraft).not.toContain("Void draft");
});
it("permits source-only journal posting without enabling generic reversal or creation",()=>{
 const props={base,journal:{id:"journal",status:"draft",source_type:"stock_receipt",version:1},canPost:true,sourceCanPost:true,periods:[]};const post=renderFinancePost(JournalAction,props);expect(post).toContain("Approve and post journal");expect(post).not.toMatch(/<button[^>]* disabled=""[^>]*>Approve and post journal/);expect(renderFinancePost(JournalAction,{...props,sourceCanPost:false})).not.toContain("Approve and post journal");expect(renderFinancePost(JournalAction,{...props,canPost:false})).not.toContain("Approve and post journal");
 const reverse=renderFinancePost(JournalAction,{...props,journal:{...props.journal,status:"posted",source_type:"manual"}});expect(reverse).toMatch(/<button[^>]* disabled=""[^>]*>Post reversing journal/);expect(post).not.toContain("Create journal");for(const secret of ["company_package_grants","assignment_permissions","sourceEventId"])expect(post).not.toContain(secret);
});

import {PaymentLifecycle,PrepareControlledPayment} from "../../../src/components/finance/payment-lifecycle";
import {ServicePaymentBankEvidence} from "../../../src/components/finance/service-payment-bank-evidence";
const unavailableBanking:FinancePurchaseControls={...sourceOnly,"finance.banking":{newWork:false,settleExisting:false,history:false,revision:'bank-unavailable'}};
import {controlledPaymentAllowed,serviceBankImportAcknowledged,controlledPaymentCreationAcknowledged,paymentReversalAcknowledged} from "../../../src/lib/finance/service-payment-controls";
const paymentFixture={id:'payment-service',payment_number:'Service receipt',payment_date:'2026-10-16',version:1,currency:'THB',amount_minor:150000,bank_account:'bank-actual'};
it('enables controlled Service receipt review through exact source flags while absent source and Banking both deny',()=>{
 const props={base,payment:paymentFixture,periods:[],invoices:[],bankTransactions:[],fxAccounts:[],action:'approve' as const},active=renderFinancePost(PaymentLifecycle,{...props,sourcePurchase:{settleExisting:true,history:true}},unavailableBanking),inactive=renderFinancePost(PaymentLifecycle,{...props,sourcePurchase:{settleExisting:false,history:true}},unavailableBanking);expect(controlledPaymentAllowed(false,{settleExisting:true,history:true})).toBe(true);expect(controlledPaymentAllowed(false,{settleExisting:false,history:true})).toBe(false);expect(active).toContain('data-client-managed-ready="false"');expect(inactive).toMatch(/<button[^>]* disabled=""[^>]*>Approve payment/);expect(renderFinancePost(PaymentLifecycle,props,unavailableBanking)).toMatch(/<button[^>]* disabled=""[^>]*>Approve payment/);
});
it('retains full Banking controlled payment authority without requiring a Service source flag',()=>{
 const purchases={...unavailableBanking,'finance.banking':{newWork:true,settleExisting:true,history:true,revision:'bank-current'}},props={base,payment:paymentFixture,periods:[],invoices:[],bankTransactions:[],fxAccounts:[],action:'release' as const,sourcePurchase:{settleExisting:false,history:false}};expect(controlledPaymentAllowed(true,props.sourcePurchase)).toBe(true);expect(renderFinancePost(PaymentLifecycle,props,purchases)).toContain('Release payment');
});
it('prepares only the actual Service invoice under its source settlement flag and keeps history readonly',()=>{
 const props={base,invoice:{id:'actual-service-invoice',party_id:'actual-customer',kind:'receivable',currency:'THB',invoice_date:'2026-10-16',outstanding_minor:150000},bankAccounts:[{id:'actual-bank',name:'Original reviewed bank',code:'1000'}]},active=renderFinancePost(PrepareControlledPayment,{...props,sourcePurchase:{settleExisting:true,history:true}},unavailableBanking),history=renderFinancePost(PrepareControlledPayment,{...props,sourcePurchase:{settleExisting:false,history:true}},unavailableBanking);expect(active).toContain('Prepare controlled receipt');expect(active).toContain('data-client-managed-ready="false"');expect(history).toContain('Prepare controlled receipt');expect(history).not.toContain('<form');expect(controlledPaymentAllowed(false,{settleExisting:false,history:true})).toBe(false);expect(active).not.toMatch(/company_package_grants|sourceImage|assignment_permissions/);
});
it('keeps exact Service bank evidence behind the standard guarded right drawer',()=>{
 const html=renderFinancePost(ServicePaymentBankEvidence,{base,payment:paymentFixture,sourcePurchase:{settleExisting:true}},unavailableBanking);expect(html).toContain('Import bank evidence for this receipt');expect(html).not.toContain('Source evidence hash');expect(html).not.toContain('Import bank statement collection');
});

it('acknowledges bank evidence only for its captured source payment and exact statement key',()=>{expect(serviceBankImportAcknowledged({id:'statement-key',settlementPaymentId:'payment',imported:1},'key','payment')).toBe(true);for(const data of [{id:'statement-other',settlementPaymentId:'payment',imported:1},{id:'statement-key',settlementPaymentId:'foreign',imported:1},{id:'statement-key',settlementPaymentId:'payment',imported:2},{id:'statement-key'},null])expect(serviceBankImportAcknowledged(data,'key','payment')).toBe(false);expect(controlledPaymentAllowed(false)).toBe(false);});

import {PaymentCorrection} from "../../../src/components/finance/payment-correction";
it("keeps Supplier correction and Service reversal editors in guarded disclosures without exporting owner snapshots",()=>{
 const correction=renderFinancePost(PrepareControlledPayment,{base,invoice:{id:"actual-supplier-invoice",party_id:"supplier",kind:"payable",currency:"THB",outstanding_minor:25000000},bankAccounts:[{id:"retained-bank",code:"1000",name:"Original reviewed account"}],correctionPaymentId:"original-reversed-payment",correctionAmountMinor:25000000,sourcePurchase:{settleExisting:true,history:true}},unavailableBanking);
 expect(correction).toContain("Prepare controlled payment");expect(correction).toContain('data-client-managed-ready="false"');expect(correction).not.toContain('<form');
 const reversal=renderFinancePost(PaymentCorrection,{base,payment:paymentFixture,periods:[],sourcePurchase:{settleExisting:true,history:true}},unavailableBanking);expect(reversal).toContain("Review a payment correction");expect(reversal).toContain('data-client-managed-ready="false"');expect(reversal).not.toContain('<form');
 for(const html of [correction,reversal])expect(html).not.toMatch(/company_package_grants|sourceImage|assignment_permissions|original-reversed-payment/);
});
it("acknowledges only the captured invoice and correction anchor on a normal controlled draft",()=>{
 const receipt={id:"controlled-correction",status:"draft",version:1,settlementInvoiceId:"actual-invoice",correctionPaymentId:"original-payment"};
 expect(controlledPaymentCreationAcknowledged(receipt,"actual-invoice","original-payment")).toBe(true);
 for(const data of [{...receipt,settlementInvoiceId:"foreign-invoice"},{...receipt,correctionPaymentId:"foreign-payment"},{...receipt,status:"posted"},{...receipt,version:2},{...receipt,id:""},null])expect(controlledPaymentCreationAcknowledged(data,"actual-invoice","original-payment")).toBe(false);
 const service={id:"service-payment",status:"draft",version:1,settlementInvoiceId:"service-invoice"};expect(controlledPaymentCreationAcknowledged(service,"service-invoice")).toBe(true);expect(controlledPaymentCreationAcknowledged(receipt,"actual-invoice")).toBe(false);
});
it("does not acknowledge an unrelated or incomplete payment reversal",()=>{
 const receipt={id:"original-payment",status:"voided",reversalJournalId:"actual-reversing-journal"};expect(paymentReversalAcknowledged(receipt,"original-payment")).toBe(true);
 for(const data of [{...receipt,id:"foreign-payment"},{...receipt,status:"settled"},{...receipt,reversalJournalId:""},{id:"original-payment",status:"voided"},null])expect(paymentReversalAcknowledged(data,"original-payment")).toBe(false);
});

import {supplierCorrectionSource,supplierCorrectionInput,sendSupplierCorrection,type SupplierPaymentCorrectionSource} from "../../../src/lib/finance/supplier-payment-correction";
import {SupplierPaymentCorrectionPreparation} from "../../../src/components/finance/supplier-payment-correction";
const partialSourceA:SupplierPaymentCorrectionSource={paymentId:"original-150",paymentNumber:"Partial first",amountMinor:15000000,bankAccountId:"bank-A",bankAccountLabel:"Bank A",bankReference:"original-debit-150"},partialSourceB:SupplierPaymentCorrectionSource={paymentId:"original-100",paymentNumber:"Partial final",amountMinor:10000000,bankAccountId:"bank-B",bankAccountLabel:"Bank B",bankReference:"original-debit-100"};
it("requires an explicit unique original partial payment and retains its exact amount and bank identity",()=>{
 const sources=[partialSourceA,partialSourceB];expect(supplierCorrectionSource(sources,"")).toBeUndefined();expect(supplierCorrectionSource(sources,"foreign-payment")).toBeUndefined();expect(supplierCorrectionSource([...sources,{...partialSourceB}],partialSourceB.paymentId)).toBeUndefined();const selected=supplierCorrectionSource(sources,partialSourceB.paymentId)!;
 expect(supplierCorrectionInput(selected,{id:"actual-invoice",partyId:"actual-supplier",currency:"THB"},{paymentNumber:"chosen-correction",paymentDate:"2026-11-16"})).toEqual({partyId:"actual-supplier",kind:"payment",paymentNumber:"chosen-correction",paymentDate:"2026-11-16",currency:"THB",amount:"100000.00",bankAccount:"bank-B",settlementInvoiceId:"actual-invoice",correctionPaymentId:"original-100"});
});
it("retries the captured partial correction body and key after selection and draft values change",async()=>{
 const source={...partialSourceA},fields={paymentNumber:"captured-reference",paymentDate:"2026-11-16"},input=supplierCorrectionInput(source,{id:"actual-invoice",partyId:"supplier",currency:"THB"},fields),attempt={input,body:JSON.stringify(input),key:"captured-original-key"},fetcher=vi.fn(async(...args:[string,RequestInit])=>{void args;return Response.json({data:{id:"prepared-150",status:"draft",version:1,settlementInvoiceId:input.settlementInvoiceId,correctionPaymentId:input.correctionPaymentId,amount:input.amount,outstanding:input.amount}});});
 source.amountMinor=partialSourceB.amountMinor;source.bankAccountId=partialSourceB.bankAccountId;fields.paymentNumber="later-draft-reference";expect(await sendSupplierCorrection(fetcher,"/api/original-context/payments",attempt)).toBe("prepared-150");expect(await sendSupplierCorrection(fetcher,"/api/original-context/payments",attempt)).toBe("prepared-150");expect(fetcher.mock.calls[0]).toEqual(fetcher.mock.calls[1]);expect(JSON.parse(fetcher.mock.calls[0][1].body as string)).toMatchObject({correctionPaymentId:"original-150",amount:"150000.00",bankAccount:"bank-A",paymentNumber:"captured-reference"});
});
it("does not acknowledge another partial payment, wrong amount or unsuccessful correction response",async()=>{
 const input=supplierCorrectionInput(partialSourceB,{id:"actual-invoice",partyId:"supplier",currency:"THB"},{paymentNumber:"correction",paymentDate:"2026-11-16"}),attempt={input,body:JSON.stringify(input),key:"exact-key"},correct={id:"prepared-100",status:"draft",version:1,settlementInvoiceId:input.settlementInvoiceId,correctionPaymentId:input.correctionPaymentId,amount:input.amount,outstanding:input.amount};
 for(const data of [{...correct,correctionPaymentId:partialSourceA.paymentId},{...correct,settlementInvoiceId:"foreign-invoice"},{...correct,amount:"150000.00"},{...correct,outstanding:"0.00"},{...correct,status:"settled"},{id:"unrelated"}])await expect(sendSupplierCorrection(async()=>Response.json({data}),"/api/original-context/payments",attempt)).rejects.toThrow();
 await expect(sendSupplierCorrection(async()=>Response.json({error:{message:"Original Finance authority changed"}},{status:403}),"/api/original-context/payments",attempt)).rejects.toThrow("Original Finance authority changed");expect(attempt.key).toBe("exact-key");expect(JSON.parse(attempt.body).correctionPaymentId).toBe(partialSourceB.paymentId);
});
it("retains a guarded correction editor for eligible choices and history-only empty choices without exposing private snapshots",()=>{
 const props={base,invoice:{id:"actual-invoice",party_id:"supplier",currency:"THB"},corrections:{capability:"finance.inventory-valuation" as const,settleExisting:true,history:true,revision:"opaque",sources:[partialSourceA,partialSourceB]}};
 for(const corrections of [props.corrections,{...props.corrections,settleExisting:false,sources:[]}]){const html=renderFinancePost(SupplierPaymentCorrectionPreparation,{...props,corrections},unavailableBanking);expect(html).toContain("Correct an original supplier payment");expect(html).toContain('data-client-managed-ready="false"');expect(html).not.toContain('<form');expect(html).not.toMatch(/company_package_grants|sourceImage|assignment_permissions/);}
});

import {ServiceNoChargeConfirmation,ServiceNoChargeConfirmationPresentation} from "../../../src/components/finance/service-no-charge-confirmation";
import {captureServiceNoChargeAttempt,serviceNoChargeAcknowledged,serviceNoChargeView} from "../../../src/server/modules/finance/public/rules/service-no-charge";
const noChargeControls={invoiceId:"covered-original",version:1,contextKey:"a".repeat(64),canConfirm:true,confirmed:false};
it("keeps newer Service no-charge edits visible after a confirmed Server refresh until explicit discard",()=>{
 const retained={reason:"A newer independent review draft",notice:"The captured confirmation was saved. Your newer draft is retained.",uncertain:false},confirmed={...noChargeControls,version:2,confirmed:true,canConfirm:false};
 expect(serviceNoChargeView(confirmed,retained)).toEqual({showReview:true,canSubmit:false});
 const html=renderFinancePost(ServiceNoChargeConfirmationPresentation,{controls:confirmed,scope:"draft-scope",...retained,busy:false,error:"",onSubmit:()=>{},onReason:()=>{},onDiscard:()=>{}});
 expect(html).toContain("Service No-Charge Confirmation retained");expect(html).toContain(retained.notice);expect(html).toContain("Review Service no-charge fulfillment");
 expect(serviceNoChargeView(confirmed,{reason:"",notice:"",uncertain:false})).toEqual({showReview:false,canSubmit:false});
 const clean=renderFinancePost(ServiceNoChargeConfirmation,{base,controls:confirmed});expect(clean).toContain("There is no customer amount to collect");expect(clean).not.toContain("Review Service no-charge fulfillment");
});
it("retains the original no-charge body, key and context across edits and acknowledges only its exact decision",()=>{
 const attempt=captureServiceNoChargeAttempt(noChargeControls," Original captured independent review ","original-key","original-context"),later={...noChargeControls,invoiceId:"other-invoice",version:9,contextKey:"b".repeat(64)};
 expect(captureServiceNoChargeAttempt(later,"Later draft","other-key","other-context")).not.toEqual(attempt);expect(attempt).toEqual({invoiceId:"covered-original",version:1,key:"original-key",scope:"original-context",body:JSON.stringify({expectedVersion:1,contextKey:"a".repeat(64),reason:"Original captured independent review"})});expect(Object.isFrozen(attempt)).toBe(true);
 const receipt={id:attempt.invoiceId,status:"no_charge_confirmed",version:2,confirmationId:"service-no-charge-original-key"};expect(serviceNoChargeAcknowledged(receipt,attempt.invoiceId,attempt.version,attempt.key)).toBe(true);
 for(const wrong of [{...receipt,id:later.invoiceId},{...receipt,version:3},{...receipt,status:"paid"},{...receipt,confirmationId:"service-no-charge-other-key"},null])expect(serviceNoChargeAcknowledged(wrong,attempt.invoiceId,attempt.version,attempt.key)).toBe(false);
 expect(serviceNoChargeView({...noChargeControls,canConfirm:false},{reason:"later",notice:"",uncertain:true})).toEqual({showReview:true,canSubmit:true});
});

it.each(['revision','stale boundary','removed boundary'] as const)('retains the exact no-charge attempt and newer reason after late %s during its actual handler lazy body',async changed=>{
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript');
 const sourcePaths=['../../../src/components/finance/service-no-charge-confirmation.tsx','../../../src/components/finance/finance-draft-boundary.tsx'],sources=await Promise.all(sourcePaths.map(path=>readFile(new URL(path,import.meta.url),'utf8')));
 const actualFunction=(source:string,name:string)=>{const file=ts.createSourceFile('actual-owner.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),found:import('typescript').FunctionDeclaration[]=[];const visit=(node:import('typescript').Node)=>{if(ts.isFunctionDeclaration(node)&&node.name?.text===name)found.push(node);ts.forEachChild(node,visit);};visit(file);if(found.length!==1)throw Error(`Expected one actual ${name} function.`);return found[0].getText(file);};
 const exact=[actualFunction(sources[1],'captureFinanceForm'),actualFunction(sources[1],'acknowledgeFinanceForm'),actualFunction(sources[0],'submit')],emitted=ts.transpileModule(exact.join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 // This executes the checked-in handler body with explicit state closure ports;
 // React hooks, source bodies and acknowledgement predicates are not replaced.
 const node={isConnected:true,dataset:{workUser:'reviewer',workRevision:'7',workCompany:'original-company',workAssignment:'original-assignment',workStale:'false'}},browser=Object.assign(new EventTarget(),{location:{origin:'https://finance-dev-erpx.jesadakorn.com'}});
 let edit='original-edit';const form={isConnected:true,getAttribute:()=>edit},events:string[]=[];
 for(const type of ['erpx:finance-command-acknowledged','erpx:record-command-saved','erpx:work-changed'])browser.addEventListener(type,()=>events.push(type));
 vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'&&node.isConnected?node:null,activeElement:{closest:()=>form}});
 const {workFetch}=await import('../../../src/lib/work_projection/work-fetch'),{captureWorkCommandContext}=await import('../../../src/lib/work_projection/command-context'),{financeDocumentCommandContext,financeDocumentFormUnchanged}=await import('../../../src/lib/finance/document-post-control'),{announceWorkChange}=await import('../../../src/lib/work_projection/work-refresh');
 const pending={current:false},attempt:{current:{command:ReturnType<typeof captureServiceNoChargeAttempt>}|null}={current:null},state:Record<string,unknown[]>={},refresh=vi.fn();
 let signalBody!:()=>void,release!:ReadableStreamDefaultController<Uint8Array>,calls=0;
 const bodyStarted=new Promise<void>(resolve=>{signalBody=resolve;}),body=new ReadableStream<Uint8Array>({start(controller){release=controller;}});
 class LazyResponse extends Response{override json(){signalBody();return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>{if(++calls===1)throw Error('Lost exact confirmation response');if(calls===2)return new LazyResponse(body,{headers:{'content-type':'application/json'}});return Response.json({data:{id:noChargeControls.invoiceId,status:'no_charge_confirmed',version:2,confirmationId:`service-no-charge-${attempt.current?.command.key}`}});});vi.stubGlobal('fetch',network);
 const names=['exports','pending','attempt','controls','reason','base','financeDocumentCommandContext','captureServiceNoChargeAttempt','workFetch','captureWorkCommandContext','serviceNoChargeAcknowledged','financeDocumentFormUnchanged','announceWorkChange','router','setError','setBusy','setUncertain','setNotice','setReason'],run=new Function(...names,`${emitted}\nreturn submit();`) as (...ports:unknown[])=>Promise<void>,setter=(name:string)=>(value:unknown)=>{(state[name]??=[]).push(value);};
 const submit=(reason:string)=>run({},pending,attempt,noChargeControls,reason,base,financeDocumentCommandContext,captureServiceNoChargeAttempt,workFetch,captureWorkCommandContext,serviceNoChargeAcknowledged,financeDocumentFormUnchanged,announceWorkChange,{refresh},...['error','busy','uncertain','notice','reason'].map(setter));
 await submit('Original independent coverage reason');const captured=attempt.current;if(!captured)throw Error('The exact original uncertain no-charge attempt is required.');
 edit='newer-local-edit';const retry=submit('Newer independent review draft');await bodyStarted;if(changed==='revision')node.dataset.workRevision='8';else if(changed==='stale boundary')node.dataset.workStale='true';else node.isConnected=false;
 release.enqueue(new TextEncoder().encode(JSON.stringify({data:{id:captured.command.invoiceId,status:'no_charge_confirmed',version:2,confirmationId:`service-no-charge-${captured.command.key}`}})));release.close();await retry;
 expect(events).toEqual([]);expect(refresh).not.toHaveBeenCalled();expect(attempt.current).toBe(captured);expect(state.uncertain.at(-1)).toBe(true);expect(state.reason??[]).toEqual([]);expect(state.error.at(-1)).toMatch(/context|role/i);
 node.dataset.workRevision='7';node.dataset.workStale='false';node.isConnected=true;await submit('Newer independent review draft');expect(attempt.current).toBeNull();expect(state.uncertain.at(-1)).toBe(false);expect(state.reason??[]).toEqual([]);expect(state.notice.at(-1)).toContain('newer draft is retained');expect(refresh).toHaveBeenCalledOnce();
 expect(events).toEqual(['erpx:finance-command-acknowledged','erpx:record-command-saved','erpx:work-changed']);
 expect(network.mock.calls.map(call=>call[1]?.body)).toEqual([captured.command.body,captured.command.body,captured.command.body]);expect(network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual([captured.command.key,captured.command.key,captured.command.key]);
});
it('dispatches only the exact no-charge API action with its original WorkContext, captured body and key behind the unchanged origin and JSON guards',async()=>{
 const actor={userId:'fixture-finance-reviewer',name:'Fixture reviewer',email:'reviewer@example.test',assignmentId:'fixture-assignment',role:'finance_approver',companyId:'fixture-company',branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'acme',holdingName:'Acme',scope:{kind:'holding' as const,holdingId:'h001',holdingSlug:'acme',holdingName:'Acme',companyId:'fixture-company'},assignmentScope:'holding' as const,inherited:false,modules:['finance'],assignments:[],workspaces:[]},input={expectedVersion:1,contextKey:'a'.repeat(64),reason:'Independent captured contractual source review'},result={id:'covered-original',status:'no_charge_confirmed',version:2,confirmationId:'service-no-charge-captured-key'},confirm=vi.fn(async()=>result),origin='https://console-dev-erpx.jesadakorn.com';
 vi.doMock('@/server/modules/identity_access/public/authorization',async()=>({...await vi.importActual<object>('@/server/modules/identity_access/public/authorization'),requireWorkContext:async()=>actor}));vi.doMock('@/server/modules/finance/public/service-no-charge',()=>({confirmServiceNoCharge:confirm}));vi.doMock('@/server/modules/manage_erpx/public/public-origin',()=>({requestPublicOrigin:()=>origin}));
 try{const {POST}=await import('../../../src/app/api/v1/holdings/[holdingSlug]/finance/[...path]/route'),context=(path=['invoices','covered-original','confirm-no-charge'])=>({params:Promise.resolve({holdingSlug:'acme',path})}),request=(headers:Record<string,string>={},body=JSON.stringify(input))=>new Request(`${origin}/api/v1/holdings/acme/finance/invoices/covered-original/confirm-no-charge`,{method:'POST',headers:{Origin:origin,'Content-Type':'application/json','Idempotency-Key':'captured-key',...headers},body});
  const response=await POST(request(),context());expect(response.status).toBe(200);expect(await response.json()).toEqual({data:result});expect(confirm).toHaveBeenCalledExactlyOnceWith(actor,'covered-original',input,'captured-key');
  for(const [req,ctx,status,code] of [[request({Origin:'https://foreign.test'}),context(),403,'ORIGIN_FORBIDDEN'],[request({},'{'),context(),422,'INVALID_JSON'],[request({'Idempotency-Key':''}),context(),422,'IDEMPOTENCY_KEY_REQUIRED'],[request(),context(['invoices','covered-original','confirm-no-charge','unregistered']),404,'NOT_FOUND']] as const){const denied=await POST(req,ctx);expect(denied.status).toBe(status);expect(await denied.json()).toMatchObject({error:{code}});}expect(confirm).toHaveBeenCalledTimes(1);
 }finally{for(const path of ['@/server/modules/identity_access/public/authorization','@/server/modules/finance/public/service-no-charge','@/server/modules/manage_erpx/public/public-origin'])vi.doUnmock(path);vi.resetModules();}
});

async function originalPaymentSubmitCode(){const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),source=await readFile(new URL('../../../src/components/finance/payment-correction.tsx',import.meta.url),'utf8'),file=ts.createSourceFile('original-payment-correction.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),found:import('typescript').FunctionDeclaration[]=[];const visit=(node:import('typescript').Node)=>{if(ts.isFunctionDeclaration(node)&&node.name?.text==='submit')found.push(node);ts.forEachChild(node,visit);};visit(file);expect(found).toHaveLength(1);return ts.transpileModule(found[0].getText(file),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;}
it.each(['revision','stale','removed','generation'] as const)('original PaymentCorrection withholds key clearing/events/refresh after late %s original JSON',async changed=>{
 const {workFetch}=await import('../../../src/lib/work_projection/work-fetch'),{setContextSwitching}=await import('../../../src/lib/work_projection/context-switch'),{captureFinanceForm}=await import('../../../src/components/finance/finance-draft-boundary'),code=await originalPaymentSubmitCode(),node={isConnected:true,dataset:{workUser:'reviewer',workRevision:'7',workCompany:'company',workAssignment:'reviewer-role',workStale:'false'}},form={isConnected:true,getAttribute:()=> 'original'},browser=Object.assign(new EventTarget(),{location:{origin:'https://finance-dev-erpx.jesadakorn.com'}}),events:string[]=[];
 for(const name of ['erpx:record-command-saved','erpx:finance-command-acknowledged','erpx:work-changed'])browser.addEventListener(name,()=>events.push(name));vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'&&node.isConnected?node:null,activeElement:{closest:()=>form},documentElement:{dataset:{}}});
 let release!:()=>void,started!:()=>void,first=true;const ready=new Promise<void>(resolve=>{started=resolve;}),paused=new Promise<void>(resolve=>{release=resolve;});class OriginalJSON extends Response{override async json(){started();await paused;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>{const payload={data:{id:'payment',status:'voided',reversalJournalId:'reversing-journal'}};if(first){first=false;return new OriginalJSON(JSON.stringify(payload),{headers:{'Content-Type':'application/json'}});}return Response.json(payload);});vi.stubGlobal('fetch',network);
 const values={periodId:'period',entryDate:'2026-10-16',reason:'Independent correction evidence'},Input=class{get(name:keyof typeof values){return values[name];}},state:Record<string,unknown[]>={},set=(name:string)=>(value:unknown)=>{(state[name]??=[]).push(value);},router={refresh:vi.fn()},names=['FormData','payment','allowed','busyRef','commands','setError','setHasAttempt','captureFinanceForm','setBusy','router','setSaved'];
 const {createPaymentCorrectionCommands}=await import('../../../src/lib/finance/sales-command'),commands={current:createPaymentCorrectionCommands(()=>({base,paymentId:'payment'}),workFetch)},run=new Function(...names,`${code}\nreturn submit;`) as (...ports:unknown[])=>(form:unknown)=>Promise<void>,submit=run(Input,{id:'payment',version:2},true,{current:false},commands,set('error'),set('hasAttempt'),captureFinanceForm,set('busy'),router,set('saved'));
 try{const pending=submit(form);await ready;if(changed==='revision')node.dataset.workRevision='8';else if(changed==='stale')node.dataset.workStale='true';else if(changed==='removed')node.isConnected=false;else setContextSwitching(true);release();await pending;const early=[...events],refreshed=router.refresh.mock.calls.length;node.dataset.workRevision='7';node.dataset.workStale='false';node.isConnected=true;setContextSwitching(false);await submit(form);const keys=network.mock.calls.map(([,init])=>new Headers(init?.headers).get('Idempotency-Key'));expect(early).toEqual([]);expect(refreshed).toBe(0);expect(keys[1]).toBe(keys[0]);expect(router.refresh).toHaveBeenCalledOnce();}finally{release();setContextSwitching(false);}
});

it.each([false,true])('PaymentCorrection explicit retry preserves first form completion defined=%s and later edited draft',async defined=>{
 const {workFetch}=await import('../../../src/lib/work_projection/work-fetch'),{captureFinanceForm}=await import('../../../src/components/finance/finance-draft-boundary'),{createPaymentCorrectionCommands}=await import('../../../src/lib/finance/sales-command'),code=await originalPaymentSubmitCode(),node={isConnected:true,dataset:{workUser:'reviewer',workRevision:'7',workCompany:'company',workAssignment:'reviewer-role',workStale:'false'}},browser=Object.assign(new EventTarget(),{location:{origin:'https://finance-dev-erpx.jesadakorn.com'}}),events:string[]=[];let edit='original',active=defined,posts=0;
 const form={isConnected:true,getAttribute:()=>edit};for(const name of ['erpx:record-command-saved','erpx:finance-command-acknowledged','erpx:work-changed'])browser.addEventListener(name,()=>events.push(name));vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'?node:null,activeElement:{closest:()=>active?form:null},documentElement:{dataset:{}}});
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>{if(++posts===1)throw Error('Unknown original reversal result');return Response.json({data:{id:'payment',status:'voided',reversalJournalId:'posted-original-reversal'}});});vi.stubGlobal('fetch',network);
 const values={periodId:'period',entryDate:'2026-10-16',reason:'Original independent correction'},Input=class{get(name:keyof typeof values){return values[name];}},commands={current:createPaymentCorrectionCommands(()=>({base,paymentId:'payment'}),workFetch)},state:Record<string,unknown[]>={},set=(name:string)=>(value:unknown)=>{(state[name]??=[]).push(value);},router={refresh:vi.fn()},names=['FormData','payment','allowed','busyRef','commands','setError','setHasAttempt','captureFinanceForm','setBusy','router','setSaved'];
 const run=new Function(...names,`${code}\nreturn submit;`) as (...ports:unknown[])=>(form:unknown,retryOriginal?:boolean)=>Promise<void>,submit=run(Input,{id:'payment',version:2},true,{current:false},commands,set('error'),set('hasAttempt'),captureFinanceForm,set('busy'),router,set('saved'));
 await submit(form);expect(commands.current.pending()).toBe(true);const original=JSON.stringify({expectedVersion:2,...values});edit='newer';active=true;values.reason='Newer unsent correction draft';await submit(form);expect(network).toHaveBeenCalledTimes(1);await submit(form,true);expect(commands.current.pending()).toBe(false);expect(network.mock.calls.map(([,init])=>init?.body)).toEqual([original,original]);expect(new Headers(network.mock.calls[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(network.mock.calls[0][1]?.headers).get('Idempotency-Key'));
 if(defined){expect(router.refresh).not.toHaveBeenCalled();expect(state.saved.at(-1)).toBe(true);expect(events).toEqual(['erpx:finance-command-acknowledged','erpx:record-command-saved','erpx:work-changed']);}else{expect(router.refresh).toHaveBeenCalledOnce();expect(events).toEqual(['erpx:work-changed']);}
});
// Added actual Finance boundary + SalesInvoiceEditor dismissal regression.
type InvoiceDrawerNode={type:unknown;props:Record<string,unknown>};
function invoiceDrawerNodes(value:unknown):InvoiceDrawerNode[]{
 if(Array.isArray(value))return value.flatMap(invoiceDrawerNodes);
 if(!value||typeof value!=='object'||!('props'in value)||!('type'in value))return [];
 const node=value as InvoiceDrawerNode;return[node,...invoiceDrawerNodes(node.props.children)];
}
async function mountedInvoiceDrawer(options:{href?:string;inline?:boolean}={}){
 const ts=await import('typescript'),{readFile}=await import('node:fs/promises'),{resolve}=await import('node:path'),switching=await import('../../../src/lib/work_projection/context-switch'),{canonicalUiHref}=await import('../../../src/lib/work_projection/destination-registry'),{calculateInvoiceLines}=await import('../../../src/server/modules/finance/public/rules/finance-invoice-lines');
 type Cell={value:unknown};type Effect={deps:unknown[];cleanup?:()=>void};
 let active:{cells:Cell[];effects:Effect[];index:number;queued:(()=>void)[]}|undefined;
 const need=()=>{if(!active)throw Error('ACTUAL_FINANCE_HOOK_RENDER_REQUIRED');return active;},draftContext={value:undefined as unknown,Provider:Symbol('FinanceDraftScope')};
 const element=(type:unknown,props:Record<string,unknown>|null,...children:unknown[])=>{const result:InvoiceDrawerNode={type,props:{...props,children:children.length===1?children[0]:children}};if(type===draftContext.Provider)draftContext.value=result.props.value;return result;};
 const hooks={createElement:element,Fragment:'Fragment',createContext:()=>draftContext,useContext:()=>draftContext.value,useId:()=> 'actual-finance-draft-scope',useState:(initial:unknown)=>{const owner=need(),index=owner.index++;owner.cells[index]??={value:typeof initial==='function'?Reflect.apply(initial,undefined,[]):initial};return[owner.cells[index].value,(next:unknown)=>{owner.cells[index].value=typeof next==='function'?Reflect.apply(next,undefined,[owner.cells[index].value]):next;}];},useRef:(initial:unknown)=>{const owner=need(),index=owner.index++;owner.cells[index]??={value:{current:initial}};return owner.cells[index].value;},useEffect:(effect:()=>void|(()=>void),deps:unknown[])=>{const owner=need(),index=owner.index++,old=owner.effects[index];if(!old||deps.some((value,position)=>value!==old.deps[position]))owner.queued.push(()=>{old?.cleanup?.();const cleanup=effect();owner.effects[index]={deps,...(typeof cleanup==='function'?{cleanup}:{})};});}};
 function controller(render:()=>unknown){const owner={cells:[]as Cell[],effects:[]as Effect[],index:0,queued:[]as (()=>void)[]};return{render(){owner.index=0;active=owner;let tree:unknown;try{tree=render();}finally{active=undefined;}for(const effect of owner.queued.splice(0))effect();return tree;},close(){for(const effect of owner.effects)effect?.cleanup?.();}};}
 class DraftElement{isConnected=true;scope='actual-finance-draft-scope';edit='edited';closest(){return this;}contains(value:unknown){return value===this;}getAttribute(name:string){return name==='data-finance-draft'?this.scope:name==='data-erpx-edit-version'?this.edit:null;}setAttribute(name:string,value:string){if(name==='data-erpx-edit-version')this.edit=value;}}
 const url=new URL(options.href??'https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/documents/sales/invoices?new=invoice'),listeners=new Map<string,EventListener>(),browser=Object.assign(new EventTarget(),{location:{origin:url.origin,href:url.href,hash:url.hash},history:{state:{},pushState:vi.fn()}}),documentPort={querySelector:()=>null,addEventListener:(name:string,listener:EventListener)=>listeners.set(name,listener),removeEventListener:(name:string,listener:EventListener)=>{if(listeners.get(name)===listener)listeners.delete(name);}};
 vi.stubGlobal('window',browser);vi.stubGlobal('document',documentPort);vi.stubGlobal('Element',DraftElement);
 const boundaryText=await readFile(resolve('src/components/finance/finance-draft-boundary.tsx'),'utf8'),boundaryCode=ts.transpileModule(boundaryText,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React,jsxFactory:'createElement',esModuleInterop:true}}).outputText,boundaryModule:{exports:Record<string,unknown>}={exports:{}};
 const requireBoundary=(name:string)=>{if(name==='react')return hooks;if(name==='next/navigation')return{usePathname:()=> url.pathname,useSearchParams:()=>new URLSearchParams(url.search)};if(name==='@/lib/work_projection/context-switch')return switching;if(name==='@/components/ui/dialog')return{Dialog:'Dialog'};if(name==='@/components/ui/foundation')return{Button:'Button'};throw Error('UNEXPECTED_FINANCE_BOUNDARY_IMPORT:'+name);};
 new Function('require','module','exports','createElement',boundaryCode)(requireBoundary,boundaryModule,boundaryModule.exports,element);
 const Boundary=boundaryModule.exports.FinanceDraftBoundary,closeHook=boundaryModule.exports.useFinanceDraftClose,scopeHook=boundaryModule.exports.useFinanceDraftScope;if(typeof Boundary!=='function'||typeof closeHook!=='function'||typeof scopeHook!=='function')throw Error('ACTUAL_FINANCE_BOUNDARY_EXPORTS_REQUIRED');
 const source=await readFile(resolve('src/components/finance/sales-controls.tsx'),'utf8'),ast=ts.createSourceFile('sales-controls.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),declaration=ast.statements.find(statement=>ts.isFunctionDeclaration(statement)&&statement.name?.text==='SalesInvoiceEditor');if(!declaration)throw Error('ACTUAL_SALES_INVOICE_EDITOR_REQUIRED');
 const editorCode=ts.transpileModule(declaration.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React,jsxFactory:'createElement'}}).outputText,editorModule:{exports:Record<string,unknown>}={exports:{}},router={replace:vi.fn()},command={pending:false,unresolved:false,send:vi.fn(),error:'',saved:false,retryOriginal:vi.fn()};
 new Function('module','exports','createElement','useState','useRef','useFinanceDraftClose','useFinanceDraftScope','useSalesCommand','useRouter','canonicalUiHref','hasUnsavedWork','calculateInvoiceLines','minorToDecimal','businessDate','styles','FormDrawer','FormGrid','Field','Select','TextInput','TextArea','Button','InlineNotice',editorCode)(editorModule,editorModule.exports,element,hooks.useState,hooks.useRef,closeHook,scopeHook,()=>command,()=>router,canonicalUiHref,switching.hasUnsavedWork,calculateInvoiceLines,(value:number)=>(value/100).toFixed(2),()=> '2026-10-18',{form:'form',line:'line',total:'total'},'FormDrawer','FormGrid','Field','Select','TextInput','TextArea','Button','InlineNotice');
 const Editor=editorModule.exports.SalesInvoiceEditor;if(typeof Editor!=='function')throw Error('ACTUAL_EDITOR_COMPILED_EXPORT_REQUIRED');
 const boundary=controller(()=>Reflect.apply(Boundary,undefined,[{children:null}])),editor=controller(()=>Reflect.apply(Editor,undefined,[{base:'/holdings/acme',parties:[],defaults:[],inline:options.inline}])) ;let boundaryTree=boundary.render(),editorTree=editor.render();
 const originalForm=new DraftElement(),otherForm=new DraftElement(),form=invoiceDrawerNodes(editorTree).find(node=>node.type==='form');if(!form)throw Error('ACTUAL_EDITOR_FORM_REQUIRED');const ref=form.props.ref;if(ref&&typeof ref==='object'&&'current'in ref)ref.current=originalForm;
 const renderBoundary=()=>boundaryTree=boundary.render(),renderEditor=()=>editorTree=editor.render(),dialog=()=>{const node=invoiceDrawerNodes(boundaryTree).find(value=>value.type==='Dialog');if(!node)throw Error('ACTUAL_DISCARD_DIALOG_REQUIRED');return node;},drawer=()=>{const node=invoiceDrawerNodes(editorTree).find(value=>value.type==='FormDrawer');if(!node)throw Error('ACTUAL_FORM_DRAWER_REQUIRED');return node;};
 const invoke=(value:unknown)=>{if(typeof value!=='function')throw Error('ACTUAL_HANDLER_REQUIRED');Reflect.apply(value,undefined,[]);},click=(label:string)=>{const button=invoiceDrawerNodes(dialog()).find(node=>node.type==='Button'&&node.props.children===label);if(!button)throw Error('ACTUAL_CONFIRMATION_BUTTON_REQUIRED');invoke(button.props.onClick);renderBoundary();renderEditor();};
 return{originalForm,otherForm,command,router,browser,dirty(form:DraftElement){const listener=listeners.get('input');if(!listener)throw Error('ACTUAL_NATIVE_INPUT_READER_REQUIRED');listener({target:form}as unknown as Event);},dismiss(){invoke(drawer().props.onClose);renderBoundary();renderEditor();},dialog,drawer,click,hasUnsavedWork:switching.hasUnsavedWork,renderEditor,replaceRoot(){if(!ref||typeof ref!=='object'||!('current'in ref))throw Error('ACTUAL_SCOPED_ROOT_REF_REQUIRED');const next=new DraftElement();ref.current=next;return next;},close(){editor.close();boundary.close();}};
}
it.each(['Escape','Close'])('actual Sales invoice %s uses Finance confirmation and discards only its own form',async()=>{
 const mounted=await mountedInvoiceDrawer();try{
  mounted.dirty(mounted.originalForm);mounted.dirty(mounted.otherForm);expect(mounted.hasUnsavedWork()).toBe(true);mounted.dismiss();expect(mounted.dialog().props).toMatchObject({open:true,title:'Discard unsaved Finance changes?'});expect(mounted.drawer().props.open).toBe(true);expect(mounted.router.replace).not.toHaveBeenCalled();
  mounted.click('Keep editing');expect(mounted.dialog().props.open).toBe(false);expect(mounted.drawer().props.open).toBe(true);expect(mounted.hasUnsavedWork()).toBe(true);mounted.dismiss();mounted.click('Discard changes');expect(mounted.drawer().props.open).toBe(false);expect(mounted.router.replace).not.toHaveBeenCalled();expect(mounted.browser.location.href).toBe('https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/documents/sales/invoices?new=invoice');expect(mounted.hasUnsavedWork()).toBe(true);mounted.otherForm.isConnected=false;expect(mounted.hasUnsavedWork()).toBe(false);
 }finally{mounted.close();}
});
it('actual Sales invoice scoped discard refuses a replaced owner root and retains other Finance drafts',async()=>{
 const mounted=await mountedInvoiceDrawer();try{mounted.dirty(mounted.originalForm);mounted.dirty(mounted.otherForm);mounted.dismiss();mounted.replaceRoot();mounted.click('Discard changes');expect(mounted.drawer().props.open).toBe(true);expect(mounted.router.replace).not.toHaveBeenCalled();expect(mounted.hasUnsavedWork()).toBe(true);}finally{mounted.close();}
});
it.each(['pending','unresolved']as const)('actual Sales invoice retains its native dismissal guard for %s commands',async state=>{
 const mounted=await mountedInvoiceDrawer();try{mounted.command[state]=true;mounted.renderEditor();expect(mounted.drawer().props.dismissDisabled).toBe(true);expect(mounted.router.replace).not.toHaveBeenCalled();expect(mounted.command.send).not.toHaveBeenCalled();}finally{mounted.close();}
});

it.each(['clean','discard']as const)('actual Sales invoice %s close removes only its open flag and retains current repeated filters, cursor and hash',async mode=>{
 const href='https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/documents/sales/invoices?tab=invoices&q=services&status=draft&status=posted&cursor=page-2&new=invoice&new=quotation#review',mounted=await mountedInvoiceDrawer({href});try{
  if(mode==='discard')mounted.dirty(mounted.originalForm);mounted.dismiss();if(mode==='discard')mounted.click('Discard changes');
  expect(mounted.drawer().props.open).toBe(false);expect(mounted.hasUnsavedWork()).toBe(false);expect(mounted.router.replace).toHaveBeenCalledExactlyOnceWith('/holdings/acme/finance/documents/sales/invoices?tab=invoices&q=services&status=draft&status=posted&cursor=page-2&new=quotation#review',{scroll:false});
  const target=new URL(String(mounted.router.replace.mock.calls[0]?.[0]),'https://finance-dev-erpx.jesadakorn.com');expect(target.pathname).toBe(new URL(href).pathname);expect(target.searchParams.getAll('status')).toEqual(['draft','posted']);expect(target.searchParams.getAll('new')).toEqual(['quotation']);expect(target.searchParams.get('cursor')).toBe('page-2');expect(target.hash).toBe('#review');
 }finally{mounted.close();}
});
it('actual Sales invoice clean close keeps another dirty Sales owner mounted without URL replacement',async()=>{
 const href='https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/documents/sales/orders?tab=orders&new=invoice&cursor=orders-page#draft',mounted=await mountedInvoiceDrawer({href});try{
  mounted.dirty(mounted.otherForm);mounted.dismiss();expect(mounted.drawer().props.open).toBe(false);expect(mounted.dialog().props.open).toBe(false);expect(mounted.router.replace).not.toHaveBeenCalled();expect(mounted.browser.location.href).toBe(href);expect(mounted.otherForm.isConnected).toBe(true);expect(mounted.hasUnsavedWork()).toBe(true);
 }finally{mounted.close();}
});
it('actual Sales invoice inline close and already-absent open flag do not navigate',async()=>{
 for(const options of [{inline:true},{href:'https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/documents/sales/invoices?status=draft#history'}]){const mounted=await mountedInvoiceDrawer(options);try{mounted.dismiss();expect(mounted.drawer().props.open).toBe(false);expect(mounted.router.replace).not.toHaveBeenCalled();}finally{mounted.close();}}
});
