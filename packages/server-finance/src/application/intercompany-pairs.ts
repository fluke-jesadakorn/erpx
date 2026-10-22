import {z} from 'zod';
import {decimalToUnits} from "@erpx/feature-finance/lib/finance-money";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {FinanceError} from '@erpx/server-finance/domain/invoice-line-revision';
import {intercompanyPairSchema,intercompanyPairDecisionSchema,type PairFactRow,type PairSource} from '@erpx/server-finance/domain/intercompany-pairs';
type PairInput=z.infer<typeof intercompanyPairSchema>;type PairDecision=z.infer<typeof intercompanyPairDecisionSchema>;
export interface IntercompanyPairRepository{moneyScale(companyId:string,currency:string):Promise<number>;tool(companyId:string):Promise<void>;entity(holdingId:string,companyId:string):Promise<PairFactRow|null>;journal(journalId:string,companyId:string,currency:string):Promise<PairFactRow|null>;lines(journalId:string,accountId:string,companyId:string):Promise<PairFactRow[]>;replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;pair(actor:WorkContext,id:string):Promise<PairFactRow|null>;prepareCommit(facts:{actor:WorkContext;key:string;op:string;input:PairInput;id:string;e:PairSource;now:number}):Promise<Record<string,unknown>>;decisionCommit(facts:{actor:WorkContext;id:string;input:PairDecision;action:'confirm'|'reject'|'reverse';key:string;op:string;expected:string;status:string;now:number}):Promise<Record<string,unknown>>;}
export interface IntercompanyPairPorts{error:FinanceError;permission(actor:WorkContext,permission:string):WorkContext;samePerson(actor:WorkContext,userId:string|number|null):boolean;hash(value:string):Promise<string>;now():number;transaction<T>(actor:WorkContext,run:(repository:IntercompanyPairRepository)=>Promise<T>):Promise<T>;workspace(actor:WorkContext):Promise<PairFactRow[]>;}
export function createIntercompanyPairService(ports:IntercompanyPairPorts){
function actorGuard(actor: WorkContext, permission: string) {
  actor = ports.permission(actor, permission);
  if (actor.scope.kind !== "holding")
    throw ports.error(
      403,
      "HOLDING_FINANCE_REQUIRED",
      "Intercompany pairing requires Holding Finance authority.",
    );
  return actor;
}

async function intercompanyPairSource(
  repo:IntercompanyPairRepository,
  actor: WorkContext,
  input: z.infer<typeof intercompanyPairSchema>,
) {
  await repo.moneyScale(actor.companyId,input.currency);
  const evidence = [];
  for (const side of [input.source, input.target]) {
    const entity = await repo.entity(actor.holdingId,side.companyId),
      journal = await repo.journal(side.journalId,side.companyId,input.currency);
    const lines = (
      await repo.lines(side.journalId,side.accountId,side.companyId)
    );
    if (!entity || !journal || !lines.length)
      throw ports.error(
        409,
        "INTERCOMPANY_POSTED_DOCUMENT_REQUIRED",
        "Select original posted document accounts from two active Group Companies in the same transaction currency.",
      );
    const net = lines.reduce(
      (s, l) => s + BigInt(l.debit_minor!) - BigInt(l.credit_minor!),
      BigInt(0),
    );
    if ((net < BigInt(0) ? -net : net) !== decimalToUnits(input.amount))
      throw ports.error(
        409,
        "INTERCOMPANY_DOCUMENT_AMOUNT_CHANGED",
        "Pair the complete signed account amount from each posted document.",
      );
    evidence.push({ entity, journal, lines, net: String(net) });
  }
  if (BigInt(evidence[0].net) + BigInt(evidence[1].net) !== BigInt(0))
    throw ports.error(
      422,
      "INTERCOMPANY_DOCUMENT_DIRECTIONS",
      "The two posted document accounts must have equal opposite signed amounts.",
    );
  const categories = evidence.map((e) => String(e.lines[0].category)),
    balance = categories.every((c) => ["asset", "liability"].includes(c)),
    profit = categories.every((c) =>
      ["revenue", "expense", "other_income", "other_expense"].includes(c),
    );
  if (!balance && !profit)
    throw ports.error(
      422,
      "INTERCOMPANY_DOCUMENT_CATEGORIES",
      "Pair balance-sheet balances or income and expense documents within the same statement.",
    );
  const hash = await ports.hash(JSON.stringify({ input, evidence }));
  return { evidence, hash };
}

async function prepareIntercompanyPair(
  actor: WorkContext,
  raw: z.input<typeof intercompanyPairSchema>,
  key: string,
) {
  actor = actorGuard(actor, "finance.write");
  const input = intercompanyPairSchema.parse(raw),
    id = `intercompany-pair-${key}`,
    op = "finance.intercompany-pair.prepare";
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    await repo.tool(actor.companyId);
    const e = await intercompanyPairSource(repo, actor, input),
      now = ports.now();
    return repo.prepareCommit({actor,key,op,input,id,e,now});
  });
}

async function decideIntercompanyPair(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof intercompanyPairDecisionSchema>,
  action: "confirm" | "reject" | "reverse",
  key: string,
) {
  actor = actorGuard(actor, "finance.approve");
  const input = intercompanyPairDecisionSchema.parse(raw),
    op = `finance.intercompany-pair.${action}`;
  return ports.transaction(actor, async (repo) => {
    const prior = await repo.replay(actor, key, op, id, input);
    if (prior) return prior;
    const r = await repo.pair(actor,id);
    if (!r)
      throw ports.error(
        404,
        "INTERCOMPANY_PAIR_NOT_FOUND",
        "Intercompany document pair was not found.",
      );
    const expected = action === "reverse" ? "confirmed" : "draft";
    if (r.status !== expected || r.version !== input.expectedVersion)
      throw ports.error(
        409,
        "INTERCOMPANY_PAIR_CHANGED",
        "Refresh the paired document evidence before deciding.",
      );
    if (ports.samePerson(actor, r.prepared_by))
      throw ports.error(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must confirm or reverse this document pair.",
      );
    const definition = intercompanyPairSchema.parse(
      JSON.parse(String(r.source_json)).input,
    );
    if (
      action === "confirm" &&
      (await intercompanyPairSource(repo, actor, definition)).hash !==
        r.source_hash
    )
      throw ports.error(
        409,
        "INTERCOMPANY_PAIR_SOURCE_CHANGED",
        "A posted document or account changed. Reject and prepare again.",
      );
    const status =
        action === "confirm"
          ? "confirmed"
          : action === "reverse"
            ? "reversed"
            : "rejected",
      now=ports.now();
    return repo.decisionCommit({actor,id,input,action,key,op,expected,status,now});
  });
}

async function intercompanyPairWorkspace(actor: WorkContext) {
  actor = actorGuard(actor, "finance.read");
  return ports.workspace(actor);
}
return {intercompanyPairSource,prepareIntercompanyPair,decideIntercompanyPair,intercompanyPairWorkspace};}


