import {z} from 'zod';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import type {FinanceError} from '../domain/invoice-line-revision';
import {bookAdjustmentSchema,bookAdjustmentReversalSchema,type BookAdjustmentRow as Row,type BookAdjustmentSource,type BookAdjustmentConsole} from '../domain/book-adjustments';
import {financeReviewSchema} from '../domain/review';
type PrepareInput=z.infer<typeof bookAdjustmentSchema>;type ReviewInput=z.infer<typeof financeReviewSchema>;type ReversalInput=z.infer<typeof bookAdjustmentReversalSchema>;
export interface BookAdjustmentRepository{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;source(actor:WorkContext,primaryJournalId:string,chartId:string):Promise<Row|null>;accounts(actor:WorkContext,chartId:string,ids:string[]):Promise<Row[]>;validateJournal(actor:WorkContext,input:PrepareInput,id:string,currency:string,key:string):Promise<void>;prepareCommit(facts:{actor:WorkContext;input:PrepareInput;key:string;op:string;id:string;source:BookAdjustmentSource;createdAt:number;updatedAt:number}):Promise<Record<string,unknown>>;row(actor:WorkContext,id:string):Promise<Row|null>;preparePosting(actor:WorkContext,input:PrepareInput,id:string,currency:string,key:string):Promise<string>;decisionCommit(facts:{actor:WorkContext;id:string;input:ReviewInput;key:string;op:string;status:string;version:number;journalId:string|null;updatedAt:number;evidenceId:string;inputHash:string;evidenceAt:number}):Promise<Record<string,unknown>>;rowForReversal(actor:WorkContext,id:string):Promise<Row|null>;prepareReversal(actor:WorkContext,journalId:string,id:string,input:ReversalInput,key:string):Promise<string>;reverseCommit(facts:{actor:WorkContext;id:string;input:ReversalInput;key:string;op:string;reversalId:string;updatedAt:number}):Promise<Record<string,unknown>>;}
export interface BookAdjustmentPorts{error:FinanceError;permission(actor:WorkContext,permission:string):WorkContext;samePerson(actor:WorkContext,userId:string|number|null):boolean;now():number;id():string;hash(value:string):Promise<string>;ready(actor:WorkContext):Promise<void>;transaction<T>(actor:WorkContext,run:(repo:BookAdjustmentRepository)=>Promise<T>):Promise<T>;workspace(actor:WorkContext):Promise<Row[]>;console(actor:WorkContext):Promise<BookAdjustmentConsole>;}
export function createBookAdjustments(ports:BookAdjustmentPorts){
async function evidence(
  repo:BookAdjustmentRepository,
  actor: WorkContext,
  input: z.infer<typeof bookAdjustmentSchema>,
) {
  const source = await repo.source(actor,input.primaryJournalId,input.chartId);
  if (!source)
    throw ports.error(
      409,
      "BOOK_SOURCE_REQUIRED",
      "Choose a currently posted source projected into this secondary book.",
    );
  if (input.entryDate < String(source.entry_date))
    throw ports.error(
      422,
      "BOOK_ADJUSTMENT_DATE",
      "Adjust on or after the source posting date.",
    );
  const ids = [...new Set(input.lines.map((l) => l.accountId))],
    accounts = (
      await repo.accounts(actor,input.chartId,ids));
  if (
    accounts.length !== ids.length ||
    accounts.some((a) => a.currency !== source.base_currency)
  )
    throw ports.error(
      422,
      "BOOK_ADJUSTMENT_ACCOUNT_INVALID",
      "Use active noncontrol accounts in the secondary functional currency. Control differences require a dedicated subledger plan.",
    );
  return {
    source,
    accounts,
    hash: await ports.hash(JSON.stringify({ input, source, accounts })),
  };
}

async function prepareBookAdjustment(
  actor: WorkContext,
  raw: z.input<typeof bookAdjustmentSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = bookAdjustmentSchema.parse(raw),
    id = `book-adjustment-${key}`,
    op = "finance.book-adjustment.prepare";
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    const source = await evidence(repo, actor, input);
    // Preparation validates balance, period and money evidence without persisting a journal.
    await repo.validateJournal(actor,input,id,String(source.source.base_currency),key);
    const createdAt=ports.now(),updatedAt=ports.now();
    return repo.prepareCommit({actor,input,key,op,id,source,createdAt,updatedAt});
  });
}

async function decideBookAdjustment(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof financeReviewSchema>,
  action: "post" | "reject",
  key: string,
) {
  actor = ports.permission(actor, "finance.approve");
  const input = financeReviewSchema.parse(raw),
    op = `finance.book-adjustment.${action}`;
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    const row = await repo.row(actor,id);
    if (!row)
      throw ports.error(
        404,
        "BOOK_ADJUSTMENT_NOT_FOUND",
        "Book adjustment not found.",
      );
    if (row.status !== "draft" || row.version !== input.expectedVersion)
      throw ports.error(
        409,
        "BOOK_ADJUSTMENT_CHANGED",
        "Refresh the draft before deciding.",
      );
    if (ports.samePerson(actor, row.prepared_by))
      throw ports.error(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must decide the book adjustment.",
      );
    const content = bookAdjustmentSchema.parse(
        JSON.parse(String(row.content_json)),
      ),
      source = action === "post" ? await evidence(repo, actor, content) : null;
    if (source && source.hash !== row.source_hash)
      throw ports.error(
        409,
        "BOOK_ADJUSTMENT_SOURCE_CHANGED",
        "Source or account evidence changed. Reject and prepare a fresh adjustment.",
      );
    const journalId=source?await repo.preparePosting(actor,content,id,String(source.source.base_currency),key):null,
      status = action === "post" ? "posted" : "rejected",
      version = input.expectedVersion + 1;
    const updatedAt=ports.now(),evidenceId=ports.id(),inputHash=await ports.hash(JSON.stringify(input)),evidenceAt=ports.now();
    return repo.decisionCommit({actor,id,input,key,op,status,version,journalId,updatedAt,evidenceId,inputHash,evidenceAt});
  });
}

async function reverseBookAdjustment(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof bookAdjustmentReversalSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.approve");
  const input = bookAdjustmentReversalSchema.parse(raw),
    op = "finance.book-adjustment.reverse";
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    const row = await repo.rowForReversal(actor,id);
    if (!row)
      throw ports.error(
        404,
        "BOOK_ADJUSTMENT_NOT_FOUND",
        "Book adjustment not found.",
      );
    if (row.status !== "posted" || row.version !== input.expectedVersion)
      throw ports.error(
        409,
        "BOOK_ADJUSTMENT_CHANGED",
        "Refresh the posted adjustment before reversing it.",
      );
    if (ports.samePerson(actor, row.prepared_by))
      throw ports.error(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must reverse the book adjustment.",
      );
    const reversalId=await repo.prepareReversal(actor,String(row.journal_id),id,input,key),updatedAt=ports.now();
    return repo.reverseCommit({actor,id,input,key,op,reversalId,updatedAt});
  });
}

async function bookAdjustmentWorkspace(actor: WorkContext) {
  actor = ports.permission(actor, "finance.read");
  await ports.ready(actor);
  return ports.workspace(actor);
}

async function bookAdjustmentConsoleWorkspace(actor: WorkContext) {
  actor = ports.permission(actor, "finance.read");
  await ports.ready(actor);
  return ports.console(actor);
}
return {prepareBookAdjustment,decideBookAdjustment,reverseBookAdjustment,bookAdjustmentWorkspace,bookAdjustmentConsoleWorkspace};}


