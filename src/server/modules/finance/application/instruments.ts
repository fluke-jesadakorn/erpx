import {z} from 'zod';
import {instrumentSchedule,halfAway,type InstrumentInput} from '../domain/finance-instrument-policy';
import {decimalToUnits,unitsToDecimal} from '@/lib/finance-money';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import type {FinanceError} from '../domain/invoice-line-revision';
import type {JournalInput,JournalLineInput} from '../domain/finance';
import {instrumentEditionSchema,type InstrumentRow as Row,type InstrumentSchedule as Schedule,type InstrumentEditionInput,type InstrumentEditionEvidence,type InstrumentRunEvidence,type InstrumentWorkspace} from '../domain/instruments';
import {instrumentDecisionSchema,instrumentReversalSchema} from '../domain/instrument-decisions';
import {instrumentRunSchema} from '../domain/due-inputs';
type Decision=z.infer<typeof instrumentDecisionSchema>;type Reversal=z.infer<typeof instrumentReversalSchema>;type RunInput=z.infer<typeof instrumentRunSchema>;
export interface InstrumentRepository{edition(actor:WorkContext,id:string):Promise<Row|null>;moneyScale(companyId:string,currency:string):Promise<number>;tool(companyId:string):Promise<void>;chart(actor:WorkContext,chartId:string):Promise<Row|null>;secondary(actor:WorkContext,chartId:string):Promise<object|null>;accounts(actor:WorkContext,chartId:string,ids:string[]):Promise<Row[]>;bank(actor:WorkContext,id:string):Promise<Row|null>;history(id:string):Promise<Row[]>;pendingRuns(id:string):Promise<object|null>;replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;prepareJournal(actor:WorkContext,input:JournalInput,key:string,status?:'draft'|'posted'):Promise<string>;prepareEditionCommit(facts:{actor:WorkContext;input:InstrumentEditionInput;key:string;op:string;id:string;e:InstrumentEditionEvidence;now:number}):Promise<Record<string,unknown>>;decideEditionCommit(facts:{actor:WorkContext;id:string;input:Decision;action:'approve'|'reject';key:string;op:string;e:InstrumentEditionEvidence|null;journalId:string|null;status:string;parentUpdatedAt:number|null;updatedAt:number}):Promise<Record<string,unknown>>;prepareRunCommit(facts:{actor:WorkContext;id:string;input:RunInput;key:string;op:string;runId:string;e:InstrumentRunEvidence;now:number}):Promise<Record<string,unknown>>;run(actor:WorkContext,id:string):Promise<Row|null>;decideRunCommit(facts:{actor:WorkContext;id:string;input:Decision;action:'post'|'reject';key:string;op:string;row:Row;run:Row;journalId:string|null;status:string;agreementStatus:string;agreementUpdatedAt:number|null;updatedAt:number}):Promise<Record<string,unknown>>;laterRuns(id:string,ordinal:number):Promise<object|null>;laterEdition(id:string):Promise<object|null>;prepareReversal(actor:WorkContext,journalId:string,sourceType:'instrument_payment',id:string,input:Reversal,key:string):Promise<string>;reverseRunCommit(facts:{actor:WorkContext;id:string;input:Reversal;key:string;op:string;row:Row;run:Row;reversalId:string;now:number}):Promise<Record<string,unknown>>;}
export interface FinanceInstrumentPorts{error:FinanceError;permission(actor:WorkContext,permission:string):WorkContext;samePerson(actor:WorkContext,userId:string|number|null):boolean;now():number;hash(value:string):Promise<string>;transaction<T>(actor:WorkContext,run:(repo:InstrumentRepository)=>Promise<T>):Promise<T>;ownedTransaction<T>(actor:WorkContext,run:(repo:InstrumentRepository)=>Promise<T>):Promise<T>;workspace(actor:WorkContext):Promise<InstrumentWorkspace>;}
export function createFinanceInstruments(ports:FinanceInstrumentPorts){
async function get(repo:InstrumentRepository, actor: WorkContext, id: string) {
  const row = await repo.edition(actor,id);
  if (!row)
    throw ports.error(
      404,
      "INSTRUMENT_NOT_FOUND",
      "Financing agreement not found.",
    );
  return row;
}

function terms(row: Row) {
  return instrumentEditionSchema.parse(JSON.parse(String(row.content_json)));
}

function schedule(row: Row) {
  return JSON.parse(String(row.schedule_json)) as Schedule;
}

async function configuration(
  repo:InstrumentRepository,
  actor: WorkContext,
  input: InstrumentInput,
) {
  await repo.moneyScale(actor.companyId,input.currency);
  const chart = await repo.chart(actor,input.chartId);
  if (
    !chart ||
    chart.base_currency !== input.currency ||
    (await repo.secondary(actor,input.chartId))
  )
    throw ports.error(
      422,
      "INSTRUMENT_PRIMARY_BOOK_REQUIRED",
      "Use the active primary book and its functional currency.",
    );
  const ids = [
    input.bankAccountId,
    input.liabilityAccountId,
    input.interestAccountId,
    ...(input.kind === "lease"
      ? [
          input.rightOfUseAccountId!,
          input.accumulatedAccountId!,
          input.depreciationAccountId!,
        ]
      : []),
  ];
  if (new Set(ids).size !== ids.length)
    throw ports.error(
      422,
      "INSTRUMENT_ACCOUNTS_REQUIRED",
      "Financing mappings must use distinct accounts.",
    );
  const accounts = (
      await repo.accounts(actor,input.chartId,ids)),
    byId = new Map(accounts.map((a) => [a.id, a]));
  if (
    accounts.length !== ids.length ||
    accounts.some((a) => a.currency !== input.currency) ||
    byId.get(input.bankAccountId)?.category !== "asset" ||
    byId.get(input.bankAccountId)?.control_kind !== "bank" ||
    byId.get(input.liabilityAccountId)?.category !== "liability" ||
    byId.get(input.liabilityAccountId)?.control_kind !== "none" ||
    byId.get(input.interestAccountId)?.category !== "expense" ||
    byId.get(input.interestAccountId)?.control_kind !== "none" ||
    (input.kind === "lease" &&
      [
        input.rightOfUseAccountId!,
        input.accumulatedAccountId!,
        input.depreciationAccountId!,
      ].some(
        (id, i) =>
          byId.get(id)?.category !== (i === 2 ? "expense" : "asset") ||
          byId.get(id)?.control_kind !== "none",
      ))
  )
    throw ports.error(
      422,
      "INSTRUMENT_ACCOUNTS_REQUIRED",
      "Map same-book bank, liability and expense accounts, with separate noncontrol right-of-use accounts for a lease.",
    );
  return { chart, accounts };
}

async function bankEvidence(
  repo:InstrumentRepository,
  actor: WorkContext,
  input: InstrumentInput,
  bankId: string | undefined,
  version: number | undefined,
  signed: bigint,
  date: string,
) {
  if (!bankId || !version)
    throw ports.error(
      422,
      "INSTRUMENT_BANK_EVIDENCE_REQUIRED",
      "Select an exact imported bank movement and its version.",
    );
  const bank = await repo.bank(actor,bankId);
  if (
    !bank ||
    bank.version !== version ||
    bank.match_status !== "unmatched" ||
    bank.bank_account !== input.bankAccountId ||
    bank.currency !== input.currency ||
    BigInt(bank.amount_minor!) !== signed ||
    bank.transaction_date !== date
  )
    throw ports.error(
      409,
      "INSTRUMENT_BANK_EVIDENCE_CHANGED",
      "Select an unmatched movement with the exact account, currency, date and cash amount.",
    );
  return bank;
}

async function history(repo:InstrumentRepository, row: Row) {
  const rows = (
    await repo.history(String(row.id)));
  if (rows.some((r, i) => r.ordinal !== i + 1 || r.journal_status !== "posted"))
    throw ports.error(
      409,
      "INSTRUMENT_HISTORY_CHANGED",
      "Reconcile the retained financing history before preparing more work.",
    );
  return rows;
}

function carrying(row: Row, rows: Row[]) {
  const s = schedule(row),
    last = rows.at(-1),
    result = last
      ? (JSON.parse(String(last.result_json)) as Schedule["rows"][number])
      : null;
  return {
    liability: decimalToUnits(result?.closing ?? s.principal),
    rightOfUse:
      decimalToUnits(s.rightOfUse ?? s.principal) -
      rows.reduce(
        (sum, r) =>
          sum + decimalToUnits(JSON.parse(String(r.result_json)).depreciation),
        BigInt(0),
      ),
    lastDate: last ? String(last.entry_date) : terms(row).startDate,
  };
}

async function editionEvidence(
  repo:InstrumentRepository,
  actor: WorkContext,
  input: z.infer<typeof instrumentEditionSchema>,
) {
  const config = await configuration(repo, actor, input),
    calculation = instrumentSchedule(input),
    parent = input.parentId ? await get(repo, actor, input.parentId) : null;
  let priorRows: Row[] = [],
    adjustment = decimalToUnits(calculation.principal);
  if (parent) {
    if (
      input.kind !== "lease" ||
      parent.kind !== "lease" ||
      parent.status !== "active" ||
      parent.version !== input.parentVersion
    )
      throw ports.error(
        409,
        "LEASE_AMENDMENT_CHANGED",
        "Amend the current active lease edition using its current version.",
      );
    const old = terms(parent);
    if (
      [
        "number",
        "chartId",
        "currency",
        "bankAccountId",
        "liabilityAccountId",
        "rightOfUseAccountId",
        "accumulatedAccountId",
        "depreciationAccountId",
        "interestAccountId",
      ].some(
        (k) => old[k as keyof typeof old] !== input[k as keyof typeof input],
      )
    )
      throw ports.error(
        422,
        "LEASE_AMENDMENT_MAPPING",
        "Retain the original book and accounts when remeasuring a lease.",
      );
    priorRows = await history(repo, parent);
    const c = carrying(parent, priorRows);
    if (input.startDate !== c.lastDate || c.liability <= BigInt(0))
      throw ports.error(
        422,
        "LEASE_AMENDMENT_BOUNDARY",
        "Remeasure at the last settled boundary, before preparing its next payment.",
      );
    if (
      await repo.pendingRuns(String(parent.id))
    )
      throw ports.error(
        409,
        "LEASE_PAYMENT_PENDING",
        "Decide existing payment drafts before remeasurement.",
      );
    adjustment = decimalToUnits(calculation.principal) - c.liability;
    const rightOfUse = c.rightOfUse + adjustment;
    if (rightOfUse < BigInt(0))
      throw ports.error(
        422,
        "LEASE_REMEASUREMENT_NEGATIVE_ASSET",
        "The revised liability cannot reduce the right-of-use carrying amount below zero under this policy.",
      );
    // Remeasurement changes liability by PV difference; ROU retains its existing carrying amount plus that difference.
    let previous = BigInt(0);
    for (const r of calculation.rows) {
      const target = (rightOfUse * BigInt(r.ordinal)) / BigInt(input.months);
      r.depreciation = unitsToDecimal(target - previous);
      previous = target;
    }
    Object.assign(calculation, { rightOfUse: unitsToDecimal(rightOfUse) });
  }
  const bank =
    input.kind === "loan"
      ? await bankEvidence(
          repo,
          actor,
          input,
          input.bankTransactionId,
          input.bankVersion,
          decimalToUnits(calculation.principal),
          input.startDate,
        )
      : null;
  const hash = await ports.hash(
    JSON.stringify({ input, config, parent, priorRows, bank, calculation }),
  );
  return { calculation, parent, adjustment, bank, hash };
}

function line(
  accountId: string,
  amount: bigint,
  debit: boolean,
  dimensions: Record<string, string>,
): JournalLineInput {
  return {
    accountId,
    debit: unitsToDecimal(debit ? amount : BigInt(0)),
    credit: unitsToDecimal(debit ? BigInt(0) : amount),
    dimensions,
  };
}

async function activationJournal(
  repo:InstrumentRepository,
  actor: WorkContext,
  id: string,
  input: z.infer<typeof instrumentEditionSchema>,
  e: Awaited<ReturnType<typeof editionEvidence>>,
  key: string,
  status: "draft" | "posted",
) {
  if (e.adjustment === BigInt(0)) return null;
  const positive = e.adjustment > BigInt(0),
    amount = positive ? e.adjustment : -e.adjustment;
  return repo.prepareJournal(
    actor,
    {
      chartId: input.chartId,
      periodId: input.periodId,
      entryDate: input.startDate,
      currency: input.currency,
      memo: input.reason,
      sourceType: "instrument_activation",
      sourceId: id,
      isOpening: false,
      lines: [
        line(
          input.kind === "lease"
            ? input.rightOfUseAccountId!
            : input.bankAccountId,
          amount,
          positive,
          input.dimensions,
        ),
        line(input.liabilityAccountId, amount, !positive, input.dimensions),
      ],
    },
    key,
    status,
  );
}

async function prepareInstrument(
  actor: WorkContext,
  raw: z.input<typeof instrumentEditionSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = instrumentEditionSchema.parse(raw),
    id = `instrument-${key}`,
    op = "finance.instrument.prepare";
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    await repo.tool(actor.companyId);
    const e = await editionEvidence(repo, actor, input);
    await activationJournal(repo, actor, id, input, e, key, "draft");
    const now = ports.now();
    return repo.prepareEditionCommit({actor,input,key,op,id,e,now});
  });
}

async function decideInstrument(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof instrumentDecisionSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = ports.permission(actor, "finance.approve");
  const input = instrumentDecisionSchema.parse(raw),
    op = `finance.instrument.${action}`;
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    const row = await get(repo, actor, id);
    if (row.status !== "draft" || row.version !== input.expectedVersion)
      throw ports.error(
        409,
        "INSTRUMENT_CHANGED",
        "Refresh the financing draft before deciding.",
      );
    if (ports.samePerson(actor, row.prepared_by))
      throw ports.error(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review the financing terms.",
      );
    const content = terms(row),
      e =
        action === "approve" ? await editionEvidence(repo, actor, content) : null;
    if (e && e.hash !== row.source_hash)
      throw ports.error(
        409,
        "INSTRUMENT_SOURCE_CHANGED",
        "Financing terms, bank evidence or mappings changed. Reject and prepare again.",
      );
    const journalId = e
        ? await activationJournal(repo, actor, id, content, e, key, "posted")
        : null;
    const status=action==='approve'?'active':'rejected',parentUpdatedAt=e?.parent?ports.now():null,updatedAt=ports.now();
    return repo.decideEditionCommit({actor,id,input,action,key,op,e,journalId,status,parentUpdatedAt,updatedAt});
  });
}

async function runEvidence(
  repo:InstrumentRepository,
  actor: WorkContext,
  row: Row,
  input: z.infer<typeof instrumentRunSchema>,
) {
  if (row.status !== "active" || row.version !== input.expectedVersion)
    throw ports.error(
      409,
      "INSTRUMENT_CHANGED",
      "Use the current active agreement version.",
    );
  const content = terms(row),
    config = await configuration(repo, actor, content),
    rows = await history(repo, row),
    s = schedule(row),
    ordinal = rows.length + 1;
  if (ordinal > s.rows.length)
    throw ports.error(
      409,
      "INSTRUMENT_COMPLETE",
      "The financing schedule is complete.",
    );
  let result = { ...s.rows[ordinal - 1] };
  if (input.kind === "payoff") {
    if (content.kind !== "loan")
      throw ports.error(
        422,
        "LOAN_PAYOFF_REQUIRED",
        "This payoff command is available for loans.",
      );
    const c = carrying(row, rows);
    if (input.entryDate < c.lastDate || input.entryDate > result.dueDate)
      throw ports.error(
        422,
        "LOAN_PAYOFF_DATE",
        "Settle past due instalments first; payoff must fall between the last settled date and next due date.",
      );
    const days = BigInt(
        Math.round(
          (Date.parse(input.entryDate) - Date.parse(c.lastDate)) / 86400000,
        ),
      ),
      interest = halfAway(
        c.liability * BigInt(content.annualRateBps) * days,
        BigInt(3650000),
      );
    result = {
      ...result,
      dueDate: input.entryDate,
      opening: unitsToDecimal(c.liability),
      payment: unitsToDecimal(c.liability + interest),
      interest: unitsToDecimal(interest),
      principal: unitsToDecimal(c.liability),
      closing: "0.00",
      depreciation: "0.00",
    };
  } else if (input.entryDate < result.dueDate)
    throw ports.error(
      422,
      "INSTRUMENT_NOT_DUE",
      "Post a scheduled instalment on or after its due date.",
    );
  if (rows.at(-1) && input.entryDate < String(rows.at(-1)!.entry_date))
    throw ports.error(
      422,
      "INSTRUMENT_SEQUENCE",
      "Payments must follow their retained accounting dates.",
    );
  const bank = await bankEvidence(
      repo,
      actor,
      content,
      input.bankTransactionId,
      input.bankVersion,
      -decimalToUnits(result.payment),
      input.entryDate,
    ),
    hash = await ports.hash(
      JSON.stringify({ row, config, rows, input, result, bank }),
    );
  const lines = [
    line(
      content.liabilityAccountId,
      decimalToUnits(result.principal),
      true,
      content.dimensions,
    ),
    line(
      content.bankAccountId,
      decimalToUnits(result.payment),
      false,
      content.dimensions,
    ),
  ];
  if (decimalToUnits(result.interest) > BigInt(0))
    lines.push(
      line(
        content.interestAccountId,
        decimalToUnits(result.interest),
        true,
        content.dimensions,
      ),
    );
  if (decimalToUnits(result.depreciation) > BigInt(0))
    lines.push(
      line(
        content.depreciationAccountId!,
        decimalToUnits(result.depreciation),
        true,
        content.dimensions,
      ),
      line(
        content.accumulatedAccountId!,
        decimalToUnits(result.depreciation),
        false,
        content.dimensions,
      ),
    );
  return { content, result, ordinal, hash, lines };
}

async function prepareInstrumentRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof instrumentRunSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = instrumentRunSchema.parse(raw),
    runId = `instrument-run-${key}`,
    op = "finance.instrument-run.prepare";
  return ports.ownedTransaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, runId, { id, input });
    if (prior) return prior;
    await repo.tool(actor.companyId);
    const row = await get(repo, actor, id),
      e = await runEvidence(repo, actor, row, input);
    await repo.prepareJournal(
      actor,
      {
        chartId: e.content.chartId,
        periodId: input.periodId,
        entryDate: input.entryDate,
        currency: e.content.currency,
        memo: input.reason,
        sourceType: "instrument_payment",
        sourceId: runId,
        isOpening: false,
        lines: e.lines,
      },
      key,
    );
    const now = ports.now();
    return repo.prepareRunCommit({actor,id,input,key,op,runId,e,now});
  });
}

async function getRun(repo:InstrumentRepository, actor: WorkContext, id: string) {
  const row = await repo.run(actor,id);
  if (!row)
    throw ports.error(
      404,
      "INSTRUMENT_RUN_NOT_FOUND",
      "Financing payment draft not found.",
    );
  return row;
}

async function decideInstrumentRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof instrumentDecisionSchema>,
  action: "post" | "reject",
  key: string,
) {
  actor = ports.permission(actor, "finance.approve");
  const input = instrumentDecisionSchema.parse(raw),
    op = `finance.instrument-run.${action}`;
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    const run = await getRun(repo, actor, id);
    if (run.status !== "draft" || run.version !== input.expectedVersion)
      throw ports.error(
        409,
        "INSTRUMENT_RUN_CHANGED",
        "Refresh the payment draft before deciding.",
      );
    if (ports.samePerson(actor, run.prepared_by))
      throw ports.error(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review the bank evidence and repayment.",
      );
    const row = await get(repo, actor, String(run.instrument_id)),
      definition = instrumentRunSchema.parse(
        JSON.parse(String(run.input_json)),
      ),
      e =
        action === "post"
          ? await runEvidence(repo, actor, row, definition)
          : null;
    if (e && e.hash !== run.source_hash)
      throw ports.error(
        409,
        "INSTRUMENT_RUN_SOURCE_CHANGED",
        "Schedule, mappings or bank evidence changed. Reject and prepare a fresh draft.",
      );
    const journalId = e
      ? await repo.prepareJournal(
          actor,
          {
            chartId: e.content.chartId,
            periodId: definition.periodId,
            entryDate: definition.entryDate,
            currency: e.content.currency,
            memo: input.reason,
            sourceType: "instrument_payment",
            sourceId: id,
            isOpening: false,
            lines: e.lines,
          },
          key,
          "posted",
        )
      : null;
    const status=action==='post'?'posted':'rejected',agreementStatus=e?.result.closing==='0.00'?'closed':'active',agreementUpdatedAt=journalId?ports.now():null,updatedAt=ports.now();
    return repo.decideRunCommit({actor,id,input,action,key,op,row,run,journalId,status,agreementStatus,agreementUpdatedAt,updatedAt});
  });
}

async function reverseInstrumentRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof instrumentReversalSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.approve");
  const input = instrumentReversalSchema.parse(raw),
    op = "finance.instrument-run.reverse";
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    const run = await getRun(repo, actor, id),
      row = await get(repo, actor, String(run.instrument_id));
    if (
      run.status !== "posted" ||
      run.version !== input.expectedVersion ||
      !["active", "closed"].includes(String(row.status))
    )
      throw ports.error(
        409,
        "INSTRUMENT_RUN_CHANGED",
        "Reverse only the latest posted payment in the current agreement edition.",
      );
    if (ports.samePerson(actor, run.prepared_by))
      throw ports.error(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "An independent Finance approver must reverse the payment.",
      );
    if (
      (await repo.laterRuns(String(row.id),Number(run.ordinal))) ||
      (await repo.laterEdition(String(row.id)))
    )
      throw ports.error(
        409,
        "INSTRUMENT_LATER_WORK",
        "Decide later work and reverse later payments first.",
      );
    const reversalId = await repo.prepareReversal(
        actor,
        String(run.journal_id),
        "instrument_payment",
        id,
        input,
        key,
      ),
      now = ports.now();
    return repo.reverseRunCommit({actor,id,input,key,op,row,run,reversalId,now});
  });
}

async function instrumentWorkspace(actor: WorkContext) {
  actor = ports.permission(actor, "finance.read");
  return ports.workspace(actor);
}
return {prepareInstrument,decideInstrument,prepareInstrumentRun,decideInstrumentRun,reverseInstrumentRun,instrumentWorkspace};}


