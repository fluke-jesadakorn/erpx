import {z} from 'zod';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {FinanceError} from '@erpx/server-finance/domain/invoice-line-revision';
import {dimensionSchema,bookRuleSchema,type OpeningRow,type BookAccount,type BookRule,type BookWorkspace} from '@erpx/server-finance/domain/books';
import {financeReviewSchema} from '@erpx/server-finance/domain/review';
type DimensionInput=z.infer<typeof dimensionSchema>;type RuleInput=z.infer<typeof bookRuleSchema>;type ReviewInput=z.infer<typeof financeReviewSchema>;
export interface BookReviewRepository{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;rule(actor:WorkContext,id:string):Promise<BookRule|null>;lockPrimary(chartId:string):Promise<unknown>;opening(companyId:string,chartId:string,date:string):Promise<OpeningRow[]>;sourcePostedSince(chartId:string,date:string):Promise<object|null>;alreadyActive(chartId:string):Promise<object|null>;targetPosted(chartId:string):Promise<object|null>;decisionCommit(facts:{actor:WorkContext;id:string;input:ReviewInput;action:'approve'|'reject';key:string;op:string;rule:BookRule;now:number;rows:OpeningRow[];createOpening:boolean}):Promise<Record<string,unknown>>;}
export interface FinanceBookPorts{error:FinanceError;permission(actor:WorkContext,permission:string):WorkContext;samePerson(actor:WorkContext,userId:string):boolean;ready(actor:WorkContext):Promise<void>;now():number;id():string;hash(value:string):Promise<string>;workspace(actor:WorkContext):Promise<BookWorkspace>;commandReplay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,input:unknown):Promise<Result|null>;moneyScale(companyId:string,currency:string):Promise<number>;opening(companyId:string,chartId:string,date:string):Promise<OpeningRow[]>;charts(actor:WorkContext,primaryChartId:string,secondaryChartId:string):Promise<{id:string;base_currency:string}[]>;chain(actor:WorkContext,primaryChartId:string,secondaryChartId:string):Promise<object|null>;existingRule(secondaryChartId:string):Promise<{primary_chart_id:string}|null>;targetPosted(secondaryChartId:string):Promise<object|null>;openingPeriod(actor:WorkContext,periodId:string,effectiveDate:string):Promise<object|null>;accounts(actor:WorkContext,primaryChartId:string,secondaryChartId:string):Promise<BookAccount[]>;mappingChanged(secondaryChartId:string,primaryAccountId:string,secondaryAccountId:string):Promise<object|null>;dimensionCommit(facts:{actor:WorkContext;input:DimensionInput;key:string;op:string;id:string;now:number;valueIds:string[]}):Promise<Record<string,unknown>>;prepareCommit(facts:{actor:WorkContext;input:RuleInput;key:string;op:string;id:string;now:number;openingHash:string;rows:OpeningRow[]}):Promise<Record<string,unknown>>;reviewTransaction<T>(actor:WorkContext,run:(repo:BookReviewRepository)=>Promise<T>):Promise<T>;}
export function createFinanceBooks(ports:FinanceBookPorts){
async function financeBookWorkspace(actor: WorkContext) {
  actor = ports.permission(actor, "finance.read");
  await ports.ready(actor);
  return ports.workspace(actor);
}

async function createFinanceDimension(
  actor: WorkContext,
  raw: z.input<typeof dimensionSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = dimensionSchema.parse(raw);
  await ports.ready(actor);
  const
    op = "finance.dimension.create";
  const prior = await ports.commandReplay(actor, key, op, input);
  if (prior) return prior;
  const id = ports.id(),
    now = ports.now(),valueIds=input.values.map(()=>ports.id());
  return ports.dimensionCommit({actor,input,key,op,id,now,valueIds});
}

async function prepareBookRule(
  actor: WorkContext,
  raw: z.input<typeof bookRuleSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = bookRuleSchema.parse(raw);
  await ports.ready(actor);
  const
    op = "finance.book.prepare";
  const prior = await ports.commandReplay(actor, key, op, input);
  if (prior) return prior;
  const charts = (
    await ports.charts(actor,input.primaryChartId,input.secondaryChartId));
  if (charts.length !== 2)
    throw ports.error(
      422,
      "BOOK_CHART_REQUIRED",
      "Select two active Company books.",
    );
  await ports.moneyScale(actor.companyId,charts[0].base_currency);
  await ports.moneyScale(actor.companyId,charts[1].base_currency);
  if (charts[0].base_currency !== charts[1].base_currency)
    throw ports.error(
      422,
      "BOOK_CURRENCY_ADAPTER_REQUIRED",
      "The initial book adapter requires the same functional currency to preserve settled carrying amounts.",
    );
  if (
    await ports.chain(actor,input.primaryChartId,input.secondaryChartId)
  )
    throw ports.error(
      422,
      "BOOK_CHAIN_FORBIDDEN",
      "A secondary book cannot be another book's source.",
    );
  const existingRule = await ports.existingRule(input.secondaryChartId);
  if (existingRule && existingRule.primary_chart_id !== input.primaryChartId)
    throw ports.error(
      422,
      "BOOK_SOURCE_IMMUTABLE",
      "An active secondary book keeps its original primary book.",
    );
  if (
    !existingRule &&
    (await ports.targetPosted(input.secondaryChartId))
  )
    throw ports.error(
      422,
      "BOOK_TARGET_NOT_EMPTY",
      "Start the secondary book with a reviewed opening; existing posted ledgers cannot be attached.",
    );
  const period = await ports.openingPeriod(actor,input.openingPeriodId,input.effectiveDate);
  if (!period)
    throw ports.error(
      422,
      "BOOK_OPENING_PERIOD_REQUIRED",
      "The opening date must be in an open Company period.",
    );
  const accounts = (
    await ports.accounts(actor,input.primaryChartId,input.secondaryChartId));
  for (const m of input.mappings) {
    const src = accounts.find((a) => a.id === m.primaryAccountId),
      dst = accounts.find((a) => a.id === m.secondaryAccountId);
    if (
      !src ||
      !dst ||
      src.chart_id !== input.primaryChartId ||
      dst.chart_id !== input.secondaryChartId ||
      src.status !== "active" ||
      dst.status !== "active" ||
      src.category !== dst.category ||
      src.control_kind !== dst.control_kind ||
      src.currency !== dst.currency
    )
      throw ports.error(
        422,
        "BOOK_ACCOUNT_MAPPING_INVALID",
        "Map active Company accounts with the same category, currency and control kind.",
      );
    if (
      existingRule &&
      (await ports.mappingChanged(input.secondaryChartId,m.primaryAccountId,m.secondaryAccountId))
    )
      throw ports.error(
        422,
        "BOOK_CARRYING_MAPPING_CHANGED",
        "Keep existing account mappings; moving carrying balances requires a reviewed book transition.",
      );
  }
  if (
    accounts.some(
      (a) =>
        a.chart_id === input.primaryChartId &&
        a.status === "active" &&
        !input.mappings.some((m) => m.primaryAccountId === a.id),
    )
  )
    throw ports.error(
      422,
      "BOOK_ACCOUNT_MAPPING_REQUIRED",
      "Map every active source account before enabling the book.",
    );
  const rows = await ports.opening(
    actor.companyId,
    input.primaryChartId,
    input.effectiveDate,
  );
  if (
    rows.some((r) => !Number.isSafeInteger(r.minor)) ||
    rows.reduce((s, r) => s + BigInt(r.minor), BigInt(0)) !== BigInt(0)
  )
    throw ports.error(
      422,
      "BOOK_OPENING_UNBALANCED",
      "The source opening must balance within supported money precision.",
    );
  const id = ports.id(),
    now = ports.now(),
    openingHash = await ports.hash(JSON.stringify(rows));
  return ports.prepareCommit({actor,input,key,op,id,now,openingHash,rows});
}

async function decideBookRule(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof financeReviewSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = ports.permission(actor, "finance.approve");
  const input = financeReviewSchema.parse(raw);
  await ports.ready(actor);
  return ports.reviewTransaction(actor,async (repo) => {
    const op = `finance.book.${action}`,
      prior = await repo.replay(
        actor,
        key,
        op,
        id,
        input,
      );
    if (prior) return prior;
    const rule = await repo.rule(actor,id);
    if (!rule)
      throw ports.error(404, "BOOK_RULE_NOT_FOUND", "Book rule not found.");
    if (rule.status !== "draft" || rule.version !== input.expectedVersion)
      throw ports.error(
        409,
        "STALE_VERSION",
        "Reload this book rule before review.",
      );
    if (ports.samePerson(actor, rule.prepared_by))
      throw ports.error(
        403,
        "SELF_REVIEW_FORBIDDEN",
        "Another person must approve the book rule and opening.",
      );
    await repo.lockPrimary(rule.primary_chart_id);
    const now = ports.now(),
      rows = JSON.parse(rule.opening_json) as OpeningRow[];
    let createOpening=false;
    if (action === "approve") {
      if (
        (await ports.hash(
          JSON.stringify(
            await repo.opening(
              actor.companyId,
              rule.primary_chart_id,
              rule.effective_date,
            ),
          ),
        )) !== rule.opening_hash
      )
        throw ports.error(
          409,
          "STALE_BOOK_OPENING",
          "Source balances changed. Prepare a fresh opening for review.",
        );
      if (
        await repo.sourcePostedSince(rule.primary_chart_id,rule.effective_date)
      )
        throw ports.error(
          409,
          "BOOK_ACTIVATION_DATE_USED",
          "Choose an activation date after all existing source postings.",
        );
      const alreadyActive = await repo.alreadyActive(rule.secondary_chart_id);
      if (
        !alreadyActive &&
        (await repo.targetPosted(rule.secondary_chart_id))
      )
        throw ports.error(
          409,
          "BOOK_TARGET_NOT_EMPTY",
          "The target ledger changed. Prepare a new rule.",
        );
      createOpening=rows.length>0&&!alreadyActive;
    }
    return repo.decisionCommit({actor,id,input,action,key,op,rule,now,rows,createOpening});
  });
}

async function financeBookRecord(actor:WorkContext,id:string){
 const data=await financeBookWorkspace(actor),row=data.rules.find(rule=>rule.id===id);
 if(!row)throw ports.error(404,'BOOK_RULE_NOT_FOUND','Book rule not found in this Company.');
 return {row,data:{...data,rules:[row],mappings:data.mappings.filter(mapping=>mapping.rule_id===id)}};
}
return {financeBookWorkspace,createFinanceDimension,prepareBookRule,decideBookRule,financeBookRecord};}


