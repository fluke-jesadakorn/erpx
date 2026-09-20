import "server-only";
import { z } from "zod";
import {
  featureSpecs,
  featureKinds,
  parseFeature,
  type FeatureRow,
  type FeatureSpec,
} from "@/lib/legal/features";
import type { WorkContext } from "../authorization";
import type { SqlDatabase, SqlStatement } from "../runtime/types";
import { access, guards, event, eligible, fail } from "./access";
import { executeCommand, replayCommand } from "../commands";
import { requirePermission } from "../iam-permissions";
import { samePerson } from "@/lib/iam";
export async function featureDetail(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
  query: Record<string, unknown> = {},
) {
  const item = await access(db, a, id);
  const q = z
    .object({
      category: z.enum(featureKinds).optional(),
      page: z.coerce.number().int().min(1).max(10000).default(1),
      search: z.string().max(200).default(""),
    })
    .parse(query);
  const groups = await Promise.all(
    Object.entries(featureSpecs)
      .filter(
        ([category, s]) =>
          (s.kinds as string[]).includes(item.kind) &&
          (!q.category || q.category === category),
      )
      .map(async ([category, s]) => ({
        category,
        page: q.page,
        total: Number(
          await db
            .prepare(
              `SELECT count(*) n FROM ${s.table} WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)`,
            )
            .bind(id, q.search, q.search)
            .first("n"),
        ),
        rows: (
          await db
            .prepare(
              `SELECT * FROM ${s.table} WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0) ORDER BY updated_at DESC,id LIMIT 50 OFFSET ?`,
            )
            .bind(id, q.search, q.search, (q.page - 1) * 50)
            .all<FeatureRow>()
        ).results.map((r) => ({
          ...r,
          data: typeof r.data === "string" ? JSON.parse(r.data) : r.data,
        })),
      })),
  );
  return groups;
}
export async function featureCommand(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum([
        "feature.create",
        "feature.update",
        "feature.transition",
      ]),
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      category: z.enum(featureKinds),
      featureId: z.string().optional(),
      featureVersion: z.number().int().positive().optional(),
      state: z.string().optional(),
      reason: z.string().max(4000).default(""),
      data: z.record(z.string(), z.string().max(12000)).optional(),
    })
    .parse(raw);
  const spec: FeatureSpec = featureSpecs[x.category],
    item = await access(db, a, x.id);
  const prior = await replayCommand<{ id: string; version: number }>(
    db,
    a,
    key,
    x.action,
    item.id,
    raw,
  );
  if (prior) return prior;
  if (!(spec.kinds as string[]).includes(item.kind))
    fail("This workflow does not belong to this register.");
  const row = x.featureId
    ? await db
        .prepare(`SELECT * FROM ${spec.table} WHERE id=? AND item_id=?`)
        .bind(x.featureId, item.id)
        .first<FeatureRow>()
    : null;
  if (row && typeof row.data === "string") row.data = JSON.parse(row.data);
  const ownerResponse =
    !!row &&
    samePerson(a, row.owner_id) &&
    ["intake", "policy_ack"].includes(x.category) &&
    x.action !== "feature.create";
  if (!ownerResponse) await access(db, a, item.id, "legal.edit");
  const steps: SqlStatement[] = guards(
    db,
    a,
    item.id,
    x.expectedVersion,
    key,
    !ownerResponse,
  );
  let snapshot: FeatureRow;
  if (x.action === "feature.create") {
    const input = parseFeature(raw);
    await eligible(db, a, input.ownerId);
    if (
      !(await db
        .prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?")
        .bind(item.id, input.ownerId)
        .first())
    )
      fail("Grant explicit record access to the workflow owner first.");
    snapshot = {
      id: crypto.randomUUID(),
      item_id: item.id,
      category: x.category,
      title: input.title,
      owner_id: input.ownerId,
      due_date: input.dueDate,
      state: spec.initial,
      version: 1,
      data: input.data,
      created_by: a.userId,
      updated_by: a.userId,
      updated_at: Date.now(),
    };
    if (
      ["custody", "policy_ack"].includes(x.category) &&
      !(await db
        .prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=?")
        .bind(input.data.documentId, item.id)
        .first())
    )
      fail("Choose an immutable document version on this record.");
    if (
      x.category === "counsel_entry" &&
      !["conflict", "time", "fee", "invoice"].includes(input.data.entryType)
    )
      fail("Choose conflict, time, fee or invoice.");
    if (
      x.category === "privacy_case" &&
      !["subject_request", "breach", "processing"].includes(input.data.caseType)
    )
      fail("Choose subject_request, breach or processing.");
    steps.push(
      db
        .prepare(
          `INSERT INTO ${spec.table}(id,item_id,category,title,owner_id,due_date,state,version,data,created_by,updated_by,updated_at) VALUES(?,?,?,?,?,?,?,1,?::jsonb,?,?,?)`,
        )
        .bind(
          snapshot.id,
          item.id,
          x.category,
          snapshot.title,
          snapshot.owner_id,
          snapshot.due_date,
          snapshot.state,
          JSON.stringify(snapshot.data),
          a.userId,
          a.userId,
          snapshot.updated_at,
        ),
    );
  } else {
    if (!row || row.version !== x.featureVersion)
      fail(
        "Workflow changed. Refresh before saving.",
        "LEGAL_VERSION_CONFLICT",
        409,
      );
    const data = { ...row.data, ...x.data };
    parseFeature({
      category: x.category,
      title: row.title,
      ownerId: row.owner_id,
      dueDate: row.due_date,
      data,
    });
    if (
      ownerResponse &&
      x.data &&
      Object.keys(x.data).some(
        (k) =>
          !["response", "evidence"].includes(k) && x.data![k] !== row.data[k],
      )
    )
      fail("Only the response and acknowledgement may be changed.");
    if (
      x.action === "feature.update" &&
      ![
        "draft",
        "planned",
        "preparing",
        "open",
        "remediation",
        "in_progress",
        "received",
        "assessed",
        "requested",
        "pending",
      ].includes(row.state)
    )
      fail("Return this workflow for changes before editing it.");
    const state =
      x.action === "feature.transition" ? z.string().parse(x.state) : row.state;
    if (x.action === "feature.transition") {
      if (!spec.states[row.state]?.includes(state))
        fail(
          "This workflow transition is unavailable.",
          "LEGAL_TRANSITION",
          409,
        );
      if (spec.evidenceStates.includes(state) && !data.evidence?.trim())
        fail("Evidence is required for this decision.");
      if (
        x.category === "intake" &&
        state === "responded" &&
        !data.response?.trim()
      )
        fail("A response is required.");
      if (x.category === "policy_ack" && !samePerson(a, row.owner_id))
        fail("Only the assigned recipient can acknowledge this policy.");
      if (spec.independentStates.includes(state)) {
        requirePermission(a, "legal.approve");
        if (samePerson(a, row.created_by) || samePerson(a, row.updated_by))
          fail(
            "An independent reviewer must decide.",
            "LEGAL_INDEPENDENT_REVIEW",
            403,
          );
      }
      if (
        x.category === "counsel_entry" &&
        data.entryType !== "conflict" &&
        state === "approved"
      ) {
        if (
          !(await db
            .prepare(
              "SELECT 1 FROM legal_counsel_entries WHERE item_id=? AND data->>'entryType'='conflict' AND state IN ('approved','closed')",
            )
            .bind(item.id)
            .first())
        )
          fail("Clear the conflict review before approving fees or time.");
        const total = await db
          .prepare(
            "SELECT coalesce(sum(coalesce(nullif(data->>'amount',''),'0')::bigint),0) amount FROM legal_counsel_entries WHERE item_id=? AND id<>? AND state IN ('approved','billed','closed') AND data->>'entryType' IN ('fee','invoice')",
          )
          .bind(item.id, row.id)
          .first<{ amount: number }>();
        if (
          ["fee", "invoice"].includes(data.entryType) &&
          BigInt(total?.amount ?? 0) + BigInt(data.amount || 0) >
            BigInt(String(item.data.budget || 0))
        )
          fail(
            "Approved fees would exceed the counsel budget. Update and review the budget first.",
          );
      }
    }
    snapshot = {
      ...row,
      state,
      data,
      version: row.version + 1,
      updated_by: a.userId,
      updated_at: Date.now(),
    };
    steps.push(
      db
        .prepare(
          `UPDATE ${spec.table} SET state=?,data=?::jsonb,version=version+1,updated_by=?,updated_at=? WHERE id=? AND version=?`,
        )
        .bind(
          state,
          JSON.stringify(data),
          a.userId,
          snapshot.updated_at,
          row.id,
          row.version,
        ),
    );
  }
  steps.push(
    db
      .prepare(
        "INSERT INTO legal_feature_history(id,item_id,feature_id,category,version,snapshot,actor_id,reason,created_at) VALUES(?,?,?,?,?,?::jsonb,?,?,?)",
      )
      .bind(
        crypto.randomUUID(),
        item.id,
        snapshot.id,
        x.category,
        snapshot.version,
        JSON.stringify(snapshot),
        a.userId,
        x.reason,
        Date.now(),
      ),
    db
      .prepare(
        "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?",
      )
      .bind(Date.now(), item.id),
    event(
      db,
      a,
      item.id,
      x.action,
      `${spec.label}: ${snapshot.title} → ${snapshot.state}`,
      item.version + 1,
    ),
  );
  return executeCommand(
    db,
    a,
    key,
    x.action,
    raw,
    item.id,
    item.version + 1,
    { id: item.id, version: item.version + 1 },
    steps,
  );
}
