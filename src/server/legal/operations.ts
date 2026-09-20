import "server-only";
import { z } from "zod";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { PostgresDatabase } from "../runtime/postgres";
import { requirePermission } from "../iam-permissions";
import {
  parseItem,
  kinds,
  type LegalItem,
  type ItemInput,
} from "@/lib/legal/model";
import { legalCommand, hash } from "./service";
import { access, visibility, tables, fail, guards, event } from "./access";
import { executeCommand, replayCommand } from "../commands";
export async function importPreview(a: WorkContext, raw: unknown) {
  requirePermission(a, "legal.create");
  const rows = z.array(z.unknown()).min(1).max(100).parse(raw),
    valid: ItemInput[] = [],
    errors: { row: number; message: string }[] = [];
  rows.forEach((r, i) => {
    try {
      valid.push(parseItem(r));
    } catch (e) {
      errors.push({
        row: i + 1,
        message: e instanceof Error ? e.message : "Invalid record",
      });
    }
  });
  return {
    rows: valid,
    errors,
    digest: hash(JSON.stringify(valid)),
    count: rows.length,
  };
}
export async function importCommit(
  db: PostgresDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
      .object({
        rows: z.array(z.unknown()).min(1).max(100),
        digest: z.string(),
      })
      .parse(raw),
    preview = await importPreview(a, x.rows);
  if (preview.errors.length || preview.digest !== x.digest)
    fail("Import changed or contains invalid rows. Preview it again.");
  return db.transaction(async (tx) => {
    const items = [];
    for (let i = 0; i < preview.rows.length; i++)
      items.push(
        await legalCommand(
          tx,
          a,
          { action: "create", item: preview.rows[i] },
          `${key}:${i}`,
        ),
      );
    return { items };
  });
}
export async function exportRegister(
  db: SqlDatabase,
  a: WorkContext,
  kind: string,
) {
  requirePermission(a, "legal.export");
  const k = z.enum(kinds).parse(kind),
    v = visibility(a);
  const rows = (
    await db
      .prepare(
        `SELECT i.*,t.data FROM legal_items i JOIN ${tables[k]} t ON t.item_id=i.id WHERE ${v.sql} AND i.kind=? ORDER BY i.id LIMIT 10001`,
      )
      .bind(...v.values, k)
      .all<LegalItem>()
  ).results;
  if (rows.length > 10000)
    fail(
      "Export is limited to 10,000 records. Use a narrower organization scope.",
    );
  for (const r of rows) {
    if (typeof r.data === "string") r.data = JSON.parse(r.data);
  }
  await db.batch(
    rows.map((r) =>
      db
        .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
        .bind(
          crypto.randomUUID(),
          r.id,
          null,
          a.userId,
          "register.export",
          Date.now(),
        ),
    ),
  );
  return rows;
}
export async function savedViews(db: SqlDatabase, a: WorkContext) {
  requirePermission(a, "legal.read");
  return (
    await db
      .prepare(
        "SELECT name,query FROM legal_saved_views WHERE company_id=? AND user_id=? ORDER BY name LIMIT 100",
      )
      .bind(a.companyId, a.userId)
      .all()
  ).results.map((r) => ({
    ...r,
    query: typeof r.query === "string" ? JSON.parse(r.query) : r.query,
  }));
}
export async function saveView(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  requirePermission(a, "legal.read");
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
  return executeCommand(
    db,
    a,
    key,
    "legal.view.save",
    x,
    a.companyId,
    1,
    { id: a.companyId, version: 1 },
    [
      db
        .prepare(
          "INSERT INTO legal_saved_views VALUES(?,?,?,?::jsonb) ON CONFLICT(company_id,user_id,name) DO UPDATE SET query=excluded.query",
        )
        .bind(a.companyId, a.userId, x.name, JSON.stringify(x.query)),
    ],
  );
}
export async function applyTemplate(
  db: PostgresDatabase,
  a: WorkContext,
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
  return db.transaction(async (tx) => {
    const t = await access(tx, a, x.templateId),
      target = await access(tx, a, x.id, "legal.edit");
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
    const result = await legalCommand(
      tx,
      a,
      {
        action: "document",
        id: x.id,
        expectedVersion: x.expectedVersion,
        name: t.title,
        body,
      },
      key,
    );
    await tx
      .prepare(
        "INSERT INTO legal_links VALUES(?,?,'derived') ON CONFLICT DO NOTHING",
      )
      .bind(target.id, t.id)
      .run();
    return result;
  });
}
export async function convertRequest(
  db: PostgresDatabase,
  a: WorkContext,
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
  return db.transaction(async (tx) => {
    const original = await access(tx, a, x.id, "legal.edit");
    if (original.kind !== "request" || original.version !== x.expectedVersion)
      fail("Request changed or unavailable.");
    const result = await legalCommand(
      tx,
      a,
      { action: "create", item: newItem },
      `${key}:create`,
    );
    await legalCommand(
      tx,
      a,
      {
        action: "link",
        id: original.id,
        expectedVersion: x.expectedVersion,
        targetId: result.id,
        relation: "derived",
      },
      `${key}:link`,
    );
    return result;
  });
}
export async function evidenceBundle(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
) {
  await access(db, a, id, "legal.export");
  const envelopes = (
    await db
      .prepare(
        "SELECT e.*,s.user_id,s.ordinal,s.state signer_state,s.signed_at,s.evidence FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.item_id=? ORDER BY e.created_at,s.ordinal",
      )
      .bind(id)
      .all()
  ).results.map((r) => ({
    ...r,
    evidence:
      typeof r.evidence === "string" ? JSON.parse(r.evidence) : r.evidence,
  }));
  const documents = (
    await db
      .prepare(
        "SELECT id,version,name,checksum,bytes,media_type,locked,created_at FROM legal_documents WHERE item_id=? ORDER BY version",
      )
      .bind(id)
      .all()
  ).results;
  const events = (
    await db
      .prepare(
        "SELECT * FROM legal_events WHERE item_id=? ORDER BY created_at,id",
      )
      .bind(id)
      .all()
  ).results;
  const accesses = (
    await db
      .prepare(
        "SELECT * FROM legal_access_events WHERE item_id=? ORDER BY created_at,id LIMIT 10001",
      )
      .bind(id)
      .all()
  ).results;
  if (accesses.length > 10000) fail("Access history exceeds the export limit.");
  await db
    .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
    .bind(
      crypto.randomUUID(),
      id,
      null,
      a.userId,
      "evidence.export",
      Date.now(),
    )
    .run();
  return {
    recordId: id,
    workflowHistory: (
      await db
        .prepare(
          "SELECT * FROM legal_feature_history WHERE item_id=? ORDER BY created_at,id",
        )
        .bind(id)
        .all()
    ).results,
    retention: (
      await db
        .prepare(
          "SELECT id,state,manifest,requested_by,approved_by,approved_at,completed_at FROM legal_destructions WHERE item_id=? ORDER BY created_at",
        )
        .bind(id)
        .all()
    ).results,
    documents,
    envelopes,
    events,
    accesses,
    exportedAt: new Date().toISOString(),
    exportedBy: a.userId,
  };
}
export async function holdPreview(db: SqlDatabase, a: WorkContext, id: string) {
  await access(db, a, id, "legal.hold");
  const v = visibility(a);
  return (
    await db
      .prepare(
        `WITH RECURSIVE linked(id) AS (SELECT ?::text UNION SELECT l.target_id FROM linked t JOIN legal_links l ON l.source_id=t.id WHERE l.relation IN ('evidence','derived','amends','obligation')) SELECT i.id,i.title FROM linked t JOIN legal_items i ON i.id=t.id WHERE ${v.sql}`,
      )
      .bind(id, ...v.values)
      .all()
  ).results;
}
export async function applyAiDraft(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  requirePermission(a, "legal.ai");
  const x = z
    .object({
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      proposalId: z.string(),
      name: z.string().trim().min(1).max(200),
      body: z.string().trim().min(1).max(200000),
    })
    .parse(raw);
  const item = await access(db, a, x.id, "legal.edit");
  const replay = await replayCommand<{ id: string; version: number }>(
    db,
    a,
    key,
    "legal.ai.apply",
    item.id,
    x,
  );
  if (replay) return replay;
  const p = await db
    .prepare(
      "SELECT source_version,source_hash,proposal FROM legal_ai_proposals WHERE id=? AND item_id=? AND created_by=? AND state='proposed'",
    )
    .bind(x.proposalId, item.id, a.userId)
    .first<{ source_version: number; source_hash: string; proposal: string }>();
  if (
    !p ||
    p.source_version !== item.version ||
    p.source_hash !== hash(JSON.stringify(item)) ||
    x.expectedVersion !== item.version
  )
    fail(
      "The proposal or source changed. Request a fresh proposal.",
      "LEGAL_AI_CONFLICT",
      409,
    );
  if (item.state !== "draft")
    fail("Return this record to draft before accepting a proposal.");
  const next = (await db
    .prepare(
      "SELECT coalesce(max(version),0)+1 n FROM legal_documents WHERE item_id=?",
    )
    .bind(item.id)
    .first<{ n: number }>())!.n;
  const documentId = crypto.randomUUID(),
    now = Date.now(),
    checksum = hash(x.body);
  return executeCommand(
    db,
    a,
    key,
    "legal.ai.apply",
    x,
    item.id,
    item.version + 1,
    { id: item.id, version: item.version + 1 },
    [
      ...guards(db, a, item.id, item.version, key, true),
      db
        .prepare(
          "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at) VALUES(?,?,?,?,'text/plain',?,?,?,?,?)",
        )
        .bind(
          documentId,
          item.id,
          next,
          x.name,
          checksum,
          Buffer.byteLength(x.body),
          x.body,
          a.userId,
          now,
        ),
      db
        .prepare(
          "UPDATE legal_ai_proposals SET state='accepted',proposal=proposal||?::jsonb WHERE id=?",
        )
        .bind(
          JSON.stringify({
            acceptance: {
              documentId,
              checksum,
              reviewedBy: a.userId,
              reviewedAt: now,
            },
          }),
          x.proposalId,
        ),
      db
        .prepare(
          "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, item.id),
      event(
        db,
        a,
        item.id,
        "ai.accepted",
        `Reviewed proposal ${x.proposalId}; document ${documentId}`,
        item.version + 1,
      ),
    ],
  );
}

export async function prepareCounselPurchase(
  db: PostgresDatabase,
  a: WorkContext,
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
  const { saveEmployeeRequest, employeeRequestSchema } =
    await import("../employee-requests");
  return db.transaction(async (tx) => {
    const r = await access(tx, a, x.id, "legal.edit");
    const prior = await replayCommand(tx, a, key, "legal.finance.prepare", r.id, raw);
    if (prior) return prior;
    if (r.kind !== "counsel" || r.version !== x.expectedVersion)
      fail("Choose the current counsel record.");
    if (r.data.financeReference)
      fail("This counsel record already has a spending request.");
    const input = employeeRequestSchema.parse({
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
    });
    const purchase = await saveEmployeeRequest(
      tx,
      a,
      null,
      input,
      `${key}:purchase`,
    );
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
    await legalCommand(
      tx,
      a,
      { action: "update", id: r.id, expectedVersion: r.version, item },
      `${key}:link`,
    );
    return executeCommand(tx, a, key, "legal.finance.prepare", raw, r.id,
      r.version + 1, { id: r.id, version: r.version + 1, requestId: purchase.id }, []);
  });
}

export async function startRevision(
  db: PostgresDatabase,
  a: WorkContext,
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
  return db.transaction(async (tx) => {
    const original = await access(tx, a, x.id, "legal.edit");
    const prior = await replayCommand<{ id: string; version: number }>(
      tx,
      a,
      key,
      "legal.revision",
      original.id,
      raw,
    );
    if (prior) return prior;
    if (!["contract", "rule", "template"].includes(original.kind))
      fail("Revisions are available for contracts, rules and templates.");
    if (original.version !== x.expectedVersion)
      fail("The source changed. Refresh first.", "LEGAL_VERSION_CONFLICT", 409);
    const created = await legalCommand(
      tx,
      a,
      {
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
      },
      `${key}:create`,
    );
    await legalCommand(
      tx,
      a,
      {
        action: "link",
        id: original.id,
        expectedVersion: original.version,
        targetId: created.id,
        relation: "amends",
      },
      `${key}:link`,
    );
    return executeCommand(
      tx,
      a,
      key,
      "legal.revision",
      raw,
      original.id,
      original.version + 1,
      created,
      [],
    );
  });
}
export async function ruleImpact(db: SqlDatabase, a: WorkContext, id: string) {
  const rule = await access(db, a, id);
  if (rule.kind !== "rule") fail("Choose a jurisdiction rule.");
  const v = visibility(a);
  return {
    ruleId: id,
    ruleVersion: rule.version,
    items: (
      await db
        .prepare(
          `SELECT i.id,i.kind,i.title,i.state,i.due_date,i.version FROM legal_items i JOIN legal_links l ON (l.source_id=? AND l.target_id=i.id) OR (l.target_id=? AND l.source_id=i.id) WHERE ${v.sql} ORDER BY i.id LIMIT 500`,
        )
        .bind(id, id, ...v.values)
        .all()
    ).results,
    notice:
      "Only accessible linked records are shown. Existing deadlines remain unchanged until their owner reviews and updates each record.",
  };
}
