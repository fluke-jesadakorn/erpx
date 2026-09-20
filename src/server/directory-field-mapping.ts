import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import type { PoolClient } from "pg";
import type { PostgresDatabase } from "./runtime/postgres";
import type { IamActor } from "./iam-context";
import { assertCompanyScope } from "./iam-context";
import { iamDb } from "./iam-db";
import { iamCommand } from "./iam-transactions";
import { requirePermission } from "./iam-permissions";
import {
  attributeAccess,
  validateAttributeValue,
  type AttributeDefinition,
} from "@/lib/user-management";
import { managementCommand } from "./user-management";
import {
  previewDirectoryChange,
  submitDirectoryChange,
} from "./directory-operations";
import {
  directoryChangeInput,
  type DirectoryInventory,
} from "@/lib/directory-operations";
import { ApiError } from "./http";
export const fieldSyncInput = z.object({
  connectionId: z.string().min(1),
  personId: z.string().min(1),
  direction: z.enum(["read", "write"]),
  reason: z.string().trim().min(3).max(1000),
  previewToken: z.string().optional(),
});
type Input = z.infer<typeof fieldSyncInput>;
async function plan(c: PoolClient, actor: IamActor, input: Input) {
  assertCompanyScope(actor);
  if (actor.platform)
    throw new ApiError(403, "COMPANY_REQUIRED", "Choose a Company.");
  requirePermission(actor, "iam.directory.manage");
  const state = (
    await c.query(
      "SELECT s.*,c.version connection_version FROM people_directory_state s JOIN iam_connections c ON c.id=s.connection_id WHERE s.connection_id=$1 AND s.company_id=$2 AND c.status='active'",
      [input.connectionId, actor.companyId],
    )
  ).rows[0];
  if (!state?.inventory || state.tested_version !== state.connection_version)
    throw new ApiError(
      409,
      "DIRECTORY_TEST_REQUIRED",
      "Read the current connector schema.",
    );
  const identities = (
    await c.query(
      "SELECT i.* FROM iam_identities i WHERE i.connection_id=$1 AND i.company_id=$2 AND i.user_id IS NOT NULL AND people_person_user($2,$3,i.user_id) AND EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=i.company_id AND m.user_id=i.user_id AND m.identity_id=i.id AND m.status='active')",
      [input.connectionId, actor.companyId, input.personId],
    )
  ).rows;
  if (identities.length !== 1)
    throw new ApiError(
      409,
      "VERIFIED_IDENTITY_REQUIRED",
      "Choose a person with one explicitly verified account for this connector.",
    );
  const inventory = state.inventory as DirectoryInventory,
    external = inventory.objects.find(
      (o) => o.id === identities[0].external_id,
    );
  if (!external)
    throw new ApiError(
      409,
      "DIRECTORY_OBJECT_MISSING",
      "Refresh the linked directory account.",
    );
  const person = (
    await c.query(
      "SELECT legal_name FROM hr_people WHERE id=$1 AND company_id=$2 AND merged_into_id IS NULL",
      [input.personId, actor.companyId],
    )
  ).rows[0];
  if (!person) throw new ApiError(404, "PERSON_NOT_FOUND", "Person not found.");
  const stored = (
    await c.query(
      "SELECT * FROM um_values WHERE company_id=$1 AND target='person' AND object_id=$2",
      [actor.companyId, input.personId],
    )
  ).rows[0];
  const values: Record<string, unknown> = {},
    fields: Record<string, string | boolean> = {},
    changes: { label: string; from: unknown; to: unknown }[] = [];
  for (const mapping of state.definition.mappings ?? []) {
    if (mapping.direction !== input.direction) continue;
    const row = (
      await c.query(
        "SELECT * FROM um_attributes WHERE company_id=$1 AND id=$2",
        [actor.companyId, mapping.attributeId],
      )
    ).rows[0];
    if (!row || row.definition.archived)
      throw new ApiError(
        409,
        "MAPPING_FIELD_CHANGED",
        "A mapped field changed.",
      );
    const attribute = {
        ...row.definition,
        id: row.id,
        version: row.version,
      } as AttributeDefinition,
      access = attributeAccess(attribute, actor.role, actor.scopeKind);
    if (!access.canView || (input.direction === "read" && !access.canEdit))
      throw new ApiError(
        403,
        "ATTRIBUTE_WRITE_FORBIDDEN",
        "Your role cannot synchronize this owned field.",
      );
    if (
      !inventory.fields.some(
        (f) => f.name === mapping.field && f[input.direction],
      )
    )
      throw new ApiError(
        409,
        "MAPPING_SCHEMA_CHANGED",
        "The connector no longer supports the mapping.",
      );
    const from =
      input.direction === "read"
        ? external.properties[mapping.field]
        : stored?.definition[mapping.attributeId];
    const previous =
      input.direction === "read"
        ? stored?.definition[mapping.attributeId]
        : external.properties[mapping.field];
    const checked = validateAttributeValue(attribute, from);
    if (input.direction === "read") values[attribute.id] = checked;
    else
      fields[mapping.field] =
        checked === null ? "" : (checked as string | boolean);
    changes.push({
      label: attribute.label,
      from: previous ?? null,
      to: checked,
    });
  }
  if (!changes.length)
    throw new ApiError(
      422,
      "MAPPING_EMPTY",
      "Configure fields for this direction first.",
    );
  const token = createHash("sha256")
    .update(
      JSON.stringify({
        input: { ...input, previewToken: undefined },
        stateVersion: state.version,
        inventory,
        identity: identities[0].id,
        stored,
        changes,
      }),
    )
    .digest("hex");
  return {
    token,
    personName: person.legal_name,
    changes,
    values,
    fields,
    valuesVersion: stored?.version ?? 0,
    externalId: external.id,
    connectionVersion: state.connection_version,
  };
}
export async function mappedFields(
  db: PostgresDatabase,
  actor: IamActor,
  input: Input,
  key: string,
  previewOnly: boolean,
) {
  const c = await iamDb(db).pool.connect();
  let p: Awaited<ReturnType<typeof plan>>;
  try {
    await c.query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY");
    p = await plan(c, actor, input);
  } finally {
    await c.query("ROLLBACK");
    c.release();
  }
  if (previewOnly)
    return {
      previewToken: p.token,
      personName: p.personName,
      changes: p.changes,
    };
  if (input.previewToken !== p.token)
    throw new ApiError(
      409,
      "MAPPING_PREVIEW_CHANGED",
      "Review the latest field changes.",
    );
  if (input.direction === "read") {
    requirePermission(actor, "people.manage");
    const change = {
      operation: "values",
      input: {
        target: "person",
        objectId: input.personId,
        expectedVersion: p.valuesVersion,
        values: p.values,
        reason: input.reason,
      },
    };
    const prepared = await managementCommand(
      db,
      actor,
      change,
      key + ":preview",
      true,
    );
    await iamCommand(
      db,
      actor,
      key + ":authorize",
      "directory.mapping.authorize",
      input,
      "iam.directory.manage",
      async (tx) => {
        const current = await plan(tx, actor, input);
        if (current.token !== p.token)
          throw new ApiError(
            409,
            "MAPPING_PREVIEW_CHANGED",
            "Directory mappings changed.",
          );
        return { ok: true };
      },
    );
    return managementCommand(
      db,
      actor,
      change,
      key,
      false,
      prepared.previewToken,
    );
  }
  const change = directoryChangeInput.parse({
    connectionId: input.connectionId,
    expectedVersion: p.connectionVersion,
    reason: input.reason,
    action: {
      kind: "account.update",
      objectId: p.externalId,
      fields: p.fields,
    },
  });
  const impact = await previewDirectoryChange(db, actor, change);
  return submitDirectoryChange(db, actor, change, impact.token, key);
}
