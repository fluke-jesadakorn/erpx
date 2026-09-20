import "server-only";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import type { PostgresDatabase } from "./runtime/postgres";
import type { IamActor } from "./iam-context";
import { assertCompanyScope } from "./iam-context";
import { iamDb } from "./iam-db";
import { iamCommand } from "./iam-transactions";
import { requirePermission } from "./iam-permissions";
import { hasPermission } from "@/lib/permissions";
import {
  holdingTemplateInput,
  type PeopleTemplatesData,
} from "@/lib/people-templates";
import { ApiError } from "./http";

function scope(actor: IamActor) {
  if (actor.platform || !actor.holdingId)
    throw new ApiError(
      403,
      "HOLDING_REQUIRED",
      "Choose a Company within a Holding.",
    );
  assertCompanyScope(actor);
}
export async function peopleTemplatesData(
  db: PostgresDatabase,
  actor: IamActor,
): Promise<PeopleTemplatesData> {
  scope(actor);
  requirePermission(actor, "organization.manage");
  const rows = (
    await iamDb(db).pool.query(
      "SELECT t.*,a.template_version adopted_version FROM people_holding_templates t LEFT JOIN people_template_adoptions a ON a.template_id=t.id AND a.company_id=$1 WHERE t.holding_id=$2 ORDER BY t.name",
      [actor.companyId, actor.holdingId],
    )
  ).rows;
  return {
    templates: rows.map((r) => ({
      id: r.id,
      name: r.name,
      version: r.version,
      definition: r.definition,
      adoptedVersion: r.adopted_version ?? null,
    })),
    canPublish: hasPermission(actor, "iam.roles.manage"),
    canAdopt: hasPermission(actor, "organization.manage"),
  };
}
export async function saveHoldingTemplate(
  db: PostgresDatabase,
  actor: IamActor,
  raw: unknown,
  key: string,
) {
  scope(actor);
  const input = holdingTemplateInput.parse(raw);
  return iamCommand(
    db,
    actor,
    key,
    "people.template.publish",
    input,
    "iam.roles.manage",
    async (c) => {
      const id = input.id ?? randomUUID(),
        old = (
          await c.query(
            "SELECT * FROM people_holding_templates WHERE id=$1 AND holding_id=$2 FOR UPDATE",
            [id, actor.holdingId],
          )
        ).rows[0];
      if (Number(old?.version ?? 0) !== input.expectedVersion)
        throw new ApiError(
          409,
          "TEMPLATE_CHANGED",
          "Review the latest template before publishing.",
        );
      if (old)
        await c.query(
          "UPDATE people_holding_templates SET version=version+1,name=$1,definition=$2,updated_at=$3 WHERE id=$4",
          [input.name, JSON.stringify(input.definition), Date.now(), id],
        );
      else
        await c.query(
          "INSERT INTO people_holding_templates(id,holding_id,name,definition,created_by,updated_at) VALUES($1,$2,$3,$4,$5,$6)",
          [
            id,
            actor.holdingId,
            input.name,
            JSON.stringify(input.definition),
            actor.userId,
            Date.now(),
          ],
        );
      return { id, version: input.expectedVersion + 1 };
    },
  );
}
export const adoptTemplateInput = z.object({
  templateId: z.string().min(1),
  revision: z.number().int().positive(),
  reason: z.string().trim().min(3).max(1000),
  previewToken: z.string().optional(),
});
export async function adoptHoldingTemplate(
  db: PostgresDatabase,
  actor: IamActor,
  input: z.infer<typeof adoptTemplateInput>,
  key: string,
  preview: boolean,
) {
  scope(actor);
  requirePermission(actor, "organization.manage");
  const run = async (c: import("pg").PoolClient) => {
    const t = (
      await c.query(
        "SELECT * FROM people_holding_templates WHERE id=$1 AND holding_id=$2",
        [input.templateId, actor.holdingId],
      )
    ).rows[0];
    if (!t || t.version !== input.revision)
      throw new ApiError(
        409,
        "TEMPLATE_CHANGED",
        "Choose the current Holding template revision.",
      );
    const definition = holdingTemplateInput.shape.definition.parse(
      t.definition,
    );
    if (definition.attributes.length)
      requirePermission(actor, "iam.roles.manage");
    const settings = (
      await c.query(
        "SELECT * FROM people_lifecycle_settings WHERE company_id=$1 FOR UPDATE",
        [actor.companyId],
      )
    ).rows[0];
    const attributes = (
      await c.query(
        "SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY id",
        [actor.companyId],
      )
    ).rows;
    const stamp = createHash("sha256")
      .update(JSON.stringify({ template: t, settings, attributes }))
      .digest("hex");
    const result = {
      previewToken: stamp,
      onboardingTasks: definition.onboardingTasks,
      offboardingTasks: definition.offboardingTasks,
      attributes: definition.attributes.map((a) => a.label),
      message:
        "Existing values and personal grants are retained. New templates affect future HR/IT work.",
    };
    if (preview) return result;
    if (input.previewToken !== stamp)
      throw new ApiError(
        409,
        "TEMPLATE_PREVIEW_CHANGED",
        "The Company configuration changed after preview.",
      );
    for (const field of definition.attributes) {
      const id = "template:" + t.id + ":" + field.key,
        old = attributes.find((a) => a.id === id);
      for (const role of new Set([
        ...(field.readRoles ?? []),
        ...(field.writeRoles ?? []),
        ...(field.ownerRole ? [field.ownerRole] : []),
      ]))
        if (
          !(
            await c.query(
              "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'",
              [role, actor.companyId],
            )
          ).rowCount
        )
          throw new ApiError(
            422,
            "ATTRIBUTE_ROLE_INVALID",
            "This Company must have the field's owner and access roles before adoption.",
          );
      if (
        old &&
        (field.archived ||
          JSON.stringify(old.definition.options) !==
            JSON.stringify(field.options)) &&
        (
          await c.query(
            "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x WHERE x->>'field'=$2)",
            [actor.companyId, "custom:" + id],
          )
        ).rowCount
      )
        throw new ApiError(
          409,
          "TEMPLATE_FIELD_IN_USE",
          "Update active group rules before changing this template field.",
        );
      if (
        old &&
        (old.definition.type !== field.type ||
          old.definition.target !== field.target ||
          old.definition.options.some(
            (o: string) => !field.options.includes(o),
          ))
      )
        throw new ApiError(
          409,
          "TEMPLATE_FIELD_INCOMPATIBLE",
          "Retain the field type, target and existing choices when updating a shared template.",
        );
      const { key: stableKey, ...config } = field;
      void stableKey;
      await c.query(
        "INSERT INTO um_attributes(company_id,id,version,definition) VALUES($1,$2,1,$3) ON CONFLICT(company_id,id) DO UPDATE SET version=um_attributes.version+1,definition=excluded.definition",
        [actor.companyId, id, JSON.stringify(config)],
      );
      await c.query(
        "INSERT INTO um_history(company_id,entity,object_id,version,definition,created_at) VALUES($1,'attribute',$2,$3,$4,$5)",
        [
          actor.companyId,
          id,
          (old?.version ?? 0) + 1,
          JSON.stringify(config),
          Date.now(),
        ],
      );
    }
    const next = {
      ...(settings?.definition ?? { itAssigneeId: null }),
      onboardingTasks: definition.onboardingTasks,
      offboardingTasks: definition.offboardingTasks,
    };
    await c.query(
      "INSERT INTO people_lifecycle_settings(company_id,version,definition) VALUES($1,1,$2) ON CONFLICT(company_id) DO UPDATE SET version=people_lifecycle_settings.version+1,definition=excluded.definition",
      [actor.companyId, JSON.stringify(next)],
    );
    await c.query(
      "INSERT INTO people_template_adoptions(company_id,template_id,template_version,definition) VALUES($1,$2,$3,$4) ON CONFLICT(company_id,template_id) DO UPDATE SET template_version=excluded.template_version,version=people_template_adoptions.version+1,definition=excluded.definition",
      [actor.companyId, t.id, t.version, JSON.stringify(definition)],
    );
    return result;
  };
  if (!preview)
    return iamCommand(
      db,
      actor,
      key,
      "people.template.adopt",
      input,
      "organization.manage",
      run,
    );
  const c = await iamDb(db).pool.connect();
  try {
    await c.query("BEGIN ISOLATION LEVEL REPEATABLE READ");
    return await run(c);
  } finally {
    await c.query("ROLLBACK");
    c.release();
  }
}
