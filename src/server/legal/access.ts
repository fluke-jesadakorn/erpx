import "server-only";
import type { WorkContext } from "../authorization";
import type { SqlDatabase, SqlStatement } from "../runtime/types";
import { requirePermission } from "../iam-permissions";
import { ApiError } from "../http";
import { assertion } from "../commands";
import type { LegalItem, LegalKind } from "@/lib/legal/model";
export const tables: Record<LegalKind, string> = {
  request: "legal_requests",
  matter: "legal_matters",
  contract: "legal_contracts",
  obligation: "legal_obligations",
  compliance: "legal_compliance",
  license: "legal_licenses",
  corporate: "legal_corporate",
  ip: "legal_ip_assets",
  privacy: "legal_privacy",
  counsel: "legal_counsel",
  template: "legal_templates",
  rule: "legal_rules",
};
export function fail(
  message: string,
  code = "LEGAL_INVALID",
  status = 422,
): never {
  throw new ApiError(status, code, message);
}
export function visibility(a: WorkContext, alias = "i") {
  return {
    sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?) AND (${alias}.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=${alias}.id AND lm.user_id=?))`,
    values: [
      a.companyId,
      a.branchId,
      a.branchId,
      a.departmentId,
      a.departmentId,
      a.userId,
    ],
  };
}
export async function enabled(db: SqlDatabase, a: WorkContext) {
  return !!(await db
    .prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND enabled=true")
    .bind(a.companyId)
    .first());
}
export async function access(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
  permission = "legal.read",
) {
  requirePermission(a, permission);
  const v = visibility(a);
  const r = await db
    .prepare(`SELECT i.* FROM legal_items i WHERE i.id=? AND ${v.sql}`)
    .bind(id, ...v.values)
    .first<Omit<LegalItem, "data">>();
  if (!r)
    fail(
      "Legal record is unavailable in your current scope.",
      "LEGAL_NOT_FOUND",
      404,
    );
  if (permission !== "legal.read" && !(await enabled(db, a)))
    fail(
      "Legal is read-only until enabled for this Company.",
      "LEGAL_DISABLED",
      409,
    );
  if (
    permission === "legal.edit" &&
    !(await db
      .prepare(
        "SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? AND access='edit'",
      )
      .bind(id, a.userId)
      .first())
  )
    fail("An editor membership is required.", "LEGAL_EDITOR_REQUIRED", 403);
  const row = await db
    .prepare(`SELECT data FROM ${tables[r.kind]} WHERE item_id=?`)
    .bind(id)
    .first<{ data: LegalItem["data"] }>();
  return {
    ...r,
    data:
      typeof row?.data === "string" ? JSON.parse(row.data) : (row?.data ?? {}),
  } as LegalItem;
}
export function lock(db: SqlDatabase, company: string) {
  return db
    .prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")
    .bind(`legal:${company}`);
}
export function guards(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
  version: number,
  key: string,
  edit = false,
  allowFrozen = false,
): SqlStatement[] {
  const v = visibility(a);
  return [
    db.prepare(
      "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))",
    ),
    lock(db, a.companyId),
    ...(!allowFrozen
      ? [
          assertion(
            db,
            key,
            "legal-content-available",
            "NOT EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=? AND state IN ('executing','completed'))",
            [id],
          ),
        ]
      : []),
    assertion(
      db,
      key,
      "legal-enabled",
      "EXISTS(SELECT 1 FROM legal_settings WHERE company_id=? AND enabled=true)",
      [a.companyId],
    ),
    assertion(
      db,
      key,
      "legal-access",
      `EXISTS(SELECT 1 FROM legal_items i WHERE i.id=? AND i.version=? AND ${v.sql}${edit ? " AND EXISTS(SELECT 1 FROM legal_members em WHERE em.item_id=i.id AND em.user_id=? AND em.access='edit')" : ""})`,
      [id, version, ...v.values, ...(edit ? [a.userId] : [])],
    ),
  ];
}
export async function eligible(db: SqlDatabase, a: WorkContext, id: string) {
  if (
    !(await db
      .prepare(
        "SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? LIMIT 1",
      )
      .bind(a.companyId, id, Date.now(), Date.now())
      .first())
  )
    fail(
      "Choose a person with current Company access.",
      "LEGAL_PERSON_UNAVAILABLE",
    );
}
export function event(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
  action: string,
  note: string,
  version: number,
) {
  return db
    .prepare(
      "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)",
    )
    .bind(crypto.randomUUID(), id, action, a.userId, note, version, Date.now());
}
