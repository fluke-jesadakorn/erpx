import "server-only";
import { z } from "zod";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { requirePermission } from "../iam-permissions";
import { executeCommand, replayCommand, assertion } from "../commands";
import { samePerson } from "@/lib/iam";
import { dateSchema } from "@/lib/legal/model";
import { access, guards, event, lock, fail } from "./access";
export async function governanceView(
  db: SqlDatabase,
  a: WorkContext,
  id?: string,
) {
  requirePermission(a, "legal.read");
  if (id) await access(db, a, id);
  return {
    forms: (
      await db
        .prepare(
          "SELECT * FROM legal_intake_forms WHERE company_id=? ORDER BY name",
        )
        .bind(a.companyId)
        .all()
    ).results.map((r) => ({
      ...r,
      fields: typeof r.fields === "string" ? JSON.parse(r.fields) : r.fields,
    })),
    authorities: id
      ? (
          await db
            .prepare(
              "SELECT * FROM legal_authorities WHERE item_id=? ORDER BY id",
            )
            .bind(id)
            .all()
        ).results
      : [],
    settings: await db
      .prepare(
        "SELECT enabled,ai_enabled,reminder_days FROM legal_settings WHERE company_id=?",
      )
      .bind(a.companyId)
      .first(),
  };
}
export async function governanceCommand(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum([
        "form.create",
        "form.approve",
        "intake.apply",
        "authority.create",
        "authority.approve",
        "authority.revoke",
      ]),
      id: z.string().optional(),
      expectedVersion: z.number().int().positive().optional(),
      reason: z.string().trim().min(3).max(4000),
    })
    .parse(raw);
  const formAction = x.action.startsWith("form.");
  requirePermission(
    a,
    formAction
      ? "legal.manage"
      : x.action === "authority.approve"
        ? "legal.approve"
        : "legal.edit",
  );
  const item = formAction ? null : await access(db, a, z.string().parse(x.id));
  const aggregate = item?.id ?? a.companyId;
  const prior = await replayCommand(db, a, key, x.action, aggregate, raw);
  if (prior) return prior;
  const steps = item
    ? guards(
        db,
        a,
        item.id,
        z.number().parse(x.expectedVersion),
        key,
        x.action !== "authority.approve",
      )
    : [lock(db, a.companyId)];
  if (formAction && (a.branchId || a.departmentId))
    fail("Manage intake forms at Company scope.");
  if (x.action === "form.create") {
    const p = z
      .object({
        name: z.string().min(3).max(200),
        slaDays: z.coerce.number().int().min(1).max(365),
        fields: z
          .array(
            z.object({
              key: z.string().regex(/^[a-zA-Z][a-zA-Z0-9_]{0,39}$/),
              label: z.string().min(1).max(200),
              required: z.boolean(),
              type: z.enum(["text", "date", "number"]),
            }),
          )
          .min(1)
          .max(30),
      })
      .parse(raw);
    if (new Set(p.fields.map((f) => f.key)).size !== p.fields.length)
      fail("Field keys must be unique.");
    steps.push(
      db
        .prepare(
          "INSERT INTO legal_intake_forms(id,company_id,name,fields,sla_days,created_by) VALUES(?,?,?,?::jsonb,?,?)",
        )
        .bind(
          key,
          a.companyId,
          p.name,
          JSON.stringify(p.fields),
          p.slaDays,
          a.userId,
        ),
    );
  } else if (x.action === "form.approve") {
    const p = z
        .object({
          formId: z.string(),
          formVersion: z.number().int().positive(),
        })
        .parse(raw),
      f = await db
        .prepare("SELECT * FROM legal_intake_forms WHERE id=? AND company_id=?")
        .bind(p.formId, a.companyId)
        .first<{ created_by: string; state: string; version: number }>();
    if (!f || f.state !== "draft" || f.version !== p.formVersion)
      fail("Form changed.");
    if (samePerson(a, f.created_by))
      fail("An independent manager must approve this form.");
    steps.push(
      assertion(
        db,
        key,
        "form-current",
        "EXISTS(SELECT 1 FROM legal_intake_forms WHERE id=? AND version=? AND state='draft')",
        [p.formId, p.formVersion],
      ),
      db
        .prepare(
          "UPDATE legal_intake_forms SET state='approved',approved_by=?,version=version+1 WHERE id=?",
        )
        .bind(a.userId, p.formId),
    );
  } else if (x.action === "intake.apply") {
    if (
      item!.kind !== "request" ||
      !["draft", "submitted", "triage"].includes(item!.state)
    )
      fail("Apply an intake form while the request is being prepared.");
    const p = z
        .object({
          formId: z.string(),
          answers: z.record(z.string(), z.string().max(12000)),
          escalationOwner: z.string().optional(),
        })
        .parse(raw),
      f = await db
        .prepare(
          "SELECT fields,sla_days FROM legal_intake_forms WHERE id=? AND company_id=? AND state='approved'",
        )
        .bind(p.formId, a.companyId)
        .first<{ fields: string; sla_days: number }>();
    if (!f) fail("Choose an approved form.");
    const fields = JSON.parse(f.fields) as {
      key: string;
      required: boolean;
      type: string;
    }[];
    if (Object.keys(p.answers).some((k) => !fields.some((f) => f.key === k)))
      fail("Unknown intake field.");
    for (const field of fields) {
      const answer = p.answers[field.key] ?? "";
      if (field.required && !answer.trim()) fail(`Complete ${field.key}.`);
      if (answer && field.type === "date") dateSchema.parse(answer);
      if (answer && field.type === "number" && !/^\d+(\.\d+)?$/.test(answer))
        fail(`Invalid number for ${field.key}.`);
    }
    if (
      p.escalationOwner &&
      !(await db
        .prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?")
        .bind(item!.id, p.escalationOwner)
        .first())
    )
      fail("Escalation owner needs explicit access.");
    steps.push(
      db
        .prepare(
          "UPDATE legal_items SET intake_form_id=?,intake_answers=?::jsonb,sla_started_at=?,sla_due_at=floor(extract(epoch FROM ((clock_timestamp() AT TIME ZONE timezone + (? || ' days')::interval) AT TIME ZONE timezone))*1000)::bigint,escalation_owner=? WHERE id=?",
        )
        .bind(
          p.formId,
          JSON.stringify(p.answers),
          Date.now(),
          String(f.sla_days),
          p.escalationOwner ?? null,
          item!.id,
        ),
    );
  } else if (x.action === "authority.create") {
    if (!["contract", "corporate"].includes(item!.kind))
      fail("Record authority on a contract or corporate record.");
    const p = z
      .object({
        userId: z.string(),
        documentId: z.string(),
        currency: z.string().regex(/^[A-Z]{3}$/),
        limitMinor: z.coerce.number().int().min(0).max(Number.MAX_SAFE_INTEGER),
        validFrom: dateSchema,
        validUntil: dateSchema,
      })
      .parse(raw);
    if (p.validUntil < p.validFrom) fail("Authority validity is reversed.");
    if (
      !(await db
        .prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=?")
        .bind(p.documentId, item!.id)
        .first())
    )
      fail("Attach the authority evidence to this record first.");
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? UNION ALL SELECT 1 FROM legal_shares WHERE item_id=? AND user_id=? AND revoked_at IS NULL AND expires_at>?",
        )
        .bind(item!.id, p.userId, item!.id, p.userId, Date.now())
        .first())
    )
      fail("The signer needs explicit access.");
    steps.push(
      db
        .prepare(
          "INSERT INTO legal_authorities(id,item_id,company_id,user_id,document_id,currency,limit_minor,valid_from,valid_until,state,created_by) VALUES(?,?,?,?,?,?,?,?,?,'proposed',?)",
        )
        .bind(
          key,
          item!.id,
          a.companyId,
          p.userId,
          p.documentId,
          p.currency,
          p.limitMinor,
          p.validFrom,
          p.validUntil,
          a.userId,
        ),
    );
  } else {
    const p = z
        .object({
          authorityId: z.string(),
          authorityVersion: z.number().int().positive(),
        })
        .parse(raw),
      r = await db
        .prepare("SELECT * FROM legal_authorities WHERE id=? AND item_id=?")
        .bind(p.authorityId, item!.id)
        .first<{ created_by: string; state: string; version: number }>();
    if (!r || r.version !== p.authorityVersion) fail("Authority changed.");
    if (
      x.action === "authority.approve" &&
      (r.state !== "proposed" || samePerson(a, r.created_by))
    )
      fail("An independent reviewer must approve the proposed authority.");
    steps.push(
      assertion(
        db,
        key,
        "authority-current",
        "EXISTS(SELECT 1 FROM legal_authorities WHERE id=? AND version=?)",
        [p.authorityId, p.authorityVersion],
      ),
      db
        .prepare(
          "UPDATE legal_authorities SET state=?,approved_by=?,version=version+1 WHERE id=?",
        )
        .bind(
          x.action === "authority.approve" ? "approved" : "revoked",
          x.action === "authority.approve" ? a.userId : null,
          p.authorityId,
        ),
    );
  }
  if (item)
    steps.push(
      db
        .prepare(
          "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(Date.now(), item.id),
      event(db, a, item.id, x.action, x.reason, item.version + 1),
    );
  return executeCommand(
    db,
    a,
    key,
    x.action,
    raw,
    aggregate,
    item ? item.version + 1 : 1,
    { id: aggregate, version: item ? item.version + 1 : 1 },
    steps,
  );
}
