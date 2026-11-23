import type {WorkContext} from '@erpx/server-identity_access/public/authorization';import type {Row} from '@erpx/server-commerce/public/commerce';import type {prepareJournal} from '@erpx/server-finance/public/journal-planning';import type {snapshotMoney} from '@erpx/server-finance/public/company-money';import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';export interface ManufacturingAccountingRepository<Effect>{activeChart(chartId:string,companyId:string):Promise<{ base_currency: string }|null>;
guardInventoryMapping(key:string,value:"chart",companyId:string,chartId:string,inventoryAccountId:string):Effect;
guardAccountCategory(key:string,value:string,account:string,companyId:string,chartId:string,accountKind:'asset'|'scrap_expense'|'conversion'):Effect;
approvedProfile(companyId:string):Promise<Row|null>;
guardApprovedProfile(key:string,value:"production-accounting-profile",value2:string,companyId:string,value3:number):Effect;
guardFunctionalCurrency(key:string,value:"production-currency",companyId:string,value2:string,value3:string):Effect;
postingPeriod(companyId:string,entryDate:string,entryDate2:string):Promise<{ id: string }|null>;
preparePostingEvidence(value:string,companyId:string,orderId:string,orderVersion:number,phase:"output" | "issue" | "report" | "scrap",id:string | number | null,version:string | number | null,id2:string,total:number,value2:string,userId:string,now:number):Effect;
prepareCostMovement(id:string,companyId:string,orderId:string,kind:string,value:number,amount:number,sourceId:string,id2:string,entryDate:string,userId:string,now:number):Effect;}export interface OwnerCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:Pick<WorkContext,'companyId'|'branchId'|'departmentId'>):Promise<Result>;}export interface ManufacturingAccountingPorts<Effect>{repository:ManufacturingAccountingRepository<Effect>;clock:Clock;commands:OwnerCommands<Effect>;requirePermission(actor:WorkContext,permission:string):WorkContext;prepareJournal(actor:WorkContext,input:Parameters<typeof prepareJournal>[2],key:string,status:Parameters<typeof prepareJournal>[4]):Promise<{result:{id:string};steps:Effect[]}>;snapshotMoney(actor:WorkContext,input:Parameters<typeof snapshotMoney>[2]):Promise<{statement:Effect}>;}export function createManufacturingAccountingApplication<Effect>(ports:ManufacturingAccountingPorts<Effect>){const r=ports.repository;async function prepareManufacturingPosting( a: WorkContext, input: {
  key: string; orderId: string; orderVersion: number; phase: "issue" | "report" | "output" | "scrap";
  entryDate: string; material?: number; labor?: number; overhead?: number; output?: number; quantityMilli?: number;
}): Promise<{ steps: Effect[]; journalId: string | null }> {
  const profile = await r.approvedProfile(a.companyId);
  if (!profile)
    throw new BusinessRuleError(422, "MANUFACTURING_ACCOUNTING_REQUIRED", "Finance must independently approve a production account map before materials or costs can be posted.");
  const amounts = [input.material ?? 0, input.labor ?? 0, input.overhead ?? 0, input.output ?? 0];
  if (amounts.some(amount => !Number.isSafeInteger(amount) || amount < 0))
    throw new BusinessRuleError(422, "MONEY_OVERFLOW", "Production costs must be nonnegative integer minor units.");
  const total = amounts.reduce((sum, amount) => sum + amount, 0);
  if (!Number.isSafeInteger(total)) throw new BusinessRuleError(422, "MONEY_OVERFLOW", "Production costs exceed ledger precision.");
  const guards: Effect[] = [
    r.guardApprovedProfile(input.key,"production-accounting-profile",String(profile.id),a.companyId,Number(profile.version)),
    r.guardFunctionalCurrency(input.key,"production-currency",a.companyId,String(profile.chart_id),String(profile.inventory_account_id)),
  ];
  if (!total) return { steps: guards, journalId: null };
  const period = await r.postingPeriod(a.companyId,input.entryDate,input.entryDate);
  if (!period) throw new BusinessRuleError(422, "MANUFACTURING_PERIOD_REQUIRED", "Open a Finance period covering the production entry date.");
  const lines: Array<{ accountId: string; debit: string; credit: string; description: string }> = [];
  const add = (account: unknown, debit: number, credit: number, description: string) => {
    if (debit || credit) lines.push({ accountId: String(account), debit: minorToDecimal(debit), credit: minorToDecimal(credit), description });
  };
  if (input.phase === "output" || input.phase === "scrap") {
    add(input.phase === "output" ? profile.inventory_account_id : profile.scrap_account_id, total, 0, input.phase === "output" ? "Finished output capitalized" : "Failed output written off");
    add(profile.wip_account_id, 0, total, "WIP relieved at independent disposition");
  } else {
    add(profile.wip_account_id, total, 0, "Production work in progress");
    add(profile.inventory_account_id, 0, input.material ?? 0, "Materials issued from inventory");
    add(profile.labor_account_id, 0, input.labor ?? 0, "Actual labor clearing");
    add(profile.overhead_account_id, 0, input.overhead ?? 0, "Actual overhead clearing");
  }
  const journal = await ports.prepareJournal( a, {
    chartId: String(profile.chart_id), periodId: period.id, entryDate: input.entryDate, currency: "THB",
    memo: `Production ${input.phase} · ${input.orderId} v${input.orderVersion}`, sourceType: `manufacturing_${input.phase}`,
    sourceId: input.orderId, isOpening: false, lines,
  }, input.key, "posted");
  const steps = [...guards, ...journal.steps], sourceId = `mfg-event-${input.key}`, now = ports.clock.now();
  steps.push(r.preparePostingEvidence(`mfg-posting-${input.key}`,a.companyId,input.orderId,input.orderVersion,input.phase,profile.id,profile.version,journal.result.id,total,JSON.stringify({ inventory: profile.inventory_account_id, wip: profile.wip_account_id, labor: profile.labor_account_id, overhead: profile.overhead_account_id }),a.userId,now));
  const movements: Array<{ kind: string; amount: number }> = input.phase === "output" || input.phase === "scrap"
    ? [{ kind: input.phase === "output" ? "output" : "abnormal_scrap", amount: total }]
    : [{ kind: "material_issue", amount: input.material ?? 0 }, { kind: "labor", amount: input.labor ?? 0 }, { kind: "overhead", amount: input.overhead ?? 0 }];
  for (const [n, movement] of movements.filter(m => m.amount > 0).entries()) {
    const id = `mfg-movement-${input.key}-${n}`;
    steps.push(r.prepareCostMovement(id,a.companyId,input.orderId,movement.kind,input.quantityMilli ?? 0,movement.amount,sourceId,journal.result.id,input.entryDate,a.userId,now));
    steps.push((await ports.snapshotMoney( a, { sourceType: "manufacturing_cost_movement", sourceId: id, amountKind: movement.kind,
      transactionDate: input.entryDate, currency: "THB", currencyScale: 2, amountUnits: String(movement.amount) })).statement);
  }
  return { steps, journalId: journal.result.id };
}return {prepareManufacturingPosting};}
