import {z} from 'zod';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import type {FinanceError} from '../domain/invoice-line-revision';
import {minorToDecimal,multiplyBps} from '@/lib/finance-money';
import {financeCsvRows} from '@/lib/finance-csv';
import {financeReviewSchema} from '../domain/review';
import {budgetEditionSchema,forecastEditionSchema,financialReportSchema,reportRowSchema,type BudgetContent,type PlanRow,type PlanAccount as Account,type LedgerLine,type RetainedReport,type PlanningSourceSnapshot} from '../domain/planning';
type Kind='budget'|'forecast'|'report';type Base=z.infer<typeof budgetEditionSchema>|z.infer<typeof forecastEditionSchema>|z.infer<typeof financialReportSchema>;type Review=z.infer<typeof financeReviewSchema>;
export interface PlanningRepository{moneyScale(companyId:string,currency:string):Promise<number>;tool(companyId:string):Promise<void>;book(actor:WorkContext,chart:string):Promise<{base_currency:string}|null>;accounts(actor:WorkContext,chart:string):Promise<Account[]>;dimension(actor:WorkContext,code:string,value:string):Promise<object|null>;commandReplay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,input:unknown):Promise<Result|null>;replay<T=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<T|null>;parent(actor:WorkContext,parentId:string,chartId:string,kind:Kind):Promise<PlanRow|null>;saveCommit(facts:{actor:WorkContext;kind:Kind;base:Base;content:unknown;key:string;op:string;input:Base&{content:unknown};edition:number;id:string;hash:string;now:number}):Promise<Record<string,unknown>>;bankAccount(actor:WorkContext,id:string):Promise<object|null>;decisionRow(actor:WorkContext,id:string):Promise<PlanRow|null>;decisionCommit(facts:{actor:WorkContext;id:string;input:Review;action:'approve'|'reject';key:string;op:string;row:PlanRow;now:number}):Promise<Record<string,unknown>>;editions(actor:WorkContext,chartId:string):Promise<Record<string,unknown>[]>;snapshots(actor:WorkContext,chartId:string):Promise<Record<string,unknown>[]>;plan(actor:WorkContext,id:string,kind:string):Promise<PlanRow|null>;actual(actor:WorkContext,chartId:string,accountId:string,month:string,dimensions:string):Promise<string|number|null>;category(accountId:string):Promise<{category:string}|null>;ledger(actor:WorkContext,chartId:string,endDate:string):Promise<LedgerLine[]>;lockChart(chartId:string):Promise<unknown>;snapshotCommit(facts:{actor:WorkContext;id:string;input:{startDate:string;endDate:string};key:string;op:string;row:PlanRow;snapshotId:string;result:RetainedReport&Record<string,unknown>&{id:string};sourceHash:string;csv:string;exportHash:string;now:number}):Promise<RetainedReport&{id:string}>;exportRow(actor:WorkContext,id:string):Promise<{result_json:string;export_content:string|null;export_sha256:string|null}|null>;recordRow(actor:WorkContext,id:string):Promise<PlanRow|null>;budgetSources(actor:WorkContext,chartId:string,accountId:string,month:string,dimensions:string,page:number):Promise<Record<string,unknown>[]>;sourceSnapshot(actor:WorkContext,id:string):Promise<PlanningSourceSnapshot|null>;sourceJournals(actor:WorkContext,ids:string[]):Promise<Record<string,unknown>[]>;}
export interface FinancePlanningPorts{error:FinanceError;permission(actor:WorkContext,permission:string):WorkContext;samePerson(actor:WorkContext,userId:string):boolean;now():number;id():string;hash(value:string):Promise<string>;read(actor:WorkContext):Promise<PlanningRepository>;transaction<T>(actor:WorkContext,run:(repo:PlanningRepository)=>Promise<T>):Promise<T>;snapshotTransaction<T>(actor:WorkContext,run:(repo:PlanningRepository)=>Promise<T>):Promise<T>;}
import {canonicalDimensions,createFinancePlanningCalculations} from '../domain/planning-calculations';
export function createFinancePlanning(ports:FinancePlanningPorts){
const {money,safe,nextMonth,reportValues,calculateCashFlow,reportCsv}=createFinancePlanningCalculations(ports.error);




async function accounts(
  repo:PlanningRepository,
  actor: WorkContext,
  chart: string,
  ids: string[],
  dimensions: Record<string, string>[] = [],
) {
  const book = await repo.book(actor,chart);
  if (!book)
    throw ports.error(
      404,
      "CHART_NOT_FOUND",
      "Select an active Company book.",
    );
  await repo.moneyScale(actor.companyId,book.base_currency);
  const rows = (
    await repo.accounts(actor,chart));
  if (ids.some((id) => !rows.some((a) => a.id === id)))
    throw ports.error(
      422,
      "ACCOUNT_INVALID",
      "Every account must belong to the selected Company book.",
    );
  for (const dims of dimensions)
    for (const [code, value] of Object.entries(dims))
      if (
        !(await repo.dimension(actor,code,value))
      )
        throw ports.error(
          422,
          "DIMENSION_INVALID",
          "Select an active Company dimension value.",
        );
  return rows;
}

async function save(
  actor: WorkContext,
  kind: "budget" | "forecast" | "report",
  base:
    | z.infer<typeof budgetEditionSchema>
    | z.infer<typeof forecastEditionSchema>
    | z.infer<typeof financialReportSchema>,
  content: unknown,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const repo=await ports.read(actor),
    op = `finance.${kind}.prepare`,
    input = { ...base, content };
  const prior=await repo.commandReplay(actor,key,op,input);if(prior)return prior;
  await repo.tool(actor.companyId);
  let edition = 1;
  if (base.parentId) {
    const parent = await repo.parent(actor,base.parentId,base.chartId,kind);
    if (
      !parent ||
      parent.version !== base.expectedVersion ||
      parent.name !== base.name
    )
      throw ports.error(
        409,
        "STALE_VERSION",
        "Select the current approved edition and its version to amend.",
      );
    edition = parent.edition + 1;

  }
  const id = ports.id(),
    hash = await ports.hash(JSON.stringify(content)),
    now = ports.now();
  return repo.saveCommit({actor,kind,base,content,key,op,input,edition,id,hash,now});
}

async function prepareBudgetEdition(
  actor: WorkContext,
  raw: z.input<typeof budgetEditionSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = budgetEditionSchema.parse(raw);
  const repo=await ports.read(actor);
  await accounts(
    repo,
    actor,
    input.chartId,
    input.lines.map((l) => l.accountId),
    input.lines.map((l) => l.dimensions),
  );
  const seen = new Set<string>();
  const lines = input.lines.map((l) => {
    if (!l.month.startsWith(`${input.year}-`))
      throw ports.error(
        422,
        "BUDGET_YEAR_MISMATCH",
        "All months must be in the budget year.",
      );
    const id = `${l.accountId}:${l.month}:${canonicalDimensions(l.dimensions)}`;
    if (seen.has(id))
      throw ports.error(
        422,
        "DUPLICATE_BUDGET_LINE",
        "Each account/month/dimension combination must be unique.",
      );
    seen.add(id);
    return {
      accountId: l.accountId,
      month: l.month,
      minor: money(l.amount),
      dimensions: JSON.parse(canonicalDimensions(l.dimensions)) as Record<
        string,
        string
      >,
    };
  });
  return save(actor, "budget", input, { year: input.year, lines }, key);
}

async function importBudgetCsv(
  actor: WorkContext,
  input: {
    chartId: string;
    name: string;
    year: number;
    csv: string;
    parentId?: string;
    expectedVersion?: number;
  },
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const repo=await ports.read(actor),
    list = await accounts(repo, actor, input.chartId, []);
  let rows: Record<string, string>[];
  try {
    rows = financeCsvRows(input.csv);
  } catch (e) {
    throw ports.error(
      422,
      "INVALID_CSV",
      String(e instanceof Error ? e.message : e),
    );
  }
  const lines = rows.map((r) => ({
    accountId: list.find((a) => a.code === r.account)?.id ?? "",
    month: r.month,
    amount: r.amount,
    dimensions: Object.fromEntries(
      Object.entries(r)
        .filter(([k, v]) => k.startsWith("dimension:") && v)
        .map(([k, v]) => [k.slice(10), v]),
    ),
  }));
  return prepareBudgetEdition(
    actor,
    budgetEditionSchema.parse({ ...input, lines }),
    key,
  );
}



async function prepareForecastEdition(
  actor: WorkContext,
  raw: z.input<typeof forecastEditionSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = forecastEditionSchema.parse(raw);
  const repo=await ports.read(actor);
  await accounts(
    repo,
    actor,
    input.chartId,
    input.lines.map((l) => l.accountId),
    input.lines.map((l) => l.dimensions),
  );
  const months = input.lines.flatMap((l) => {
    let base = money(l.monthlyAmount);
    return Array.from({ length: 12 }, (_, i) => {
      if (i) base = safe(multiplyBps(BigInt(base), 10000 + l.monthlyGrowthBps));
      return {
        accountId: l.accountId,
        month: nextMonth(input.startMonth, i),
        dimensions: l.dimensions,
        baseMinor: base,
        goodMinor: safe(multiplyBps(BigInt(base), 10000 + l.goodBps)),
        badMinor: safe(multiplyBps(BigInt(base), 10000 + l.badBps)),
      };
    });
  });
  return save(
    actor,
    "forecast",
    input,
    {
      assumptions: input.assumptions,
      inputs: input.lines,
      startMonth: input.startMonth,
      months,
    },
    key,
  );
}

async function prepareFinancialReport(
  actor: WorkContext,
  raw: z.input<typeof financialReportSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = financialReportSchema.parse(raw);
  const repo=await ports.read(actor);
  const groups = input.rows.filter((r) => r.kind === "accounts");
  await accounts(
    repo,
    actor,
    input.chartId,
    [
      ...groups.flatMap((r) => r.accountIds),
      ...(input.cashFlow?.bankAccountIds ?? []),
      ...(input.cashFlow?.classifications.map((r) => r.accountId) ?? []),
    ],
    groups.map((r) => r.dimensions),
  );
  if (input.cashFlow)
    for (const id of input.cashFlow.bankAccountIds)
      if (
        !(await repo.bankAccount(actor,id))
      )
        throw ports.error(
          422,
          "BANK_ACCOUNT_REQUIRED",
          "Cash flow requires bank control accounts.",
        );
  return save(
    actor,
    "report",
    input,
    {
      rows: input.rows,
      ...(input.cashFlow ? { cashFlow: input.cashFlow } : {}),
    },
    key,
  );
}

async function decideFinancePlan(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof financeReviewSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = ports.permission(actor, "finance.approve");
  const input = financeReviewSchema.parse(raw);
  const op = `finance.plan.${action}`;
  return ports.transaction(actor,async repo=>{
  const prior = await repo.replay(actor, key, op, id, input);
  if (prior) return prior;
  const row = await repo.decisionRow(actor,id);
  if (!row)
    throw ports.error(404, "PLAN_NOT_FOUND", "Finance edition not found.");
  if (row.status !== "draft" || row.version !== input.expectedVersion)
    throw ports.error(
      409,
      "STALE_VERSION",
      "Reload this edition before review.",
    );
  if (ports.samePerson(actor, row.prepared_by))
    throw ports.error(
      403,
      "SELF_REVIEW_FORBIDDEN",
      "Another person must approve this edition.",
    );
  if(action==="approve"){
   const content=JSON.parse(row.content_json) as {lines?:{accountId:string;dimensions:Record<string,string>}[];months?:{accountId:string;dimensions:Record<string,string>}[];rows?:z.infer<typeof reportRowSchema>[];cashFlow?:z.infer<typeof financialReportSchema>["cashFlow"]};
   const lines=content.lines??content.months??[],groups=(content.rows??[]).filter(r=>r.kind==="accounts");
   await accounts(repo,actor,row.chart_id,[...lines.map(l=>l.accountId),...groups.flatMap(g=>g.accountIds),...(content.cashFlow?.bankAccountIds??[]),...(content.cashFlow?.classifications.map(c=>c.accountId)??[])],[...lines.map(l=>l.dimensions),...groups.map(g=>g.dimensions)]);
  }
  const now=ports.now();
  return repo.decisionCommit({actor,id,input,action,key,op,row,now});
  });
}

async function financePlanningWorkspace(
  actor: WorkContext,
  chartId: string,
) {
  actor = ports.permission(actor, "finance.read");
  const repo=await ports.read(actor);
  const list = await accounts(repo, actor, chartId, []);
  return {
    accounts: list,
    editions: (
      await repo.editions(actor,chartId)),
    snapshots:(await repo.snapshots(actor,chartId)),
  };
}

async function plan(
  repo:PlanningRepository,
  actor: WorkContext,
  id: string,
  kind: string,
) {
  const row = await repo.plan(actor,id,kind);
  if (!row)
    throw ports.error(
      404,
      "APPROVED_PLAN_NOT_FOUND",
      "Select an approved Company edition.",
    );
  return row;
}

async function budgetActuals(actor: WorkContext, id: string) {
  actor = ports.permission(actor, "finance.read");
  const repo=await ports.read(actor),
    row = await plan(repo, actor, id, "budget"),
    content = JSON.parse(row.content_json) as BudgetContent;
  const lines = [];
  for (const l of content.lines) {
    const dims = canonicalDimensions(l.dimensions),
      actual = Number(
        (await repo.actual(actor,row.chart_id,l.accountId,l.month,dims)) ?? 0,
      );
    const account = await repo.category(l.accountId),
      natural = ["liability", "equity", "revenue", "other_income"].includes(
        account!.category,
      )
        ? -actual
        : actual;
    lines.push({
      ...l,
      budget: minorToDecimal(l.minor),
      actual: minorToDecimal(natural),
      variance: minorToDecimal(safe(BigInt(natural) - BigInt(l.minor))),
      actualMinor: natural,
    });
  }
  return {
    id,
    edition: row.edition,
    chartId: row.chart_id,
    year: content.year,
    lines,
  };
}

async function ledger(
  repo:PlanningRepository,
  actor: WorkContext,
  chartId: string,
  endDate: string,
) {
  return (
    await repo.ledger(actor,chartId,endDate)).map(({ isFiscalClose, ...line }) =>
    Number(isFiscalClose) === 1 && line.sourceType === "book_projection"
      ? { ...line, isFiscalClose: 1 }
      : line,
  );
}



async function snapshotFinancialReport(
  actor: WorkContext,
  id: string,
  input: { startDate: string; endDate: string },
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  input = z
    .object({ startDate: z.string().date(), endDate: z.string().date() })
    .refine((v) => v.endDate >= v.startDate)
    .parse(input);
  return ports.snapshotTransaction(actor,async (repo) => {
    const op = "finance.report.snapshot",
      prior = await repo.replay<{
        id: string;
        rows: ReturnType<typeof reportValues>;
      }>(actor, key, op, id, input);
    if (prior) return prior;
    const row = await plan(repo, actor, id, "report");
    await repo.lockChart(row.chart_id);
    const source = await ledger(repo, actor, row.chart_id, input.endDate),
      definition = JSON.parse(row.content_json) as Pick<
        z.infer<typeof financialReportSchema>,
        "rows" | "cashFlow"
      >,
      sourceHash = await ports.hash(JSON.stringify(source)),
      rows = reportValues(definition.rows, source, input.startDate),
      cashFlow = definition.cashFlow
        ? calculateCashFlow(source, input.startDate, definition.cashFlow)
        : null,
      snapshotId = ports.id(),
      result = {
        id: snapshotId,
        definitionId: id,
        edition: row.edition,
        chartId: row.chart_id,
        ...input,
        sourceHash,
        rows,
        cashFlow,
      };
    const csv=reportCsv(result),exportHash=await ports.hash(csv);
    const now=ports.now();
    return repo.snapshotCommit({actor,id,input,key,op,row,snapshotId,result,sourceHash,csv,exportHash,now});
  });
}





async function exportFinancialReport(actor:WorkContext,id:string){
 actor=ports.permission(actor,"finance.read");const repo=await ports.read(actor),row=await repo.exportRow(actor,id);
 if(!row)throw ports.error(404,"REPORT_NOT_FOUND","Report snapshot not found.");
 const content=row.export_content??reportCsv(JSON.parse(row.result_json) as RetainedReport,false),hash=await ports.hash(content);
 if(row.export_sha256&&hash!==row.export_sha256)throw ports.error(409,"REPORT_EXPORT_INTEGRITY","The retained export does not match its recorded checksum.");
 return {content,sha256:hash};
}

async function financePlanRecord(actor:WorkContext,id:string){
 ports.permission(actor,'finance.read');const repo=await ports.read(actor);
 const row=await repo.recordRow(actor,id);
 if(!row)throw ports.error(404,'PLAN_NOT_FOUND','Finance edition not found.');
 const data=await financePlanningWorkspace(actor,row.chart_id);
 return {row,data:{...data,editions:data.editions.filter(edition=>edition.id===id),snapshots:data.snapshots.filter(snapshot=>snapshot.definition_id===id)}};
}

async function budgetSourceRecords(actor:WorkContext,id:string,index:number,page=1){
 const report=await budgetActuals(actor,id),line=report.lines[index];if(!line)throw ports.error(404,'BUDGET_LINE_NOT_FOUND','Budget line not found.');
 const repo=await ports.read(actor);
 const rows=(await repo.budgetSources(actor,report.chartId,line.accountId,line.month,canonicalDimensions(line.dimensions),page));
 return {row:{name:`Budget actuals · ${line.month}`,reported_total:line.actual,description:`Budget ${line.budget} · Variance ${line.variance}`},rows,page,hasNext:rows.length>30};
}

async function financialReportSourceRecords(actor:WorkContext,id:string,args:Record<string,string>,page=1){
 ports.permission(actor,'finance.read');const repo=await ports.read(actor);
 const saved=await repo.sourceSnapshot(actor,id);
 if(!saved)throw ports.error(404,'REPORT_NOT_FOUND','Report snapshot not found.');
 if(!args.key&&!args.cashFlow)return {row:{name:'Retained financial report',start_date:saved.start_date,end_date:saved.end_date,result:JSON.parse(saved.result_json),source_hash:saved.source_hash},rows:[],page,hasNext:false};
 const source=await ledger(repo,actor,saved.chart_id,saved.end_date);
 if(await ports.hash(JSON.stringify(source))!==saved.source_hash)throw ports.error(409,'REPORT_SOURCES_CHANGED','The ledger changed since this snapshot. Open the retained report or create a current report.');
 const result=JSON.parse(saved.result_json) as {rows:ReturnType<typeof reportValues>;cashFlow:ReturnType<typeof calculateCashFlow>|null},definition=JSON.parse(saved.content_json) as Pick<z.infer<typeof financialReportSchema>,'rows'|'cashFlow'>;
 let ids:string[]=[],label='',value:string|null=null;
 if(args.cashFlow){
  const key=z.enum(['opening','operating','investing','financing','change','closing']).parse(args.cashFlow),cash=result.cashFlow;
  if(!cash||!definition.cashFlow)throw ports.error(404,'CASH_FLOW_NOT_FOUND','Cash-flow sources unavailable.');
  label=`Cash flow · ${key}`;value=cash[key];
  ids=['opening','closing'].includes(key)?source.filter(line=>definition.cashFlow!.bankAccountIds.includes(line.accountId)&&(key==='closing'||line.entryDate<saved.start_date||line.sourceType==='book_opening')).map(line=>line.journalId):cash.trace.filter(item=>key==='change'||item.kind===key).map(item=>item.journalId);
 }else{const row=result.rows.find(row=>row.key===args.key);if(!row)throw ports.error(404,'REPORT_ROW_NOT_FOUND','Report row not found.');ids=row.journalIds;label=row.label;value=row.value;}
 ids=[...new Set(ids)];const selected=ids.slice((page-1)*30,page*30);
 const rows=selected.length?(await repo.sourceJournals(actor,selected)):[];
 return {row:{name:label,reported_total:value,start_date:saved.start_date,end_date:saved.end_date},rows,page,hasNext:ids.length>page*30};
}
return {prepareBudgetEdition,importBudgetCsv,prepareForecastEdition,prepareFinancialReport,decideFinancePlan,financePlanningWorkspace,budgetActuals,snapshotFinancialReport,exportFinancialReport,financePlanRecord,budgetSourceRecords,financialReportSourceRecords};}


