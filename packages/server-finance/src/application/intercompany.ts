import {z} from 'zod';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {FinanceError} from '@erpx/server-finance/domain/invoice-line-revision';
import {transition,type DecisionAction,type IntercompanyRow,type GroupEntity,type intercompanyTransactionSchema,type intercompanyDecisionSchema,type groupEntitySchema} from '@erpx/server-finance/domain/intercompany';
type CreateInput=z.infer<typeof intercompanyTransactionSchema>;type DecisionInput=z.infer<typeof intercompanyDecisionSchema>;type EntityInput=z.infer<typeof groupEntitySchema>;
export interface IntercompanyPorts{error:FinanceError;permission(actor:WorkContext,permission:string):WorkContext;allowed(actor:WorkContext,permission:string):boolean;ready(actor:WorkContext):Promise<void>;amount(value:string):number;now():number;replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;entities(actor:WorkContext,targetCompanyId:string):Promise<{company_id:string}[]>;transaction(actor:WorkContext,id:string):Promise<IntercompanyRow|null>;journal(id:string,companyId:string,currency:string):Promise<object|null>;workspace(actor:WorkContext):Promise<{transactions:IntercompanyRow[];groupEntities:GroupEntity[];availableCompanies:{company_id:string;display_name:string}[]}>;company(id:string):Promise<{id:string}|null>;existingEntity(actor:WorkContext,companyId:string):Promise<{id:string}|null>;createEntityCommit(facts:{actor:WorkContext;input:EntityInput;key:string;id:string;now:number}):Promise<Record<string,unknown>>;createCommit(facts:{actor:WorkContext;input:CreateInput;key:string;id:string;now:number;result:{id:string;status:string;version:number;amountMinor:number};amount:number}):Promise<Record<string,unknown>>;previousActor(id:string):Promise<{actor_id:string}|null>;decisionCommit(facts:{actor:WorkContext;id:string;action:DecisionAction;input:DecisionInput;key:string;rule:typeof transition[DecisionAction];row:IntercompanyRow;version:number;now:number;result:{id:string;status:string;version:number}}):Promise<Record<string,unknown>>;detail(actor:WorkContext,id:string):Promise<IntercompanyRow|null>;}
export function createIntercompanyService(ports:IntercompanyPorts){
function holdingActor(actor: WorkContext, permission: string) {
  actor = ports.permission(actor, permission);
  if (actor.scope.kind !== "holding") throw ports.error(403, "HOLDING_FINANCE_REQUIRED", "Intercompany finance requires a Holding-scoped Finance assignment.");
  return actor;
}

async function entities(actor: WorkContext, targetCompanyId: string) {
  const rows = await ports.entities(actor,targetCompanyId);
  if (rows.length !== 2) throw ports.error(422, "GROUP_ENTITY_REQUIRED", "Both companies must have active Group Finance entities before recording an intercompany item.");
}

async function getTransaction(actor: WorkContext, id: string) {
  const row = await ports.transaction(actor,id);
  if (!row) throw ports.error(404, "INTERCOMPANY_NOT_FOUND", "Intercompany transaction not found in this Holding.");
  return row;
}

async function journalGuard(id: string | undefined, companyId: string, currency: string) {
  if (!id) return;
  const row = await ports.journal(id,companyId,currency);
  if (!row) throw ports.error(422, "INTERCOMPANY_JOURNAL_INVALID", "Source and target journals must be posted in the matching company and currency.");
}

async function intercompanyWorkspace(actor: WorkContext) {
  actor = holdingActor(actor, "finance.read");
  await ports.ready(actor);
  const {transactions,groupEntities,availableCompanies}=await ports.workspace(actor);
  return {
    companyId: actor.companyId,
    transactions: transactions,
    entities: groupEntities,
    availableCompanies: availableCompanies,
    canWrite: ports.allowed(actor, "finance.write"),
    canApprove: ports.allowed(actor, "finance.approve"),
    canPay: ports.allowed(actor, "finance.pay"),
    statusCounts: transactions.reduce<Record<string, number>>((result, row) => { result[row.status] = (result[row.status] ?? 0) + 1; return result; }, {}),
  };
}

async function createGroupEntity(actor: WorkContext, input: z.infer<typeof groupEntitySchema>, key: string) {
  actor = holdingActor(actor, "finance.write");
  await ports.ready(actor);
  const id = `group-entity-${key}`;
  const previous = await ports.replay(actor, key, "finance.group-entity.create", id, input);
  if (previous) return previous;
  const company = await ports.company(input.companyId);
  if (!company) throw ports.error(422, "GROUP_COMPANY_REQUIRED", "Choose an active Company organization node in this Holding.");
  const existing = await ports.existingEntity(actor,input.companyId);
  if (existing) throw ports.error(409, "GROUP_ENTITY_EXISTS", "This company is already registered as a Group Finance entity.");
  const now = ports.now();
  return ports.createEntityCommit({actor,input,key,id,now});
}

async function createIntercompany(actor: WorkContext, input: z.infer<typeof intercompanyTransactionSchema>, key: string) {
  actor = holdingActor(actor, "finance.write");
  await ports.ready(actor);
  const previous = await ports.replay(actor, key, "finance.intercompany.create", `intercompany-${key}`, input);
  if (previous) return previous;
  if (input.targetCompanyId === actor.companyId) throw ports.error(422, "INTERCOMPANY_SAME_COMPANY", "Choose a different target company.");
  await entities(actor, input.targetCompanyId);
  const amount = ports.amount(input.amount);
  if (amount <= 0) throw ports.error(422, "INTERCOMPANY_AMOUNT_INVALID", "Intercompany amount must be greater than zero.");
  await journalGuard( input.sourceJournalId, actor.companyId, input.currency);
  await journalGuard( input.targetJournalId, input.targetCompanyId, input.currency);
  if ((input.sourceJournalId && !input.targetJournalId) || (!input.sourceJournalId && input.targetJournalId)) throw ports.error(422, "INTERCOMPANY_JOURNAL_PAIR_REQUIRED", "Provide both posted journals or neither journal.");
  const id = `intercompany-${key}`, now = ports.now(), result = { id, status: "unmatched", version: 1, amountMinor: amount };
  return ports.createCommit({actor,input,key,id,now,result,amount});
}

async function decideIntercompany(actor: WorkContext, id: string, action: DecisionAction, input: z.infer<typeof intercompanyDecisionSchema>, key: string) {
  const rule = transition[action];
  actor = holdingActor(actor, rule.permission);
  await ports.ready(actor);
  const previous = await ports.replay(actor, key, rule.operation, id, input);
  if (previous) return previous;
  const row = await getTransaction(actor, id);
  if (row.status !== rule.from || Number(row.version) !== input.expectedVersion) throw ports.error(409, "INTERCOMPANY_CHANGED", `The intercompany item is no longer ${rule.from}; reload before deciding.`);
  const previousEvent = await ports.previousActor(id);
  if (previousEvent?.actor_id === actor.userId) throw ports.error(409, "SELF_REVIEW_FORBIDDEN", "A different authorized Finance colleague must perform this handoff.");
  const version = input.expectedVersion + 1, now = ports.now(), result = { id, status: rule.to, version };
  return ports.decisionCommit({actor,id,action,input,key,rule,row,version,now,result});
}

async function intercompanyDetail(actor: WorkContext, id: string) {
 actor = holdingActor(actor, "finance.read");
 await ports.ready(actor);
 const row = await ports.detail(actor,id);
 if (!row) throw ports.error(404,"INTERCOMPANY_NOT_FOUND","Intercompany transaction not found.");
 return { row, data: { ...await intercompanyWorkspace(actor), transactions: [row] } };
}
return {intercompanyWorkspace,createGroupEntity,createIntercompany,decideIntercompany,intercompanyDetail};}


