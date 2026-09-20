import "server-only";
import { z } from "zod";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { getOrder } from "../commerce";
import { directoryPerson } from "../people-directory";
import { getEmployeeRequest } from "../employee-requests";
import { getExpense } from "../expenses";
import { requirePermission } from "../iam-permissions";
import { access, guards, event } from "./access";
import { executeCommand, replayCommand, assertion } from "../commands";
const targetKinds = [
  "commerce_order",
  "person",
  "employee_request",
  "expense",
] as const;
async function target(
  db: SqlDatabase,
  a: WorkContext,
  kind: (typeof targetKinds)[number],
  id: string,
) {
  let row: Record<string, unknown>;
  let path: string;
  if (kind === "person") {
    requirePermission(a, "people.read");
    row = await directoryPerson(a, id);
    path = `people/${encodeURIComponent(id)}`;
  } else if (kind === "commerce_order") {
    row = (await getOrder(a, id)).order;
    path = `commerce/orders/${encodeURIComponent(id)}`;
  } else if (kind === "employee_request") {
    row = (await getEmployeeRequest(db, a, id)) as unknown as Record<
      string,
      unknown
    >;
    path = `requests/${encodeURIComponent(id)}`;
  } else {
    row = (await getExpense(a, id)) as unknown as Record<string, unknown>;
    path = `expenses/${encodeURIComponent(id)}`;
  }
  return {
    kind,
    id,
    label: String(row.title ?? row.legal_name ?? row.order_number ?? id),
    version: Number(row.version ?? 0),
    path,
  };
}
export async function legalReferences(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
) {
  await access(db, a, id);
  const rows = (
    await db
      .prepare(
        "SELECT target_module,target_id FROM legal_native_references WHERE item_id=? ORDER BY ordinal LIMIT 50",
      )
      .bind(id)
      .all<{ target_module: (typeof targetKinds)[number]; target_id: string }>()
  ).results;
  const visible = [];
  for (const r of rows) {
    try {
      visible.push(await target(db, a, r.target_module, r.target_id));
    } catch {
      /* An inaccessible target does not reveal its existence or label. */
    }
  }
  return visible;
}
export async function addLegalReference(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
      .object({
        id: z.string(),
        expectedVersion: z.number().int().positive(),
        targetKind: z.enum(targetKinds),
        targetId: z.string(),
      })
      .parse(raw),
    item = await access(db, a, x.id, "legal.edit");
  const prior = await replayCommand(
    db,
    a,
    key,
    "legal.reference",
    item.id,
    raw,
  );
  if (prior) return prior;
  await target(db, a, x.targetKind, x.targetId);
  return executeCommand(
    db,
    a,
    key,
    "legal.reference",
    raw,
    item.id,
    item.version + 1,
    { id: item.id, version: item.version + 1 },
    [
      ...guards(db, a, item.id, x.expectedVersion, key, true),
      assertion(
        db,
        key,
        "reference-limit",
        "(SELECT count(*) FROM legal_native_references WHERE item_id=?)<50",
        [item.id],
      ),
      db
        .prepare(
          "INSERT INTO legal_native_references(item_id,target_module,target_id,ordinal,created_by,created_at) VALUES(?,?,?,(SELECT coalesce(max(ordinal),-1)+1 FROM legal_native_references WHERE item_id=?),?,?) ON CONFLICT DO NOTHING",
        )
        .bind(item.id, x.targetKind, x.targetId, item.id, a.userId, Date.now()),
      db
        .prepare(
          "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(Date.now(), item.id),
      event(
        db,
        a,
        item.id,
        "reference.added",
        `${x.targetKind}:${x.targetId}`,
        item.version + 1,
      ),
    ],
  );
}
