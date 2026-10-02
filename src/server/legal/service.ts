import { samePerson } from "@/lib/iam";
import { legalPeople } from "./assignees";
import { productionFeatureGuard } from "../hospitality-rollout";
import "server-only";
import { z } from "zod";
import { createHash } from "node:crypto";
import {
  kinds,
  parseItem,
  allowedTransitions,
  type LegalItem,
  type LegalData,
  type LegalDetail,
  type DocumentVersion,
} from "@/lib/legal/model";
import type { SqlDatabase, SqlStatement } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { hasPermission, permissionCatalog } from "@/lib/permissions";
import { requirePermission } from "../iam-permissions";
import { executeCommand, replayCommand, assertion } from "../commands";
import {
  tables,
  access,
  enabled,
  visibility,
  guards,
  lock,
  event,
  eligible,
  personGuard,
  fail,
} from "./access";
export const hash = (s: string | Uint8Array) =>
  createHash("sha256").update(s).digest("hex");
export async function legalData(
  db: SqlDatabase,
  a: WorkContext,
  query: Record<string, unknown> = {},
): Promise<LegalData> {
  requirePermission(a, "legal.read");
  const q = z
      .object({
        kind: z.enum(kinds).optional(),
        search: z.string().max(200).default(""),
        state: z.string().max(40).default(""),
        page: z.coerce.number().int().min(1).max(100000).default(1),
        mine: z.enum(["true", "false"]).default("false"),
      })
      .parse(query),
    v = visibility(a);
  const clause = `${v.sql} AND (?::text IS NULL OR i.kind=?) AND (?='' OR strpos(lower(i.title||' '||i.description),lower(?))>0 OR EXISTS(SELECT 1 FROM legal_documents d WHERE d.item_id=i.id AND strpos(lower(d.name||' '||d.body),lower(?))>0) OR EXISTS(SELECT 1 FROM legal_document_inspections x JOIN legal_documents d ON d.id=x.document_id WHERE d.item_id=i.id AND x.kind='ocr' AND strpos(lower(x.result_json::jsonb->>'text'),lower(?))>0)) AND (?='' OR i.state=?) AND (?='false' OR i.owner_id=?)`;
  const args = [
    ...v.values,
    q.kind ?? null,
    q.kind ?? null,
    q.search,
    q.search,
    q.search,
    q.search,
    q.state,
    q.state,
    q.mine,
    a.userId,
  ];
  const [items, total, counts, people] = await Promise.all([
    db
      .prepare(
        `SELECT i.* FROM legal_items i WHERE ${clause} ORDER BY i.updated_at DESC,i.id LIMIT 30 OFFSET ?`,
      )
      .bind(...args, (q.page - 1) * 30)
      .all<LegalItem>(),
    db
      .prepare(`SELECT count(*)::int n FROM legal_items i WHERE ${clause}`)
      .bind(...args)
      .first<{ n: number }>(),
    db
      .prepare(
        `SELECT i.state,count(*)::int count FROM legal_items i WHERE ${v.sql} GROUP BY i.state`,
      )
      .bind(...v.values)
      .all<{ state: string; count: number }>(),
    legalPeople(db,a),
  ]);
  return {
    enabled: await enabled(db, a),
    items: items.results.map((i) => ({ ...i, data: {} })),
    total: total?.n ?? 0,
    page: q.page,
    counts: counts.results,
    permissions: permissionCatalog
      .filter((p) => p.id.startsWith("legal.") && hasPermission(a, p.id))
      .map((p) => p.id),
    userId: a.userId,
    workContext:{companyId:a.companyId,branchId:a.branchId,departmentId:a.departmentId,assignmentId:a.assignmentId,userId:a.userId},
    people: people.map((p) =>
      p.id === a.userId ? { ...p, name: a.name } : p,
    ),
  };
}
export async function detail(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
): Promise<LegalDetail> {
  const item = await access(db, a, id),
    v = visibility(a, "t"),
    holdScope = visibility(a, "source");
  const [documents, events, members, holds, links, comments, envelopes] =
    await Promise.all([
      db
        .prepare(
          "SELECT * FROM legal_documents WHERE item_id=? ORDER BY version DESC LIMIT 100",
        )
        .bind(id)
        .all<DocumentVersion>(),
      db
        .prepare(
          "SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at DESC,id DESC LIMIT 100",
        )
        .bind(id)
        .all<LegalDetail["events"][number]>(),
      db
        .prepare("SELECT user_id,access FROM legal_members WHERE item_id=?")
        .bind(id)
        .all<LegalDetail["members"][number]>(),
      db
        .prepare(
          `SELECT h.id,CASE WHEN ${holdScope.sql} THEN h.reason ELSE 'Protected by a related legal hold' END reason,h.state,(h.item_id=?) direct,(SELECT n.acknowledged_at FROM legal_hold_notices n WHERE n.hold_id=h.id AND n.item_id=t.item_id AND n.user_id=?) acknowledged_at FROM legal_holds h JOIN legal_hold_targets t ON t.hold_id=h.id JOIN legal_items source ON source.id=h.item_id WHERE t.item_id=? AND h.state!='released'`,
        )
        .bind(...holdScope.values, id, a.userId, id)
        .all<LegalDetail["holds"][number]>(),
      db
        .prepare(
          `SELECT l.target_id,l.relation,t.title FROM legal_links l JOIN legal_items t ON t.id=l.target_id WHERE l.source_id=? AND ${v.sql}`,
        )
        .bind(id, ...v.values)
        .all<LegalDetail["links"][number]>(),
      db
        .prepare(
          "SELECT id,body,visibility,created_by,created_at FROM legal_comments WHERE item_id=? ORDER BY created_at DESC LIMIT 100",
        )
        .bind(id)
        .all<LegalDetail["comments"][number]>(),
      db
        .prepare(
          "SELECT id,state,document_id,expires_at,consent FROM legal_envelopes WHERE item_id=? ORDER BY created_at DESC LIMIT 20",
        )
        .bind(id)
        .all<Omit<LegalDetail["envelopes"][number], "signers">>(),
    ]);
  const enriched = await Promise.all(
    envelopes.results.map(async (e) => ({
      ...e,
      signers: (
        await db
          .prepare(
            "SELECT user_id,ordinal,state,signed_at FROM legal_signers WHERE envelope_id=? ORDER BY ordinal",
          )
          .bind(e.id)
          .all<LegalDetail["envelopes"][number]["signers"][number]>()
      ).results,
    })),
  );
  await db
    .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
    .bind(crypto.randomUUID(), id, null, a.userId, "record.read", Date.now())
    .run();
  const shares = hasPermission(a, "legal.share")
    ? (
        await db
          .prepare(
            "SELECT id,email,document_id,user_id,expires_at,revoked_at FROM legal_shares WHERE item_id=? ORDER BY created_at DESC LIMIT 100",
          )
          .bind(id)
          .all<LegalDetail["shares"][number]>()
      ).results
    : [];
  const submissions = (
    await db
      .prepare(
        "SELECT u.id,u.name,u.checksum,u.created_by,u.created_at FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id WHERE s.item_id=? ORDER BY u.created_at DESC LIMIT 100",
      )
      .bind(id)
      .all<LegalDetail["submissions"][number]>()
  ).results;
  return {
    submissions,
    shares,
    item,
    documents: documents.results.map((d) => ({ ...d, object_key: null })),
    events: events.results,
    members: members.results,
    holds: holds.results,
    links: links.results,
    comments: comments.results,
    envelopes: enriched,
  };
}
const commandBase = z.object({
  action: z.string(),
  id: z.string().optional(),
  expectedVersion: z.number().int().positive().optional(),
});
export async function legalCommand(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const b = commandBase.parse(raw),
    input = raw as Record<string, unknown>;
  if (b.action === "settings") {
    requirePermission(a, "legal.manage");
    if (a.branchId || a.departmentId)
      fail(
        "Company settings require a Holding assignment.",
        "LEGAL_SCOPE",
        403,
      );
    const x = z
      .object({
        action: z.literal("settings"),
        enabled: z.boolean(),
        aiEnabled: z.boolean().default(false),
        reminderDays: z
          .array(z.number().int().min(1).max(365))
          .min(1)
          .max(20)
          .optional(),
      })
      .parse(raw);
    return executeCommand(
      db,
      a,
      key,
      "legal.settings",
      x,
      a.companyId,
      1,
      { id: a.companyId, version: 1 },
      [
        lock(db, a.companyId),
        db
          .prepare(
            "INSERT INTO legal_settings(company_id,enabled,ai_enabled,reminder_days) VALUES(?,?,?,coalesce(?::jsonb,'[90,60,30,7,1]'::jsonb)) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,ai_enabled=excluded.ai_enabled,reminder_days=coalesce(?::jsonb,legal_settings.reminder_days),version=legal_settings.version+1",
          )
          .bind(
            a.companyId,
            x.enabled,
            x.aiEnabled,
            x.reminderDays
              ? JSON.stringify(
                  [...new Set(x.reminderDays)].sort((a, b) => b - a),
                )
              : null,
            x.reminderDays
              ? JSON.stringify(
                  [...new Set(x.reminderDays)].sort((a, b) => b - a),
                )
              : null,
          ),
      ],
    );
  }
  if (b.action === "create") {
    requirePermission(a, "legal.create");
    if (!(await enabled(db, a)))
      fail("Enable Legal for this Company first.", "LEGAL_DISABLED", 409);
    let x;
    try {
      x = parseItem(input.item);
    } catch (e) {
      fail(e instanceof Error ? e.message : "Invalid legal record.");
    }
    await eligible(db, a, x.ownerId);
    if(x.kind==="obligation"&&x.data.escalationOwner)await eligible(db,a,String(x.data.escalationOwner));
    if(a.role==="legal_requester"&&(x.kind!=="request"||x.ownerId!==a.userId||x.data.advice))fail("Create a request owned by you; the Legal team records advice.","LEGAL_REQUESTER_SCOPE",403);
    const prior = await replayCommand<{ id: string; version: number }>(
      db,
      a,
      key,
      "legal.create",
      key,
      x,
    );
    if (prior) return prior;
    if(a.role==="legal_requester"&&!(await db.prepare("SELECT 1 FROM assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review') LIMIT 1").bind(a.companyId,a.branchId,a.departmentId).first()))fail("A current Legal Manager must be assigned to this Company before requests can be submitted.","LEGAL_ROUTING_UNAVAILABLE",409);
    const id = key,
      now = Date.now();
    return executeCommand(
      db,
      a,
      key,
      "legal.create",
      x,
      key,
      1,
      { id, version: 1 },
      [
        lock(db, a.companyId),
        personGuard(db,a,key,x.ownerId),
        ...(x.kind==="obligation"&&x.data.escalationOwner&&x.data.escalationOwner!==x.ownerId?[personGuard(db,a,key,String(x.data.escalationOwner))]:[]),
        ...(["rule","template"].includes(x.kind)&&!a.branchId&&!a.departmentId&&hasPermission(a,"legal.manage")?[]:productionFeatureGuard(db,key,a.companyId,"legal")),
        assertion(
          db,
          key,
          "legal-enabled",
          "EXISTS(SELECT 1 FROM legal_settings WHERE company_id=? AND enabled)",
          [a.companyId],
        ),
        db
          .prepare(
            "INSERT INTO legal_items(id,company_id,branch_id,department_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,due_date,owner_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            a.companyId,
            a.branchId,
            a.departmentId,
            x.kind,
            x.title,
            x.description,
            x.classification,
            x.country,
            x.jurisdiction,
            x.language,
            x.currency,
            x.timezone,
            x.priority,
            x.dueDate,
            x.ownerId,
            a.userId,
            now,
            now,
          ),
        db
          .prepare(`INSERT INTO ${tables[x.kind]} VALUES(?,?::jsonb)`)
          .bind(id, JSON.stringify(x.data)),
        ...Array.from(new Set([a.userId, x.ownerId])).map((u) =>
          db
            .prepare("INSERT INTO legal_members VALUES(?,?,'edit')")
            .bind(id, u),
        ),
        ...(a.role==="legal_requester"?[assertion(db,key,"legal-request-routing","EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review'))",[a.companyId,a.branchId,a.departmentId]),db.prepare("INSERT INTO legal_members(item_id,user_id,access) SELECT DISTINCT ?,user_id,'edit' FROM assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review') ON CONFLICT DO NOTHING").bind(id,a.companyId,a.branchId,a.departmentId)]:[]),
        event(db, a, id, "created", "", 1),
      ],
    );
  }
  const id = z.string().min(1).parse(b.id),
    version = z.number().int().positive().parse(b.expectedVersion);
  const permission =
    a.role==="legal_requester"&&b.action==="transition"&&input.state==="review"?"legal.edit":b.action === "hold.acknowledge"
      ? "legal.read"
      : b.action === "transition"
        ? input.state === "approved"
          ? "legal.approve"
          : input.state === "review" ||
              input.state === "approval" ||
              input.state === "rejected"
            ? "legal.review"
            : "legal.edit"
        : b.action.startsWith("hold.")
          ? b.action === "hold.release"
            ? "legal.hold.release"
            : "legal.hold"
          : b.action === "member"
            ? "legal.share"
            : b.action === "share" || b.action === "share.revoke"
              ? "legal.share"
              : b.action === "approve"
                ? "legal.approve"
                : "legal.edit";
  const r = await access(db, a, id, permission);
  if(a.role==="legal_requester"&&(!["update","comment","document","transition"].includes(b.action)||b.action==="transition"&&input.state!=="review"))fail("Requesters can edit their draft, submit it and reply to the Legal team.","LEGAL_REQUESTER_SCOPE",403);
  const prior = await replayCommand<{ id: string; version: number }>(
    db,
    a,
    key,
    `legal.${b.action}`,
    id,
    raw,
  );
  if (prior) return prior;
  if(a.role==="legal_requester"&&["update","document","transition"].includes(b.action)&&r.state!=="draft")fail("Only your draft question may be changed or submitted.","LEGAL_REQUESTER_SCOPE",403);
  if (r.version !== version)
    fail(
      "This record changed. Refresh before saving.",
      "LEGAL_VERSION_CONFLICT",
      409,
    );
  if (
    b.action === "approve" ||
    (b.action === "transition" && input.state === "approved")
  ) {
    const lastAuthor = await db
      .prepare(
        "SELECT actor_id FROM legal_events WHERE item_id=? AND action IN ('created','update','document','document.upload','ai.accepted') ORDER BY version DESC,created_at DESC LIMIT 1",
      )
      .bind(id)
      .first<{ actor_id: string }>();
    if (
      samePerson(a, r.created_by) ||
      (lastAuthor && samePerson(a, lastAuthor.actor_id))
    )
      fail(
        "The creator or latest content author cannot approve their own work.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
  }
  const steps: SqlStatement[] = guards(
    db,
    a,
    id,
    version,
    key,
    permission === "legal.edit",
    false,
    ["rule","template"].includes(r.kind),
  );
  let state = r.state,
    note = "";
  if (b.action === "update") {
    if (
      !["draft", "active"].includes(r.state) ||
      (["contract", "rule", "template"].includes(r.kind) && r.state !== "draft")
    )
      fail("Return the record to draft before changing approved content.");
    let x;
    try {
      x = parseItem(input.item);
    } catch (e) {
      fail(e instanceof Error ? e.message : "Invalid record.");
    }
    if (x.kind !== r.kind) fail("Record type cannot change.");
    if(a.role==="legal_requester"&&(r.state!=="draft"||x.ownerId!==r.owner_id||(x.data.advice??"")!==(r.data.advice??"")||x.classification!==r.classification||x.country!==r.country||x.jurisdiction!==r.jurisdiction||x.language!==r.language||x.currency!==r.currency||x.timezone!==r.timezone||x.priority!==r.priority||x.dueDate!==r.due_date))fail("Only your draft question may be edited.","LEGAL_REQUESTER_SCOPE",403);
    await eligible(db, a, x.ownerId,r);
    if(x.kind==="obligation"&&x.data.escalationOwner)await eligible(db,a,String(x.data.escalationOwner),r);
    steps.push(
      personGuard(db,a,key,x.ownerId,r),
      ...(x.kind==="obligation"&&x.data.escalationOwner&&x.data.escalationOwner!==x.ownerId?[personGuard(db,a,key,String(x.data.escalationOwner),r)]:[]),
      db
        .prepare(
          "UPDATE legal_items SET title=?,description=?,classification=?,country=?,jurisdiction=?,language=?,currency=?,timezone=?,priority=?,due_date=?,owner_id=? WHERE id=?",
        )
        .bind(
          x.title,
          x.description,
          x.classification,
          x.country,
          x.jurisdiction,
          x.language,
          x.currency,
          x.timezone,
          x.priority,
          x.dueDate,
          x.ownerId,
          id,
        ),
      db
        .prepare(`UPDATE ${tables[r.kind]} SET data=?::jsonb WHERE item_id=?`)
        .bind(JSON.stringify(x.data), id),
      db
        .prepare(
          "INSERT INTO legal_members VALUES(?,?,'edit') ON CONFLICT(item_id,user_id) DO UPDATE SET access='edit'",
        )
        .bind(id, x.ownerId),
    );
  } else if(b.action==="assign"){
    const x=z.object({ownerId:z.string().min(1).max(200),reason:z.string().trim().min(3).max(4000)}).parse(input);
    if(["archived","cancelled","closed","completed","terminated","expired"].includes(r.state))fail("Reopen this record before changing its owner.","LEGAL_TRANSITION",409);
    await eligible(db,a,x.ownerId,r);
    steps.push(personGuard(db,a,key,x.ownerId,r),db.prepare("UPDATE legal_items SET owner_id=? WHERE id=?").bind(x.ownerId,id),db.prepare("INSERT INTO legal_members VALUES(?,?,'edit') ON CONFLICT(item_id,user_id) DO UPDATE SET access='edit'").bind(id,x.ownerId));
    note=x.reason;
  } else if (b.action === "transition") {
    state = z.string().parse(input.state);
    note = z.string().trim().min(3).max(4000).parse(input.reason);
    if (!allowedTransitions(r.kind, r.state).includes(state))
      fail(`Cannot move ${r.state} to ${state}.`, "LEGAL_TRANSITION", 409);
    if (state === "approved" && r.kind === "contract")
      fail(
        "Approve a locked document and named signer list using contract approval.",
      );
    if (["approved", "rejected"].includes(state) && samePerson(a, r.created_by))
      fail(
        "An independent reviewer must decide.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
    if (
      state === "approved" &&
      r.kind === "rule" &&
      (!r.data.reviewNote || !r.data.effectiveDate)
    )
      fail("Record the legal review evidence before activating a rule.");
    if (
      state === "active" &&
      r.kind === "contract" &&
      !(await db
        .prepare(
          "SELECT 1 FROM legal_envelopes WHERE item_id=? AND state='completed' UNION ALL SELECT 1 FROM legal_executed_copies WHERE item_id=? AND state='confirmed' AND approved_cycle=(SELECT max(cycle) FROM legal_approvals WHERE item_id=?)",
        )
        .bind(id,id,id)
        .first())
    )
      fail("An independently confirmed executed copy is required.");
    if (
      state === "completed" &&
      r.kind === "obligation" &&
      !r.data.completionEvidence
    )
      fail("Completion evidence is required.");
    if (state === "cancelled")
      steps.push(
        db
          .prepare(
            "UPDATE legal_envelopes SET state='cancelled' WHERE item_id=? AND state='sent'",
          )
          .bind(id),
      );
  } else if (b.action === "member") {
    const x = z
      .object({
        userId: z.string().min(1),
        access: z.enum(["read", "edit", "remove"]),
      })
      .parse(input);
    if(x.access!=="remove")await eligible(db, a, x.userId,r);
    if (x.access === "remove" && [r.created_by, r.owner_id].includes(x.userId))
      fail(
        "Reassign ownership before removing the owner; creator membership is retained.",
      );
    steps.push(
      ...(x.access!=="remove"?[personGuard(db,a,key,x.userId,r)]:[]),
      x.access === "remove"
        ? db
            .prepare("DELETE FROM legal_members WHERE item_id=? AND user_id=?")
            .bind(id, x.userId)
        : db
            .prepare(
              "INSERT INTO legal_members VALUES(?,?,?) ON CONFLICT(item_id,user_id) DO UPDATE SET access=excluded.access",
            )
            .bind(id, x.userId, x.access),
    );
    note = `${x.userId}: ${x.access}`;
  } else if (b.action === "comment") {
    const x = z
      .object({
        body: z.string().trim().min(1).max(20000),
        visibility: z.enum(["internal", "external"]).default("internal"),
        section: z.string().max(200).default(""),
        documentId: z.string().default(""),
      })
      .parse(input);
    if (x.visibility === "external") {
      requirePermission(a, "legal.share");
      if (!x.documentId)
        fail("Choose the document version for external discussion.");
    }
    if (
      x.documentId &&
      !(await db
        .prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=?")
        .bind(x.documentId, id)
        .first())
    )
      fail("Choose a document belonging to this record.");
    steps.push(
      db
        .prepare(
          "INSERT INTO legal_comments(id,item_id,section,body,visibility,created_by,created_at,document_id) VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          crypto.randomUUID(),
          id,
          x.section,
          x.body,
          x.visibility,
          a.userId,
          Date.now(),
          x.documentId || null,
        ),
    );
    note = x.visibility;
  } else if (b.action === "document") {
    if (r.state !== "draft") fail("New content requires a draft record.");
    const x = z
      .object({
        name: z.string().min(1).max(200),
        body: z.string().min(1).max(200000),
      })
      .parse(input);
    const next = (await db
      .prepare(
        "SELECT coalesce(max(version),0)+1 n FROM legal_documents WHERE item_id=?",
      )
      .bind(id)
      .first<{ n: number }>())!.n;
    steps.push(
      db
        .prepare(
          "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at) VALUES(?,?,?,?,'text/plain',?,?,?,?,?)",
        )
        .bind(
          crypto.randomUUID(),
          id,
          next,
          x.name,
          hash(x.body),
          Buffer.byteLength(x.body),
          x.body,
          a.userId,
          Date.now(),
        ),
    );
    note = `Document version ${next}`;
  } else if (b.action === "approve") {
    if (
      await db
        .prepare(
          "SELECT 1 FROM approval_instances WHERE record_type='legal' AND record_id=? AND state='pending'",
        )
        .bind(r.id)
        .first()
    )
      fail(
        "Use the assigned approval route to decide this contract.",
        "LEGAL_APPROVAL_ROUTE",
        409,
      );
    if (r.kind !== "contract" || r.state !== "approval")
      fail("Contract must be awaiting approval.");
    if (samePerson(a, r.created_by))
      fail(
        "An independent reviewer must approve.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
    const x = z
      .object({
        documentId: z.string(),
        signerIds: z.array(z.string().min(1)).min(1).max(20),
        reason: z.string().trim().min(3).max(4000),
      })
      .parse(input);
    if (new Set(x.signerIds).size !== x.signerIds.length)
      fail("Each signer must be unique.");
    const d = await db
      .prepare("SELECT * FROM legal_documents WHERE id=? AND item_id=?")
      .bind(x.documentId, id)
      .first<DocumentVersion>();
    if (!d) fail("Choose a document belonging to this contract.");
    if(await db.prepare("SELECT 1 FROM legal_executed_copies WHERE document_id=?").bind(d.id).first())fail("Approve the draft document; imported executed copies are reviewed separately.");
    if (samePerson(a, d.created_by))
      fail(
        "The document author cannot approve their own document.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
    for (const signer of x.signerIds) {
      const internal = await db
        .prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?")
        .bind(id, signer)
        .first();
      const external = await db
        .prepare(
          "SELECT 1 FROM legal_shares WHERE item_id=? AND document_id=? AND user_id=? AND revoked_at IS NULL AND expires_at>?",
        )
        .bind(id, d.id, signer, Date.now())
        .first();
      if (!internal && !external)
        fail("Every signer needs explicit access to this document.");
    }
    steps.push(
      db
        .prepare("UPDATE legal_documents SET locked=true WHERE id=?")
        .bind(d.id),
      db
        .prepare(
          "INSERT INTO legal_approvals VALUES(?,?,?,?,?::jsonb,?,'approved',?)",
        )
        .bind(
          id,
          version + 1,
          d.id,
          d.checksum,
          JSON.stringify(x.signerIds),
          a.userId,
          Date.now(),
        ),
    );
    state = "approved";
    note = x.reason;
  } else if (b.action === "link") {
    const x = z
      .object({
        targetId: z.string(),
        relation: z.enum([
          "related",
          "evidence",
          "amends",
          "obligation",
          "derived",
        ]),
      })
      .parse(input);
    await access(db, a, x.targetId);
    if (id === x.targetId) fail("Choose another record.");
    steps.push(
      db
        .prepare("INSERT INTO legal_links VALUES(?,?,?) ON CONFLICT DO NOTHING")
        .bind(id, x.targetId, x.relation),
    );
    note = `${x.relation}: ${x.targetId}`;
  } else if (b.action === "hold.acknowledge") {
    const holdId = z.string().parse(input.holdId);
    note = z.string().trim().min(10).max(4000).parse(input.reason);
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM legal_hold_notices WHERE hold_id=? AND item_id=? AND user_id=?",
        )
        .bind(holdId, id, a.userId)
        .first())
    )
      fail("This preservation notice is not assigned to you.");
    steps.push(
      db
        .prepare(
          "UPDATE legal_hold_notices SET acknowledged_at=?,statement=? WHERE hold_id=? AND item_id=? AND user_id=?",
        )
        .bind(Date.now(), note, holdId, id, a.userId),
    );
  } else if (b.action === "hold.create") {
    note = z.string().trim().min(3).max(4000).parse(input.reason);
    const hold = crypto.randomUUID();
    steps.push(
      db
        .prepare(
          "INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at) VALUES(?,?,?,'active',?,?)",
        )
        .bind(hold, id, note, a.userId, Date.now()),
      db.prepare("INSERT INTO legal_hold_targets VALUES(?,?)").bind(hold, id),
    );
  } else if (
    b.action === "hold.request-release" ||
    b.action === "hold.release"
  ) {
    const h = await db
      .prepare("SELECT * FROM legal_holds WHERE id=? AND item_id=?")
      .bind(z.string().parse(input.holdId), id)
      .first<{
        id: string;
        state: string;
        created_by: string;
        release_requested_by: string | null;
      }>();
    if (!h) fail("Hold not found.");
    note = z.string().trim().min(3).max(4000).parse(input.reason);
    if (b.action === "hold.request-release") {
      if (h.state !== "active") fail("Hold is not active.");
      steps.push(
        db
          .prepare(
            "UPDATE legal_holds SET state='release_requested',release_requested_by=? WHERE id=?",
          )
          .bind(a.userId, h.id),
      );
    } else {
      if (
        h.state !== "release_requested" ||
        samePerson(a, h.release_requested_by) ||
        samePerson(a, h.created_by)
      )
        fail(
          "An independent reviewer must release this hold.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
      steps.push(
        db
          .prepare(
            "UPDATE legal_holds SET state='released',released_by=?,released_at=? WHERE id=?",
          )
          .bind(a.userId, Date.now(), h.id),
      );
    }
  } else if (b.action === "share") {
    const x = z
      .object({
        documentId: z.string(),
        email: z.email(),
        expiresAt: z
          .number()
          .int()
          .min(Date.now() + 60000)
          .max(Date.now() + 90 * 86400000),
      })
      .parse(input);
    if (
      !(await db
        .prepare("SELECT 1 FROM legal_documents WHERE item_id=? AND id=?")
        .bind(id, x.documentId)
        .first())
    )
      fail("Select a document from this record.");
    const shareId = crypto.randomUUID();
    steps.push(
      db
        .prepare(
          "INSERT INTO legal_shares(id,item_id,document_id,email,expires_at,created_by,created_at) VALUES(?,?,?,?,?,?,?)",
        )
        .bind(
          shareId,
          id,
          x.documentId,
          x.email.toLowerCase(),
          x.expiresAt,
          a.userId,
          Date.now(),
        ),
      db
        .prepare(
          "INSERT INTO legal_invitation_jobs(id,share_id,holding_slug,state,created_at) VALUES(?,?,?,'queued',?)",
        )
        .bind(crypto.randomUUID(), shareId, a.holdingSlug, Date.now()),
    );
    note = `Shared selected version with ${x.email}`;
  } else if (b.action === "share.revoke") {
    const share = z.string().parse(input.shareId);
    steps.push(
      db
        .prepare(
          "UPDATE legal_shares SET revoked_at=?,version=version+1 WHERE id=? AND item_id=?",
        )
        .bind(Date.now(), share, id),
    );
    note = "External access revoked";
  } else fail("Unsupported legal action.");
  steps.push(
    db
      .prepare(
        "UPDATE legal_items SET state=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(state, Date.now(), id),
    event(db, a, id, b.action, note, version + 1),
  );
  // A fixed point with UNION (not UNION ALL) terminates on cycles. Newly linked evidence is protected too.
  steps.push(
    db.prepare(
      "WITH RECURSIVE targets(hold_id,item_id) AS (SELECT id,item_id FROM legal_holds WHERE state!='released' UNION SELECT t.hold_id,l.target_id FROM targets t JOIN legal_links l ON l.source_id=t.item_id WHERE l.relation IN ('evidence','derived','amends','obligation')) INSERT INTO legal_hold_targets SELECT * FROM targets ON CONFLICT DO NOTHING",
    ),
  );
  if (["hold.create", "link"].includes(b.action))
    steps.push(
      assertion(
        db,
        key,
        "hold-preservable",
        "NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id JOIN legal_destructions d ON d.item_id=t.item_id WHERE h.state!='released' AND d.state IN ('executing','completed') AND (h.item_id=? OR EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=h.id AND own.item_id=?)))",
        [id, id],
      ),
      assertion(
        db,
        key,
        "hold-sources-preservable",
        "NOT EXISTS(SELECT 1 FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE hold.state!='released' AND legal_item_has_destroyed_attachment(target.item_id) AND (hold.item_id=? OR EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=hold.id AND own.item_id=?)))",
        [id, id],
      ),
    );
  steps.push(
    db
      .prepare(
        "WITH added AS (INSERT INTO legal_hold_notices(hold_id,item_id,user_id,notified_at) SELECT h.id,t.item_id,m.user_id,? FROM legal_holds h JOIN legal_hold_targets t ON t.hold_id=h.id JOIN legal_members m ON m.item_id=t.item_id WHERE h.state!='released' AND t.item_id IN (SELECT id FROM legal_items WHERE company_id=?) ON CONFLICT DO NOTHING RETURNING *) INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT 'hold-'||md5(n.hold_id||n.item_id||n.user_id),i.company_id,n.user_id,'legal',i.id,i.version,'A preservation notice requires acknowledgement',n.notified_at FROM added n JOIN legal_items i ON i.id=n.item_id ON CONFLICT DO NOTHING",
      )
      .bind(Date.now(), a.companyId),
  );
  return executeCommand(
    db,
    a,
    key,
    `legal.${b.action}`,
    raw,
    id,
    version + 1,
    { id, version: version + 1, state },
    steps,
  );
}
