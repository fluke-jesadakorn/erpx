import {z} from "zod";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Clock,IdentifierSource} from "@erpx/server-shared/application/ports";
import {kinds,parseItem,type LegalItem,type ItemInput} from "@erpx/server-legal/domain/model";
import {previewLegalImport} from "@erpx/server-legal/domain/operations";
import type {LegalWrite} from "@erpx/server-legal/application/service";
export type LegalRows<Row>={results:Row[]};
export type LegalOperationUnit={
registerRows(kind:LegalItem["kind"]):Promise<LegalRows<LegalItem>>;
exportRead(facts:{identifier:string;id:string;null:null;actorId:string;fact4:"register.export";now:number;}):LegalWrite;
savedViews(facts:{companyId:string;actorId:string;}):Promise<LegalRows<Record<string, unknown>>>;
saveView(facts:{companyId:string;actorId:string;name:string;fact3:string;}):LegalWrite;
derivedLink(facts:{id:string;id2:string;}):Promise<LegalRows<Record<string, unknown>>>;
envelopes(facts:{id:string;}):Promise<LegalRows<Record<string, unknown>>>;
documents(facts:{id:string;}):Promise<LegalRows<Record<string, unknown>>>;
events(facts:{id:string;}):Promise<LegalRows<Record<string, unknown>>>;
accesses(facts:{id:string;}):Promise<LegalRows<Record<string, unknown>>>;
evidenceRead(facts:{identifier:string;id:string;null:null;actorId:string;fact4:"evidence.export";now:number;}):Promise<LegalRows<Record<string, unknown>>>;
featureHistory(facts:{id:string;}):Promise<LegalRows<Record<string, unknown>>>;
retention(facts:{id:string;}):Promise<LegalRows<Record<string, unknown>>>;
holdPreview(id:string):Promise<LegalRows<Record<string, unknown>>>;
proposedAi(facts:{proposalId:string;id:string;actorId:string;}):Promise<{ source_version: number; source_hash: string; proposal: string; } | null>;
nextDocumentVersion(facts:{id:string;}):Promise<{ n: number; } | null>;
reviewedAiDocument(facts:{documentId:string;id:string;next:number;name:string;checksum:string;bytes:number;body:string;actorId:string;now:number;}):LegalWrite;
acceptProposal(facts:{fact0:string;proposalId:string;}):LegalWrite;
advanceItem(facts:{now:number;id:string;}):LegalWrite;
ruleImpact(id:string):Promise<LegalRows<Record<string, unknown>>>;
access(id:string,permission?:string):Promise<LegalItem>;
command(raw:unknown,key:string):Promise<{id:string;version:number}>;
purchase(raw:unknown,key:string):Promise<{id:string}>;
executedCopies(id:string):ReturnType<ReturnType<typeof import("@erpx/server-legal/application/executed-copies").createExecutedCopyApplication>["executedCopies"]>;
batch(writes:LegalWrite[]):Promise<unknown>;
guardChanges(id:string,version:number,key:string,edit?:boolean,allowFrozen?:boolean,preparation?:boolean):LegalWrite[];
event(id:string,action:string,note:string,version:number,identifier:string,now:number):LegalWrite;
replay<Result extends object=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
commit<Result extends object>(key:string,operation:string,input:unknown,id:string,version:number,result:Result,writes:LegalWrite[]):Promise<Result>;
};
export type LegalOperationsRepository=LegalOperationUnit&{transaction<Result>(work:(unit:LegalOperationUnit)=>Promise<Result>):Promise<Result>};
export function createLegalOperationsApplication(ports:{actor:WorkContext;repository:LegalOperationsRepository;clock:Clock;identifiers:IdentifierSource;hash(text:string):string;byteLength(text:string):number;require(permission:string):void;error(status:number,code:string,message:string):Error}){const a=ports.actor;function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
async function importPreview(raw:unknown){ports.require("legal.create");return previewLegalImport(raw,ports.hash);}
async function importCommit(
  raw: unknown,
  key: string,
) {
  const x = z
      .object({
        rows: z.array(z.unknown()).min(1).max(100),
        digest: z.string(),
      })
      .parse(raw),
    preview = await importPreview(x.rows);
  if (preview.errors.length || preview.digest !== x.digest)
    fail("Import changed or contains invalid rows. Preview it again.");
  return ports.repository.transaction(async (tx) => {
    const items = [];
    for (let i = 0; i < preview.rows.length; i++)
      items.push(
        await tx.command({ action: "create", item: preview.rows[i] },`${key}:${i}`),
      );
    return { items };
  });
}
async function exportRegister(
  kind: string,
) {
  ports.require("legal.export");
  const k = z.enum(kinds).parse(kind);
  const rows = (
    await ports.repository.registerRows(k)
  ).results;
  if (rows.length > 10000)
    fail(
      "Export is limited to 10,000 records. Use a narrower organization scope.",
    );
  for (const r of rows) {
    if (typeof r.data === "string") r.data = JSON.parse(r.data);
  }
  await ports.repository.batch(
    rows.map((r) =>
      ports.repository.exportRead({identifier:ports.identifiers.next(),id:r.id,null:null,actorId:a.userId,fact4:"register.export",now:ports.clock.now()}),
    ),
  );
  return rows;
}
async function savedViews() {
  ports.require("legal.read");
  return (
    await ports.repository.savedViews({companyId:a.companyId,actorId:a.userId})
  ).results.map((r) => ({
    ...r,
    query: typeof r.query === "string" ? JSON.parse(r.query) : r.query,
  }));
}
async function saveView(
  raw: unknown,
  key: string,
) {
  ports.require("legal.read");
  const x = z
    .object({
      name: z.string().trim().min(1).max(80),
      query: z.object({
        kind: z.enum(kinds),
        search: z.string().max(200),
        state: z.string().max(40),
      }),
    })
    .parse(raw);
  return ports.repository.commit(key,
    "legal.view.save",
    x,
    a.companyId,
    1,
    { id: a.companyId, version: 1 },
    [
      ports.repository.saveView({companyId:a.companyId,actorId:a.userId,name:x.name,fact3:JSON.stringify(x.query)}),
    ],
  );
}
async function applyTemplate(
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      id: z.string(),
      expectedVersion: z.number().int(),
      templateId: z.string(),
      values: z.record(z.string(), z.string().max(10000)),
    })
    .parse(raw);
  return ports.repository.transaction(async (tx) => {
    const t = await tx.access(x.templateId),
      target = await tx.access(x.id,"legal.edit");
    if (t.kind !== "template" || !["approved", "active"].includes(t.state))
      fail("Use an approved template.");
    if (target.state !== "draft") fail("Templates apply only to drafts.");
    const source = String(t.data.body ?? ""),
      body = source.replace(
        /\{\{([a-zA-Z][a-zA-Z0-9_]*)\}\}/g,
        (_whole, name) => {
          if (!(name in x.values))
            fail(`Template variable ${name} is missing.`);
          return x.values[name];
        },
      );
    const result = await tx.command({
        action: "document",
        id: x.id,
        expectedVersion: x.expectedVersion,
        name: t.title,
        body,
      },key);
    await tx.derivedLink({id:target.id,id2:t.id});
    return result;
  });
}
async function convertRequest(
  raw: unknown,
  key: string,
) {
  const x = z
      .object({
        id: z.string(),
        expectedVersion: z.number().int(),
        item: z.unknown(),
      })
      .parse(raw),
    newItem = parseItem(x.item);
  if (!["contract", "matter"].includes(newItem.kind))
    fail("Convert a request into a contract or matter.");
  return ports.repository.transaction(async (tx) => {
    const original = await tx.access(x.id,"legal.edit");
    if (original.kind !== "request" || original.version !== x.expectedVersion)
      fail("Request changed or unavailable.");
    const result = await tx.command({ action: "create", item: newItem },`${key}:create`);
    await tx.command({
        action: "link",
        id: original.id,
        expectedVersion: x.expectedVersion,
        targetId: result.id,
        relation: "derived",
      },`${key}:link`);
    return result;
  });
}
async function evidenceBundle(
  id: string,
) {
  await ports.repository.access(id,"legal.export");
  const envelopes = (
    await ports.repository.envelopes({id:id})
  ).results.map((r) => ({
    ...r,
    evidence:
      typeof r.evidence === "string" ? JSON.parse(r.evidence) : r.evidence,
  }));
  const documents = (
    await ports.repository.documents({id:id})
  ).results;
  const events = (
    await ports.repository.events({id:id})
  ).results;
  const accesses = (
    await ports.repository.accesses({id:id})
  ).results;
  if (accesses.length > 10000) fail("Access history exceeds the export limit.");
  await ports.repository.evidenceRead({identifier:ports.identifiers.next(),id:id,null:null,actorId:a.userId,fact4:"evidence.export",now:ports.clock.now()});
  return {
    recordId: id,
    workflowHistory: (
      await ports.repository.featureHistory({id:id})
    ).results,
    retention: (
      await ports.repository.retention({id:id})
    ).results,
    documents,
    executedCopies:await ports.repository.executedCopies(id),
    envelopes,
    events,
    accesses,
    exportedAt: new Date(ports.clock.now()).toISOString(),
    exportedBy: a.userId,
  };
}
async function holdPreview(id: string) {
  await ports.repository.access(id,"legal.hold");
  return (
    await ports.repository.holdPreview(id)
  ).results;
}
async function applyAiDraft(
  raw: unknown,
  key: string,
) {
  ports.require("legal.ai");
  const x = z
    .object({
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      proposalId: z.string(),
      name: z.string().trim().min(1).max(200),
      body: z.string().trim().min(1).max(200000),
    })
    .parse(raw);
  const item = await ports.repository.access(x.id,"legal.edit");
  const replay = await ports.repository.replay<{ id: string; version: number }>(key,
    "legal.ai.apply",
    item.id,
    x,
  );
  if (replay) return replay;
  const p = await ports.repository.proposedAi({proposalId:x.proposalId,id:item.id,actorId:a.userId});
  if (
    !p ||
    p.source_version !== item.version ||
    p.source_hash !== ports.hash(JSON.stringify(item)) ||
    x.expectedVersion !== item.version
  )
    fail(
      "The proposal or source changed. Request a fresh proposal.",
      "LEGAL_AI_CONFLICT",
      409,
    );
  if (item.state !== "draft")
    fail("Return this record to draft before accepting a proposal.");
  const next = (await ports.repository.nextDocumentVersion({id:item.id}))!.n;
  const documentId = ports.identifiers.next(),
    now = ports.clock.now(),
    checksum = ports.hash(x.body);
  return ports.repository.commit(key,
    "legal.ai.apply",
    x,
    item.id,
    item.version + 1,
    { id: item.id, version: item.version + 1 },
    [
      ...ports.repository.guardChanges(item.id,item.version,key,true),
      ports.repository.reviewedAiDocument({documentId:documentId,id:item.id,next:next,name:x.name,checksum:checksum,bytes:ports.byteLength(x.body),body:x.body,actorId:a.userId,now:now}),
      ports.repository.acceptProposal({fact0:JSON.stringify({
            acceptance: {
              documentId,
              checksum,
              reviewedBy: a.userId,
              reviewedAt: now,
            },
          }),proposalId:x.proposalId}),
      ports.repository.advanceItem({now:now,id:item.id}),
      ports.repository.event(item.id,"ai.accepted",`Reviewed proposal ${x.proposalId}; document ${documentId}`,item.version + 1,ports.identifiers.next(),ports.clock.now()),
    ],
  );
}

async function prepareCounselPurchase(
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      id: z.string(),
      expectedVersion: z.number().int(),
      summary: z.string().trim().min(3).max(500),
      estimatedCost: z.string().regex(/^\d{1,12}(\.\d{1,2})?$/),
    })
    .parse(raw);
  return ports.repository.transaction(async (tx) => {
    const r = await tx.access(x.id,"legal.edit");
    const prior = await tx.replay(key, "legal.finance.prepare", r.id, raw);
    if (prior) return prior;
    if (r.kind !== "counsel" || r.version !== x.expectedVersion)
      fail("Choose the current counsel record.");
    if (r.data.financeReference)
      fail("This counsel record already has a spending request.");
    const input = {
      kind: "purchase",
      category: "finance",
      title: "Outside counsel services",
      description: x.summary,
      currency: r.currency,
      branchId: r.branch_id,
      departmentId: r.department_id,
      items: [
        {
          description: x.summary,
          quantity: "1",
          estimatedCost: x.estimatedCost,
        },
      ],
    };
    const purchase = await tx.purchase(input,`${key}:purchase`);
    const item: ItemInput = {
      kind: r.kind,
      title: r.title,
      description: r.description,
      classification: r.classification,
      country: r.country,
      jurisdiction: r.jurisdiction,
      language: r.language,
      currency: r.currency,
      timezone: r.timezone,
      priority: r.priority as ItemInput["priority"],
      dueDate: r.due_date,
      ownerId: r.owner_id,
      data: { ...r.data, financeReference: purchase.id },
    };
    await tx.command({ action: "update", id: r.id, expectedVersion: r.version, item },`${key}:link`);
    return tx.commit(key, "legal.finance.prepare", raw, r.id,
      r.version + 1, { id: r.id, version: r.version + 1, requestId: purchase.id }, []);
  });
}

async function startRevision(
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      title: z.string().trim().min(3).max(240),
      reason: z.string().trim().min(3).max(4000),
    })
    .parse(raw);
  return ports.repository.transaction(async (tx) => {
    const original = await tx.access(x.id,"legal.edit");
    const prior = await tx.replay<{ id: string; version: number }>(key,
      "legal.revision",
      original.id,
      raw,
    );
    if (prior) return prior;
    if (!["contract", "rule", "template"].includes(original.kind))
      fail("Revisions are available for contracts, rules and templates.");
    if (original.version !== x.expectedVersion)
      fail("The source changed. Refresh first.", "LEGAL_VERSION_CONFLICT", 409);
    const created = await tx.command({
        action: "create",
        item: {
          kind: original.kind,
          title: x.title,
          description: x.reason,
          classification: original.classification,
          country: original.country,
          jurisdiction: original.jurisdiction,
          language: original.language,
          currency: original.currency,
          timezone: original.timezone,
          priority: original.priority,
          ownerId: original.owner_id,
          data: original.data,
        },
      },`${key}:create`);
    await tx.command({
        action: "link",
        id: original.id,
        expectedVersion: original.version,
        targetId: created.id,
        relation: "amends",
      },`${key}:link`);
    return tx.commit(key,
      "legal.revision",
      raw,
      original.id,
      original.version + 1,
      created,
      [],
    );
  });
}
async function ruleImpact(id: string) {
  const rule = await ports.repository.access(id);
  if (rule.kind !== "rule") fail("Choose a jurisdiction rule.");
  return {
    ruleId: id,
    ruleVersion: rule.version,
    items: (
      await ports.repository.ruleImpact(id)
    ).results,
    notice:
      "Only accessible linked records are shown. Existing deadlines remain unchanged until their owner reviews and updates each record.",
  };
}

return {importPreview,importCommit,exportRegister,savedViews,saveView,applyTemplate,convertRequest,evidenceBundle,holdPreview,applyAiDraft,prepareCounselPurchase,startRevision,ruleImpact};}
