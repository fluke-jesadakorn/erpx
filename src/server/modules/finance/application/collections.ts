import {z} from 'zod';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import {minorToDecimal} from "@/lib/finance/finance-money";
import {financeReviewSchema} from '../domain/review';
import {collectionCaseSchema,reviseCollectionCaseSchema,stage,type CollectionInvoice,type CollectionParent,type CollectionCase,type CollectionExposure,type CollectionPage,type CollectionQuery} from '../domain/collections';
import type {FinanceError} from '../domain/invoice-line-revision';
type PrepareInput=z.infer<typeof collectionCaseSchema>;type ReviseInput=z.infer<typeof reviseCollectionCaseSchema>;type ReviewInput=z.infer<typeof financeReviewSchema>;
export interface FinanceCollectionPorts{error:FinanceError;permission(actor:WorkContext,permission:'finance.read'|'finance.write'|'finance.approve'):WorkContext;samePerson(actor:WorkContext,userId:string):boolean;ready(actor:WorkContext):Promise<void>;now():number;id():string;hash(value:string):Promise<string>;tool(actor:WorkContext):Promise<void>;replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,op:string,input:unknown):Promise<Result|null>;reviewReplay(actor:WorkContext,key:string,op:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;invoice(actor:WorkContext,id:string):Promise<CollectionInvoice|null>;existingStage(actor:WorkContext,invoice:CollectionInvoice,stage:string):Promise<object|null>;prepareCommit(facts:{actor:WorkContext;key:string;op:string;input:PrepareInput;id:string;row:CollectionInvoice;ageStage:string;message:string;hash:string}):Promise<Record<string,unknown>>;parent(actor:WorkContext,id:string):Promise<CollectionParent|null>;hasChild(id:string):Promise<object|null>;hasDelivery(rootId:string):Promise<object|null>;reviseCommit(facts:{actor:WorkContext;key:string;op:string;parentId:string;input:ReviseInput;id:string;parent:CollectionParent;invoice:CollectionInvoice;message:string;sourceHash:string;now:number}):Promise<Record<string,unknown>>;case(actor:WorkContext,id:string):Promise<CollectionCase|null>;decisionCommit(facts:{actor:WorkContext;id:string;input:ReviewInput;action:'approve'|'cancel'|'test-deliver';key:string;op:string;row:CollectionCase;invoice:CollectionInvoice|null;status:string;now:number;payloadHash:string|null}):Promise<Record<string,unknown>>;picker(actor:WorkContext,asOf:string,query:CollectionQuery,search:string):Promise<{invoices:CollectionPage;cases:CollectionPage}>;chart(actor:WorkContext,id:string):Promise<{base_currency:string}|null>;bookSource(actor:WorkContext,id:string,asOf:string):Promise<{primary_chart_id:string}|null>;exposures(actor:WorkContext,asOf:string,chartId:string):Promise<CollectionExposure[]>;opening(actor:WorkContext,chartId:string,asOf:string):Promise<string|number|null>;}
export function createFinanceCollections(ports:FinanceCollectionPorts){
async function collectionSource(actor: WorkContext, id: string) {
  await ports.ready(actor);
  const row = await ports.invoice(actor,id);
  if (!row)
    throw ports.error(
      404,
      "COLLECTION_INVOICE_NOT_FOUND",
      "Select an open Company customer invoice.",
    );
  return row;
}

async function prepareCollectionCase(
  actor: WorkContext,
  raw: z.input<typeof collectionCaseSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = collectionCaseSchema.parse(raw);
  await ports.ready(actor);
  const
    op = "finance.collection.prepare",
    prior=await ports.replay(actor,key,op,input);
  if(prior)return prior;
  await ports.tool(actor);
  const row = await collectionSource(actor, input.invoiceId);
  if (row.version !== input.expectedVersion)
    throw ports.error(
      409,
      "STALE_VERSION",
      "Reload the customer obligation before preparing a collection case.",
    );
  const days = Math.floor(
    (Date.parse(input.asOf) - Date.parse(row.due_date)) / 86400000,
  );
  if (days <= 0)
    throw ports.error(
      422,
      "INVOICE_NOT_OVERDUE",
      "Collection cases require an overdue invoice.",
    );
  const ageStage = stage(days),
    id = ports.id(),
    hash = await ports.hash(
      JSON.stringify({
        version: row.version,
        amount: row.outstanding_minor,
        status: row.status,
        partyVersion: row.party_version,
      }),
    ),
    message =
      input.message ??
      `${row.party_name}: invoice ${row.invoice_number}, ${row.currency} ${minorToDecimal(row.outstanding_minor)}, was due ${row.due_date} (${days} days overdue). Please check payment status and contact us to arrange settlement.`;
  if (
    await ports.existingStage(actor,row,ageStage)
  )
    throw ports.error(
      409,
      "COLLECTION_CASE_ALREADY_PREPARED",
      "Open the existing collection case and revise its unsent message.",
    );
  return ports.prepareCommit({actor,key,op,input,id,row,ageStage,message,hash});
}

async function reviseCollectionCase(
  actor: WorkContext,
  parentId: string,
  raw: z.input<typeof reviseCollectionCaseSchema>,
  key: string,
) {
  actor = ports.permission(actor, "finance.write");
  const input = reviseCollectionCaseSchema.parse(raw);
  await ports.ready(actor);
  const
    op = "finance.collection.revise";
  const prior=await ports.replay(actor,key,op,{parentId,...input});if(prior)return prior;
  const parent = await ports.parent(actor,parentId);
  if (!parent)
    throw ports.error(
      404,
      "COLLECTION_CASE_NOT_FOUND",
      "Collection case not found.",
    );
  await ports.tool(actor);
  if (
    parent.version !== input.expectedVersion ||
    !["draft", "approved", "cancelled"].includes(parent.status) ||
    (await ports.hasChild(parentId))
  )
    throw ports.error(
      409,
      "COLLECTION_EDITION_CHANGED",
      "Open the current unsent collection edition before revising it.",
    );
  if (
    await ports.hasDelivery(parent.root_id)
  )
    throw ports.error(
      409,
      "COLLECTION_ALREADY_DELIVERED",
      "Delivered text is retained. A later invoice version or aging stage requires its own reviewed case.",
    );
  const invoice = await collectionSource(actor, parent.invoice_id);
  if (invoice.version !== input.expectedInvoiceVersion)
    throw ports.error(
      409,
      "STALE_VERSION",
      "Reload the current customer obligation before revising its message.",
    );
  const days = Math.floor(
    (Date.parse(input.asOf) - Date.parse(invoice.due_date)) / 86400000,
  );
  if (
    input.asOf < parent.as_of_date ||
    days <= 0 ||
    stage(days) !== parent.stage
  )
    throw ports.error(
      422,
      "COLLECTION_STAGE_CHANGED",
      "Use the original aging stage and a current date, or prepare the next stage as a new case.",
    );
  const id = ports.id(),
    now = ports.now();
  const sourceHash = await ports.hash(
    JSON.stringify({
      version: invoice.version,
      amount: invoice.outstanding_minor,
      status: invoice.status,
      partyVersion: invoice.party_version,
    }),
  );
  const message =
    input.message ??
    `${invoice.party_name}: invoice ${invoice.invoice_number}, ${invoice.currency} ${minorToDecimal(invoice.outstanding_minor)}, was due ${invoice.due_date} (${days} days overdue). Please check payment status and contact us to arrange settlement.`;
  return ports.reviseCommit({actor,key,op,parentId,input,id,parent,invoice,message,sourceHash,now});
}

async function decideCollectionCase(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof financeReviewSchema>,
  action: "approve" | "cancel" | "test-deliver",
  key: string,
) {
  actor = ports.permission(
    actor,
    action === "approve" ? "finance.approve" : "finance.write",
  );
  const input = financeReviewSchema.parse(raw);
  await ports.ready(actor);
  const
    op = `finance.collection.${action}`,
    prior = await ports.reviewReplay(actor,key,op,id,input);
  if (prior) return prior;
  const row = await ports.case(actor,id);
  if (!row)
    throw ports.error(
      404,
      "COLLECTION_CASE_NOT_FOUND",
      "Collection case not found.",
    );
  if (
    row.version !== input.expectedVersion ||
    ["cancelled", "superseded", "test_delivered"].includes(row.status) ||
    (action === "approve" && row.status !== "draft") ||
    (action === "test-deliver" && row.status !== "approved")
  )
    throw ports.error(
      409,
      "STALE_VERSION",
      "Reload the collection case before continuing.",
    );
  if (action === "approve" && ports.samePerson(actor, row.prepared_by))
    throw ports.error(
      403,
      "SELF_REVIEW_FORBIDDEN",
      "Another person must approve this customer message.",
    );
  let invoice:CollectionInvoice|null=null;
  if(action!=='cancel'){invoice=await collectionSource(actor,row.invoice_id);const hash=await ports.hash(JSON.stringify({version:invoice.version,amount:invoice.outstanding_minor,status:invoice.status,partyVersion:invoice.party_version}));if(hash!==row.source_hash)throw ports.error(409,'STALE_COLLECTION_EVIDENCE','The customer or amount changed. Prepare a fresh collection case.');}
  const status=action==='approve'?'approved':action==='cancel'?'cancelled':'test_delivered',now=ports.now(),payloadHash=action==='test-deliver'?await ports.hash(row.message):null;
  return ports.decisionCommit({actor,id,input,action,key,op,row,invoice,status,now,payloadHash});
}

async function collectionWorkspace(
  actor: WorkContext,
  asOf: string,
  query: CollectionQuery = {},
) {
  actor = ports.permission(actor, "finance.read");
  asOf = z.string().date().parse(asOf);
  await ports.ready(actor);
  const search = z
    .string()
    .max(200)
    .parse(query.q ?? "");
  const {invoices,cases}=await ports.picker(actor,asOf,query,search);
  return {
    asOf,
    invoices: invoices.items,
    cases: cases.items,
    pages: {
      invoices: {
        nextCursor: invoices.nextCursor,
        previousCursor: invoices.previousCursor,
      },
      cases: {
        nextCursor: cases.nextCursor,
        previousCursor: cases.previousCursor,
      },
    },
  };
}

async function cashDueForecast(
  actor: WorkContext,
  input: { chartId: string; asOf: string },
) {
  actor = ports.permission(actor, "finance.read");
  input = z
    .object({ chartId: z.string().min(1), asOf: z.string().date() })
    .parse(input);
  await ports.ready(actor);
  const
    chart = await ports.chart(actor,input.chartId);
  if (!chart)
    throw ports.error(404, "CHART_NOT_FOUND", "Company book not found.");
  const bookSource = await ports.bookSource(actor,input.chartId,input.asOf);
  const exposures = (
    await ports.exposures(actor,input.asOf,bookSource?.primary_chart_id??input.chartId)
  );
  const opening = Number(
    (await ports.opening(actor,input.chartId,input.asOf)) ?? 0,
  );
  let cash = BigInt(opening);
  const start = new Date(`${input.asOf.slice(0, 7)}-01T00:00:00Z`),
    months = Array.from({ length: 12 }, (_, i) => {
      const date = new Date(start);
      date.setUTCMonth(start.getUTCMonth() + i);
      const month = date.toISOString().slice(0, 7),
        items = exposures.filter(
          (r) =>
            (r.due_date.slice(0, 7) < input.asOf.slice(0, 7)
              ? input.asOf.slice(0, 7)
              : r.due_date.slice(0, 7)) === month,
        );
      const movement = items.reduce(
        (s, r) =>
          s +
          BigInt(r.historical_base_outstanding_minor) *
            (r.kind === "receivable" ? BigInt(1) : BigInt(-1)),
        BigInt(0),
      );
      cash += movement;
      if (
        cash > BigInt(Number.MAX_SAFE_INTEGER) ||
        cash < BigInt(Number.MIN_SAFE_INTEGER)
      )
        throw ports.error(
          422,
          "MONEY_OVERFLOW",
          "Forecast exceeds supported precision.",
        );
      return {
        month,
        movement: minorToDecimal(Number(movement)),
        closing: minorToDecimal(Number(cash)),
        invoiceIds: items.map((r) => r.id),
      };
    });
  return {
    ...input,
    currency: chart.base_currency,
    opening: minorToDecimal(opening),
    basis:
      "Due dates and recorded functional carrying amounts; future FX and unissued business are not estimated.",
    months,
  };
}
return {prepareCollectionCase,reviseCollectionCase,decideCollectionCase,collectionWorkspace,cashDueForecast};}


