import {z} from 'zod';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import {financeCsvRows} from "@/lib/finance/finance-csv";
import {minorToDecimal} from "@/lib/finance/finance-money";
import {financeReviewSchema} from '../domain/review';
import {bankCsvMappingSchema,bankCsvImportSchema,bankMatchGroupSchema,type BankRow,type LedgerRow,type BankAutomationQuery,type BankSourceFacts,type BankMatchGroup,type BankMatchAllocation,type BankCandidate,type BankPickerPage} from '../domain/bank-automation';
import type {BankImportInput} from './bank';
import type {FinanceError} from '../domain/invoice-line-revision';
type MappingInput=z.infer<typeof bankCsvMappingSchema>;type ImportInput=z.infer<typeof bankCsvImportSchema>;type GroupInput=z.infer<typeof bankMatchGroupSchema>;type ReviewInput=z.infer<typeof financeReviewSchema>;
export interface BankAutomationPorts{error:FinanceError;permission(actor:WorkContext,permission:'finance.read'|'finance.write'|'finance.approve'):WorkContext;samePerson(actor:WorkContext,userId:string):boolean;ready(actor:WorkContext):Promise<void>;now():number;id():string;hash(value:string):Promise<string>;tool(actor:WorkContext):Promise<void>;replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,input:unknown):Promise<Result|null>;reviewReplay(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;mappedTransaction<T>(run:()=>Promise<T>):Promise<T>;reviewTransaction<T>(run:()=>Promise<T>):Promise<T>;mappingCommit(facts:{actor:WorkContext;key:string;op:string;input:MappingInput;id:string}):Promise<Record<string,unknown>>;mapping(actor:WorkContext,id:string):Promise<{columns_json:string}|null>;account(actor:WorkContext,id:string):Promise<{currency:string}|null>;importedFile(actor:WorkContext,account:string,hash:string):Promise<{statement_number:string}|null>;importStatement(actor:WorkContext,input:BankImportInput,key:string):Promise<Record<string,unknown>>;importCommit(facts:{actor:WorkContext;key:string;op:string;input:ImportInput;statement:Record<string,unknown>;fileHash:string}):Promise<Record<string,unknown>>;bank(actor:WorkContext,id:string):Promise<BankRow|null>;line(actor:WorkContext,id:string):Promise<LedgerRow|null>;proposeCommit(facts:{actor:WorkContext;key:string;op:string;input:GroupInput;id:string;source:BankSourceFacts;now:number;allocations:BankMatchAllocation[]}):Promise<Record<string,unknown>>;group(actor:WorkContext,id:string):Promise<BankMatchGroup|null>;decisionCommit(facts:{actor:WorkContext;id:string;input:ReviewInput;key:string;op:string;group:BankMatchGroup;prepared:{input:GroupInput};current:BankSourceFacts|null;action:'confirm'|'reverse'|'reject';status:string;now:number}):Promise<Record<string,unknown>>;picker(actor:WorkContext,query:BankAutomationQuery,chart:string,search:string,cashSearch:string):Promise<{mappings:Record<string,unknown>[];groups:BankPickerPage<Record<string,unknown>&{id:string;page_key:string}>;rows:BankPickerPage<BankRow&{page_key:string}>;targets:BankPickerPage<LedgerRow&{page_key:string}>}>;candidates(actor:WorkContext,chart:string,ids:string[]):Promise<BankCandidate[]>;suggestionTargets(actor:WorkContext,chart:string,ids:string[]):Promise<LedgerRow[]>;}
export function createBankAutomation(ports:BankAutomationPorts){
async function createBankCsvMapping(
  actor: WorkContext,
  raw: z.input<typeof bankCsvMappingSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = bankCsvMappingSchema.parse(raw);
  await ports.ready(actor);
  const op = "finance.bank-mapping.create",
    prior = await ports.replay(actor, key, op, input);
  if (prior) return prior;
  await ports.tool(actor);
  const id = ports.id();
  return ports.mappingCommit({actor,key,op,input,id});
}

async function importMappedBankCsv(
  actor: WorkContext,
  raw: z.input<typeof bankCsvImportSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = bankCsvImportSchema.parse(raw);
  await ports.ready(actor);
  return ports.mappedTransaction(async () => {
    const op = "finance.bank-csv.import",
      prior = await ports.replay(actor, key, op, input);
    if (prior) return prior;
    const saved = await ports.mapping(actor,input.mappingId);
    if (!saved)
      throw ports.error(
        404,
        "BANK_MAPPING_NOT_FOUND",
        "Saved CSV mapping not found.",
      );
    await ports.tool(actor);
    const mapping = bankCsvMappingSchema.parse(JSON.parse(saved.columns_json)),
      account = await ports.account(actor,input.bankAccountId);
    if (!account)
      throw ports.error(
        404,
        "BANK_ACCOUNT_NOT_FOUND",
        "Bank account not found.",
      );
    const fileHash = await ports.hash(
      input.csv.replace(/^\uFEFF/, "").replace(/\r\n/g, "\n"),
    );
    const imported=await ports.importedFile(actor,input.bankAccountId,fileHash);
    if(imported)throw ports.error(409,"BANK_CSV_ALREADY_IMPORTED",`This CSV was already imported as ${imported.statement_number}. Review the existing statement before importing another file.`);
    let parsed: Record<string, string>[];
    try {
      parsed = financeCsvRows(input.csv);
    } catch (e) {
      throw ports.error(
        422,
        "INVALID_CSV",
        e instanceof Error ? e.message : "Invalid CSV",
      );
    }
    const rows = parsed.map((r) => {
      const date = r[mapping.columns.transactionDate],
        transactionDate =
          mapping.dateFormat === "DD/MM/YYYY"
            ? date?.split("/").reverse().join("-")
            : date;
      return z
        .object({
          externalId: z.string().min(1).max(160),
          transactionDate: z.string().date(),
          description: z.string().max(2000),
          amount: z.string(),
          currency: z.string().length(3),
        })
        .parse({
          externalId: r[mapping.columns.externalId],
          transactionDate,
          description: mapping.columns.description
            ? r[mapping.columns.description]
            : "",
          amount: r[mapping.columns.amount],
          currency: mapping.columns.currency
            ? r[mapping.columns.currency]?.toUpperCase()
            : account.currency,
        });
    });
    // Explicitly pass the owned transaction to the existing import command.
    const statement = await ports.importStatement(
      actor,
      {
        bankAccount: input.bankAccountId,
        statementNumber: input.statementNumber,
        statementDate: input.statementDate,
        opening: input.opening,
        closing: input.closing,
        closingFxRate: input.closingFxRate,
        sourceHash: fileHash,
        rows,
      },
      `${key}-statement`,
    );
    return ports.importCommit({actor,key,op,input,statement,fileHash});
  });
}

async function sources(
    actor: WorkContext,
  input: z.infer<typeof bankMatchGroupSchema>,
) {
  const banks: BankRow[] = [],
    lines: LedgerRow[] = [];
  for (const id of [...input.bankTransactionIds].sort()) {
    const r = await ports.bank(actor,id);
    if (
      !r ||
      r.match_status === "matched" ||
      r.match_status === "ignored" ||
      r.amount_minor === 0
    )
      throw ports.error(
        409,
        "BANK_ROW_UNAVAILABLE",
        "Select unmatched nonzero bank movements.",
      );
    banks.push(r);
  }
  for (const id of [...input.journalLineIds].sort()) {
    const r = await ports.line(actor,id);
    if (!r)
      throw ports.error(
        409,
        "BANK_TARGET_UNAVAILABLE",
        "Select unmatched posted primary-book cash lines.",
      );
    lines.push(r);
  }
  const currency = banks[0].currency,
    account = banks[0].bank_account,
    direction = Math.sign(banks[0].amount_minor);
  if (
    banks.some(
      (r) =>
        r.currency !== currency ||
        r.bank_account !== account ||
        Math.sign(r.amount_minor) !== direction,
    ) ||
    lines.some(
      (l) =>
        l.currency !== currency ||
        l.account_id !== account ||
        Math.sign(l.debit_minor - l.credit_minor) !== direction,
    )
  )
    throw ports.error(
      422,
      "BANK_MATCH_SCOPE",
      "Match the same bank account, currency and movement direction.",
    );
  const bankTotal = banks.reduce(
      (s, r) => s + BigInt(r.amount_minor),
      BigInt(0),
    ),
    ledgerTotal = lines.reduce(
      (s, l) => s + BigInt(l.debit_minor) - BigInt(l.credit_minor),
      BigInt(0),
    );
  if (bankTotal !== ledgerTotal)
    throw ports.error(
      422,
      "BANK_MATCH_DIFFERENCE",
      "The selected movements and posted cash lines must total exactly the same amount.",
    );
  return {
    banks,
    lines,
    currency,
    account,
    hash: await ports.hash(JSON.stringify({ banks, lines })),
  };
}

async function proposeBankMatchGroup(
  actor: WorkContext,
  raw: z.input<typeof bankMatchGroupSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = bankMatchGroupSchema.parse(raw);
  await ports.ready(actor);
  const op = "finance.bank-group.propose",
    prior = await ports.replay(actor, key, op, input);
  if (prior) return prior;
  await ports.tool(actor);
  const source = await sources(actor, input),
    id = ports.id(),
    now = ports.now(),
    allocations = [];
  let targetIndex = 0,
    targetRemaining = Math.abs(
      source.lines[0].debit_minor - source.lines[0].credit_minor,
    );
  for (const bank of source.banks) {
    let remaining = Math.abs(bank.amount_minor);
    while (remaining) {
      const amount = Math.min(remaining, targetRemaining),
        line = source.lines[targetIndex];
      allocations.push(
                {id:ports.id(),bankId:bank.id,lineId:line.id,amount},
      );
      remaining -= amount;
      targetRemaining -= amount;
      if (targetRemaining === 0 && targetIndex < source.lines.length - 1) {
        targetIndex++;
        targetRemaining = Math.abs(
          source.lines[targetIndex].debit_minor -
            source.lines[targetIndex].credit_minor,
        );
      }
    }
  }
  return ports.proposeCommit({actor,key,op,input,id,source,now,allocations});
}

async function decideBankMatchGroup(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof financeReviewSchema>,
  action: "confirm" | "reverse" | "reject",
  key: string,
) {
  actor = ports.permission(actor, "finance.approve");
  const input = financeReviewSchema.parse(raw);
  await ports.ready(actor);
  return ports.reviewTransaction(async () => {
    const op = `finance.bank-group.${action}`,
      prior = await ports.reviewReplay(actor,key,op,id,input);
    if (prior) return prior;
    const group = await ports.group(actor,id);
    if (!group)
      throw ports.error(
        404,
        "BANK_GROUP_NOT_FOUND",
        "Bank match group not found.",
      );
    if (
      group.version !== input.expectedVersion ||
      group.status !== (action === "reverse" ? "matched" : "proposed")
    )
      throw ports.error(
        409,
        "STALE_VERSION",
        "Reload this match group before review.",
      );
    if (action !== "reverse" && ports.samePerson(actor, group.proposed_by))
      throw ports.error(
        403,
        "SELF_REVIEW_FORBIDDEN",
        "Another person must review this match group.",
      );
    const prepared = JSON.parse(group.explanation_json) as {
      input: z.infer<typeof bankMatchGroupSchema>;
    };
    if (!bankMatchGroupSchema.safeParse(prepared.input).success)
      throw ports.error(
        409,
        "BANK_GROUP_MIGRATION_REQUIRED",
        "This legacy proposal needs a fresh source review before use.",
      );
    const now=ports.now();let current:BankSourceFacts|null=null;
    if(action==='confirm'){current=await sources(actor,prepared.input);if(current.hash!==group.source_hash)throw ports.error(409,'STALE_BANK_EVIDENCE','Bank or journal evidence changed. Prepare a fresh proposal.');}
    const status=action==='confirm'?'matched':action==='reverse'?'reversed':'rejected';
    return ports.decisionCommit({actor,id,input,key,op,group,prepared,current,action,status,now});
  });
}

async function bankAutomationWorkspace(
  actor: WorkContext,
  query: BankAutomationQuery = {},
) {
  actor = ports.permission(actor, "finance.read");
  await ports.ready(actor);
  const chart = z
    .string()
    .max(180)
    .parse(query.chart ?? "");
  const search = z
    .string()
    .max(200)
    .parse(query.q ?? "");
  const cashSearch = z
    .string()
    .max(200)
    .parse(query.cashQuery ?? "");
  const {mappings,groups,rows,targets}=await ports.picker(actor,query,chart,search,cashSearch);
  // Aggregate every eligible source, independently of the cash picker page.
  // Otherwise a second exact match beyond that page falsely appears unique.
  const candidates = rows.items.length
    ? (
        await ports.candidates(actor,chart,rows.items.map(row=>row.id))
      )
    : [];
  const suggestions = rows.items.map((b) => {
    const candidate = candidates.find((row) => row.id === b.id);
    if (candidate?.exact_count === 1 && candidate.exact_id)
      return {
        bankTransactionIds: [b.id],
        journalLineIds: [candidate.exact_id],
        confidence: "amount_date",
        amount: minorToDecimal(b.amount_minor),
      };
    if (
      candidate &&
      candidate.same_day_count > 1 &&
      candidate.same_day_count <= 100 &&
      BigInt(candidate.same_day_amount) === BigInt(b.amount_minor)
    )
      return {
        bankTransactionIds: [b.id],
        journalLineIds: JSON.parse(candidate.same_day_ids) as string[],
        confidence: "same_day_total",
        amount: minorToDecimal(b.amount_minor),
      };
    return {
      bankTransactionIds: [b.id],
      journalLineIds: [],
      confidence: "needs_review",
      amount: minorToDecimal(b.amount_minor),
    };
  });
  const suggestedIds = [
    ...new Set(suggestions.flatMap((row) => row.journalLineIds)),
  ];
  const suggestionTargets = suggestedIds.length
    ? (
        await ports.suggestionTargets(actor,chart,suggestedIds)
      )
    : [];
  return {
    mappings: mappings,
    groups: groups.items,
    suggestions,
    suggestionTargets,
    rows: rows.items,
    targets: targets.items,
    pages: {
      bank: {
        nextCursor: rows.nextCursor,
        previousCursor: rows.previousCursor,
      },
      cash: {
        nextCursor: targets.nextCursor,
        previousCursor: targets.previousCursor,
      },
      groups: {
        nextCursor: groups.nextCursor,
        previousCursor: groups.previousCursor,
      },
    },
  };
}
return {createBankCsvMapping,importMappedBankCsv,proposeBankMatchGroup,decideBankMatchGroup,bankAutomationWorkspace};}


