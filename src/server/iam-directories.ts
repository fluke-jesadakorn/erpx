import { iamDb } from "./iam-db";
import { emptyDirectoryHierarchy, validateDirectoryHierarchy } from "@/lib/hierarchy";
import "server-only";
import { z } from "zod";
import type { PoolClient } from "pg";
import type { DirectoryConfig, DirectoryProvider } from "@/lib/iam";
import { iamCommand } from "./iam-transactions";
import { assertCompanyScope, type IamActor } from "./iam-context";
import { requireFreshMfa } from "./iam-mfa";
import {
  proposeIamReview,
  insertIamGrant,
  grantInput,
  validateIamGrant,
} from "./iam-service";
import {
  readDirectory,
  testDirectory,
  type ProviderSnapshot,
} from "./iam-providers";
import { ApiError } from "./http";
import { holdingResources, runtimeEnv } from "./runtime";
import { PostgresDatabase } from "./runtime/postgres";

export const connectionInput = z
  .object({
    id: z.string().optional(),
    expectedVersion: z.number().int().nonnegative(),
    name: z.string().trim().min(2).max(100),
    provider: z.enum(["active_directory", "entra_id", "univention"]),
    config: z
      .object({
        mode: z.enum(["ldaps", "oidc"]),
        secretProfile: z.string().regex(/^[a-zA-Z0-9_-]{1,80}$/),
        authority: z.string().trim().min(3).max(200),
        ldapUrl: z.string().url().optional(),
        baseDn: z.string().min(1).optional(),
        groupBaseDn: z.string().optional(),
        userFilter: z.string().optional(),
        issuer: z.string().url().optional(),
        tenantId: z.string().uuid().optional(),
        clientId: z.string().optional(),
      })
      .strict(),
  })
  .strict();
export type ConnectionRow = {
  id: string;
  company_id: string;
  name: string;
  provider: DirectoryProvider;
  config: DirectoryConfig;
  status: string;
  version: number;
  tested_version: number | null;
  last_sync_at: number | null;
};
export async function connectionById(
  db: PostgresDatabase,
  companyId: string,
  id: string,
) {
  db = iamDb(db);
  const row = await db
    .prepare("SELECT * FROM iam_connections WHERE company_id=? AND id=?")
    .bind(companyId, id)
    .first<ConnectionRow>();
  if (!row)
    throw new ApiError(
      404,
      "CONNECTION_NOT_FOUND",
      "Directory connection not found.",
    );
  return row;
}
export async function routedConnection(id: string) {
  const env = await runtimeEnv();
  const route = await env.CONTROL_DB.prepare(
    "SELECT * FROM iam_connection_routes WHERE connection_id=?",
  )
    .bind(id)
    .first<{ holding_id: string | null; company_id: string }>();
  if (!route)
    throw new ApiError(
      404,
      "CONNECTION_NOT_FOUND",
      "Directory connection not found.",
    );
  const db = (
    route.holding_id
      ? (await holdingResources(route.holding_id)).write
      : env.CONTROL_DB
  ) as PostgresDatabase;
  return {
    db,
    holdingId: route.holding_id,
    connection: await connectionById(db, route.company_id, id),
  };
}
export async function saveConnection(
  db: PostgresDatabase,
  actor: IamActor,
  input: z.infer<typeof connectionInput>,
  key: string,
) {
  db = iamDb(db);
  assertCompanyScope(actor);
  if (
    (input.provider === "active_directory" && input.config.mode !== "ldaps") ||
    (input.provider === "entra_id" && input.config.mode !== "oidc")
  )
    throw new ApiError(
      422,
      "PROVIDER_MODE_INVALID",
      "Select a supported authentication method.",
    );
  if (input.provider === "entra_id") {
    if (!input.config.tenantId)
      throw new ApiError(
        422,
        "TENANT_REQUIRED",
        "Choose a concrete Entra tenant.",
      );
    input.config.issuer = `https://login.microsoftonline.com/${input.config.tenantId}/v2.0`;
  }
  if (
    input.provider !== "entra_id" &&
    (!input.config.ldapUrl?.startsWith("ldaps://") || !input.config.baseDn)
  )
    throw new ApiError(
      422,
      "LDAP_CONFIGURATION_REQUIRED",
      "LDAPS URL and base DN are required.",
    );
  const result = await iamCommand(
    db,
    actor,
    key,
    "connection.save",
    input,
    "iam.directory.manage",
    async (c) => {
      const id = input.id ?? crypto.randomUUID();
      const existing = (
        await c.query(
          "SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 FOR UPDATE",
          [id, actor.companyId],
        )
      ).rows[0];
      if (existing?.status === "active")
        throw new ApiError(
          409,
          "CONNECTION_ACTIVE",
          "Disable this connection before replacing its trust configuration.",
        );
      if (Number(existing?.version ?? 0) !== input.expectedVersion)
        throw new ApiError(
          409,
          "CONNECTION_CHANGED",
          "This connection changed. Refresh first.",
        );
      if (
        existing &&
        (existing.provider !== input.provider ||
          existing.config.authority !== input.config.authority)
      )
        throw new ApiError(
          409,
          "AUTHORITY_IMMUTABLE",
          "Create a new connection to change directory authority.",
        );
      if (existing)
        await c.query(
          "UPDATE iam_connections SET name=$1,config=$2,status='draft',version=version+1,tested_version=NULL,health='untested',last_error=NULL WHERE id=$3",
          [input.name, JSON.stringify(input.config), id],
        );
      else
        await c.query(
          "INSERT INTO iam_connections(id,company_id,name,provider,config,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
          [
            id,
            actor.companyId,
            input.name,
            input.provider,
            JSON.stringify(input.config),
            actor.userId,
            Date.now(),
          ],
        );
      return { id, version: input.expectedVersion + 1, status: "draft" };
    },
  );
  await (
    await runtimeEnv()
  ).CONTROL_DB.prepare(
    "INSERT INTO iam_connection_routes VALUES(?,?,?) ON CONFLICT(connection_id) DO NOTHING",
  )
    .bind(result.id, actor.holdingId, actor.companyId)
    .run();
  return result;
}
export async function testConnection(
  db: PostgresDatabase,
  actor: IamActor,
  id: string,
  version: number,
  key: string,
) {
  db = iamDb(db);
  assertCompanyScope(actor);
  const connection = await connectionById(db, actor.companyId, id);
  // Authorize before any outbound connection.
  await iamCommand(
    db,
    actor,
    `${key}:authorize`,
    "connection.test.authorize",
    { id, version },
    "iam.directory.manage",
    async () => ({ id }),
  );
  let result: Awaited<ReturnType<typeof testDirectory>>;
  try {
    result = await testDirectory(connection);
  } catch (error) {
    await db
      .prepare(
        "UPDATE iam_connections SET health='error',last_error=? WHERE id=? AND version=?",
      )
      .bind(
        error instanceof ApiError ? error.code : "DIRECTORY_TEST_FAILED",
        id,
        version,
      )
      .run();
    throw error instanceof ApiError
      ? error
      : new ApiError(
          502,
          "DIRECTORY_TEST_FAILED",
          "Connection test failed. Check endpoint, trust and credential profile.",
        );
  }
  return iamCommand(
    db,
    actor,
    key,
    "connection.test",
    { id, version },
    "iam.directory.manage",
    async (c) => {
      const update = await c.query(
        "UPDATE iam_connections SET tested_version=version,status=CASE WHEN status='active' THEN status ELSE 'tested' END,health='ready',last_error=NULL WHERE id=$1 AND company_id=$2 AND version=$3 RETURNING id",
        [id, actor.companyId, version],
      );
      if (!update.rowCount)
        throw new ApiError(
          409,
          "CONNECTION_CHANGED",
          "Configuration changed during testing.",
        );
      await c.query(
        "INSERT INTO iam_connection_previews VALUES($1,$2,$3,$4) ON CONFLICT(connection_id) DO UPDATE SET version=excluded.version,snapshot_json=excluded.snapshot_json,created_at=excluded.created_at",
        [id, version, JSON.stringify(result.snapshot), Date.now()],
      );
      return {
        id,
        userCount: result.userCount,
        groupCount: result.groupCount,
        status: "tested",
      };
    },
  );
}
export async function proposeConnectionActivation(
  db: PostgresDatabase,
  actor: IamActor,
  id: string,
  version: number,
  key: string,
) {
  db = iamDb(db);
  assertCompanyScope(actor);
  await requireFreshMfa(actor);
  return iamCommand(
    db,
    actor,
    key,
    "connection.activate.propose",
    { id, version },
    "iam.directory.manage",
    async (c) => {
      const r = (
        await c.query(
          "SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2",
          [id, actor.companyId],
        )
      ).rows[0];
      if (
        !r ||
        r.status !== "tested" ||
        Number(r.version) !== version ||
        Number(r.tested_version) !== version
      )
        throw new ApiError(
          409,
          "CONNECTION_TEST_REQUIRED",
          "Test the exact configuration before activation.",
        );
      return proposeIamReview(
        c,
        actor,
        "connection.activate",
        { id, version },
        "Activate tested directory connection.",
      );
    },
  );
}
export async function disableConnection(
  db: PostgresDatabase,
  actor: IamActor,
  id: string,
  version: number,
  key: string,
) {
  db = iamDb(db);
  assertCompanyScope(actor);
  await requireFreshMfa(actor);
  return iamCommand(
    db,
    actor,
    key,
    "connection.disable",
    { id, version },
    "iam.directory.manage",
    async (c) => {
      const changed = await c.query(
        "UPDATE iam_connections SET status='disabled',version=version+1,health='disabled' WHERE id=$1 AND company_id=$2 AND version=$3 RETURNING id",
        [id, actor.companyId, version],
      );
      if (!changed.rowCount)
        throw new ApiError(
          409,
          "CONNECTION_CHANGED",
          "This connection changed.",
        );
      await c.query(
        "UPDATE iam_members SET version=version+1 WHERE connection_id=$1 AND company_id=$2",
        [id, actor.companyId],
      );
      if (!actor.platform)
        await c.query(
          "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND user_id IN (SELECT user_id FROM iam_members WHERE connection_id=$2)",
          [actor.companyId, id],
        );
      return { id, status: "disabled" };
    },
  );
}

/** Only complete snapshots replace membership; credentials never enter the snapshot. */
export async function applyDirectorySnapshot(
  db: PostgresDatabase,
  connection: ConnectionRow,
  snapshot: ProviderSnapshot,
  observedAt = Date.now(),
) {
  db = iamDb(db);
  if (snapshot.complete !== true)
    throw new ApiError(
      502,
      "DIRECTORY_INCOMPLETE",
      "A complete directory snapshot is required.",
    );
  const now = observedAt,
    c = await db.pool.connect();
  try {
    await c.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
    const row = (
      await c.query("SELECT * FROM iam_connections WHERE id=$1 FOR UPDATE", [
        connection.id,
      ])
    ).rows[0];
    if (
      !row ||
      row.status !== "active" ||
      Number(row.version) !== Number(connection.version)
    )
      throw new ApiError(
        409,
        "CONNECTION_CHANGED",
        "The connection changed during synchronization.",
      );
    if (Number(row.last_sync_at) > now) {
      await c.query("COMMIT");
      return;
    }
    if (
      new Set(snapshot.users.map((u) => u.externalId)).size !==
        snapshot.users.length ||
      new Set(snapshot.groups.map((g) => g.externalId)).size !==
        snapshot.groups.length
    )
      throw new ApiError(
        502,
        "DIRECTORY_DUPLICATE_IDENTITY",
        "The directory returned duplicate immutable identifiers.",
      );
    const hierarchy = snapshot.hierarchy ?? emptyDirectoryHierarchy();
    validateDirectoryHierarchy(hierarchy, snapshot.users.map(u => u.externalId), snapshot.groups.map(g => g.externalId));
    if (snapshot.users.some(u => u.groupIds.some(id => !snapshot.groups.some(g => g.externalId === id))))
      throw new ApiError(502, "DIRECTORY_INCOMPLETE", "A membership refers to a missing directory group.");
    await c.query("INSERT INTO iam_directory_hierarchies(connection_id,hierarchy_json,observed_at) VALUES($1,$2,$3) ON CONFLICT(connection_id) DO UPDATE SET hierarchy_json=excluded.hierarchy_json,observed_at=excluded.observed_at", [connection.id, JSON.stringify(hierarchy), now]);
    for (const group of snapshot.groups)
      await c.query(
        "INSERT INTO iam_groups VALUES($1,$2,$3,$4) ON CONFLICT(connection_id,external_id) DO UPDATE SET name=excluded.name,observed_at=excluded.observed_at",
        [connection.id, group.externalId, group.name, now],
      );
    for (const user of snapshot.users)
      await c.query(
        "INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,email,enabled,groups_json,observed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(connection_id,external_id) DO UPDATE SET username=excluded.username,name=excluded.name,email=excluded.email,enabled=excluded.enabled,groups_json=excluded.groups_json,observed_at=excluded.observed_at",
        [
          crypto.randomUUID(),
          connection.company_id,
          connection.id,
          user.externalId,
          user.username,
          user.name,
          user.email,
          user.enabled,
          JSON.stringify(user.groupIds),
          now,
        ],
      );
    await c.query(
      "UPDATE iam_identities SET enabled=false,groups_json='[]',observed_at=$1 WHERE connection_id=$2 AND NOT (external_id=ANY($3::text[]))",
      [now, connection.id, snapshot.users.map((u) => u.externalId)],
    );
    await c.query(
      "DELETE FROM iam_groups WHERE connection_id=$1 AND NOT (external_id=ANY($2::text[]))",
      [connection.id, snapshot.groups.map((g) => g.externalId)],
    );
    const table =
      connection.company_id === "platform"
        ? "platform_assignments"
        : "assignments";
    await c.query(
      `UPDATE ${table} a SET revoked_at=$1,version=version+1 WHERE a.source='directory' AND a.revoked_at IS NULL AND a.source_id IN (SELECT id FROM iam_mappings WHERE connection_id=$2) AND NOT EXISTS(SELECT 1 FROM iam_mappings m JOIN iam_identities i ON i.connection_id=m.connection_id AND i.user_id=a.user_id WHERE m.id=a.source_id AND m.status='active' AND m.valid_until>$1 AND i.enabled AND i.groups_json @> jsonb_build_array(m.group_id) AND iam_assignment_active(m.parent_id,$1))`,
      [now, connection.id],
    );
    // Imports are pending identities. Mapping grants apply only after reviewed migration.
    const candidates = (
      await c.query(
        "SELECT m.*,i.user_id,i.person_id FROM iam_mappings m JOIN iam_identities i ON i.connection_id=m.connection_id JOIN iam_members u ON u.identity_id=i.id AND u.company_id=m.company_id AND u.user_id=i.user_id WHERE m.connection_id=$1 AND m.status='active' AND m.valid_until>$2 AND i.enabled AND i.groups_json @> jsonb_build_array(m.group_id) AND u.status='active' AND u.login_policy='directory' AND iam_assignment_active(m.parent_id,$2)",
        [connection.id, now],
      )
    ).rows;
    for (const m of candidates) {
      if (
        (
          await c.query(
            `SELECT 1 FROM ${table} WHERE user_id=$1 AND source='directory' AND source_id=$2`,
            [m.user_id, m.id],
          )
        ).rowCount
      )
        continue;
      const parent = (
        await c.query(`SELECT * FROM ${table} WHERE id=$1`, [m.parent_id])
      ).rows[0];
      if (Number(m.valid_until) > Number(parent.valid_until)) continue;
      const parentRole = (
        await c.query(
          "SELECT v.grantable_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2)",
          [parent.role, connection.company_id],
        )
      ).rows[0];
      const actor = {
        userId: parent.user_id,
        assignmentId: parent.id,
        scopeKind: parent.scope_kind ?? "platform",
        scopeNodeId: parent.scope_node_id ?? null,
        role: parent.role,
        companyId: connection.company_id,
        platform: connection.company_id === "platform",
        iam: { grantablePermissions: parentRole?.grantable_json ?? [] },
      } as IamActor;
      try {
        await validateIamGrant(
          c,
          actor,
          grantInput.parse({
            userId: m.user_id,
            roleId: m.role_id,
            scopeKind: m.scope_kind,
            scopeNodeId: m.scope_node_id,
            validUntil: Number(m.valid_until),
            reason: "Approved directory mapping",
          }),
          m.parent_id,
        );
      } catch (e) {
        if (e instanceof ApiError) continue;
        throw e;
      }
      await insertIamGrant(
        c,
        actor,
        grantInput.parse({
          userId: m.user_id,
          roleId: m.role_id,
          scopeKind: m.scope_kind,
          scopeNodeId: m.scope_node_id,
          validUntil: Number(m.valid_until),
          reason: "Approved directory mapping",
        }),
        m.parent_id,
        "directory",
        m.id,
      );
    }
    await c.query(
      "UPDATE iam_connections SET last_sync_at=$1,health='healthy',last_error=NULL WHERE id=$2",
      [now, connection.id],
    );
    await c.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
      crypto.randomUUID(),
      connection.company_id,
      "directory-sync",
      "system",
      "directory.synchronized",
      connection.id,
      JSON.stringify({
        users: snapshot.users.length,
        groups: snapshot.groups.length,
        version: connection.version,
      }),
      now,
    ]);
    await c.query("COMMIT");
  } catch (error) {
    await c.query("ROLLBACK");
    throw error;
  } finally {
    c.release();
  }
}
export async function synchronizeConnection(
  db: PostgresDatabase,
  connection: ConnectionRow,
) {
  db = iamDb(db);
  const startedAt = Date.now();
  try {
    await applyDirectorySnapshot(
      db,
      connection,
      await readDirectory(connection),
      startedAt,
    );
  } catch (error) {
    await db
      .prepare(
        "UPDATE iam_connections SET health='error',last_error=? WHERE id=? AND version=? AND COALESCE(last_sync_at,0)<=?",
      )
      .bind(
        error instanceof ApiError ? error.code : "DIRECTORY_SYNC_FAILED",
        connection.id,
        connection.version,
        startedAt,
      )
      .run();
    await db
      .prepare(
        "UPDATE iam_identities SET observed_at=LEAST(observed_at,?) WHERE connection_id=? AND EXISTS(SELECT 1 FROM iam_connections WHERE id=? AND version=? AND health='error' AND COALESCE(last_sync_at,0)<=?)",
      )
      .bind(
        Date.now() - 300000,
        connection.id,
        connection.id,
        connection.version,
        startedAt,
      )
      .run();
    throw error;
  }
}
export const mappingInput = z.object({
  expectedRoleRevision: z.number().int().positive().optional(),
  connectionId: z.string(),
  groupId: z.string(),
  roleId: z.string(),
  scopeKind: z.enum(["holding", "branch", "department", "platform"]),
  scopeNodeId: z.string().nullable().default(null),
  validUntil: z.number().int(),
  reason: z.string().min(3).max(1000),
});
export async function proposeMapping(
  db: PostgresDatabase,
  actor: IamActor,
  input: z.infer<typeof mappingInput>,
  key: string,
) {
  db = iamDb(db);
  await requireFreshMfa(actor);
  return iamCommand(
    db,
    actor,
    key,
    "mapping.propose",
    input,
    "iam.manage",
    async (c) => {
      const group = (
        await c.query(
          "SELECT 1 FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=$1 AND c.id=$2 AND g.external_id=$3",
          [actor.companyId, input.connectionId, input.groupId],
        )
      ).rowCount;
      if (!group) {
        const preview = (
          await c.query(
            "SELECT p.snapshot_json FROM iam_connection_previews p JOIN iam_connections c ON c.id=p.connection_id AND c.version=p.version WHERE c.id=$1 AND c.company_id=$2",
            [input.connectionId, actor.companyId],
          )
        ).rows[0];
        if (
          !preview?.snapshot_json.groups.some(
            (g: { externalId: string }) => g.externalId === input.groupId,
          )
        )
          throw new ApiError(
            404,
            "GROUP_NOT_FOUND",
            "Test and preview the selected group first.",
          );
      }
      const grant = grantInput.parse({
        ...input,
        userId: "directory-mapping-target",
      });
      await validateIamGrant(c, actor, grant);
      return proposeIamReview(
        c,
        actor,
        "mapping.activate",
        {
          ...input,
          expectedRoleRevision: grant.expectedRoleRevision,
          parentId: actor.assignmentId,
        },
        input.reason,
      );
    },
  );
}
export async function applyDirectoryReview(
  c: PoolClient,
  actor: IamActor,
  kind: string,
  p: Record<string, unknown>,
) {
  if (kind === "connection.activate") {
    const changed = await c.query(
      "UPDATE iam_connections SET status='active',health='awaiting_sync' WHERE id=$1 AND company_id=$2 AND version=$3 AND tested_version=version AND status='tested' RETURNING id",
      [p.id, actor.companyId, p.version],
    );
    if (!changed.rowCount)
      throw new ApiError(
        409,
        "CONNECTION_CHANGED",
        "Retest and propose the current configuration.",
      );
    await c.query(
      "INSERT INTO iam_sync_jobs(connection_id,next_run_at) VALUES($1,0) ON CONFLICT(connection_id) DO UPDATE SET next_run_at=0",
      [p.id],
    );
  } else if (kind === "mapping.activate") {
    const input = mappingInput.parse(p);
    if (
      (
        await c.query(
          "SELECT 1 FROM iam_identities WHERE company_id=$1 AND connection_id=$2 AND groups_json @> jsonb_build_array($3::text) AND user_id IS NOT NULL AND iam_same_person(user_id,$4,$1)",
          [actor.companyId, input.connectionId, input.groupId, actor.userId],
        )
      ).rowCount
    )
      throw new ApiError(
        403,
        "INDEPENDENT_REVIEW_REQUIRED",
        "A member benefiting from this group mapping cannot approve it.",
      );
    await validateIamGrant(
      c,
      actor,
      grantInput.parse({ ...input, userId: "directory-mapping-target" }),
      String(p.parentId),
    );
    await c.query(
      "INSERT INTO iam_mappings(id,company_id,connection_id,group_id,role_id,scope_kind,scope_node_id,valid_until,parent_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
      [
        crypto.randomUUID(),
        actor.companyId,
        input.connectionId,
        input.groupId,
        input.roleId,
        input.scopeKind,
        input.scopeNodeId,
        input.validUntil,
        p.parentId,
      ],
    );
  } else
    throw new ApiError(
      409,
      "REVIEW_UNSUPPORTED",
      "The review kind is not supported.",
    );
}
async function drainOneDirectoryJob(db: PostgresDatabase) {
  db = iamDb(db);
  const now = Date.now(),
    token = crypto.randomUUID();
  const row = await db
    .prepare(
      `UPDATE iam_sync_jobs SET lease_until=?,lease_token=?,attempts=attempts+1 WHERE connection_id=(SELECT j.connection_id FROM iam_sync_jobs j JOIN iam_connections c ON c.id=j.connection_id WHERE c.status='active' AND j.next_run_at<=? AND (j.lease_until IS NULL OR j.lease_until<?) ORDER BY j.next_run_at FOR UPDATE OF j SKIP LOCKED LIMIT 1) RETURNING connection_id`,
    )
    .bind(now + 120000, token, now, now)
    .first<{ connection_id: string }>();
  if (!row) return false;
  const connection = await db
    .prepare("SELECT * FROM iam_connections WHERE id=?")
    .bind(row.connection_id)
    .first<ConnectionRow>();
  if (!connection) return false;
  const renewal = setInterval(() => {
    void db
      .prepare(
        "UPDATE iam_sync_jobs SET lease_until=? WHERE connection_id=? AND lease_token=?",
      )
      .bind(Date.now() + 120000, connection.id, token)
      .run()
      .catch(() => {});
  }, 30000);
  try {
    await synchronizeConnection(db, connection);
  } catch {
    /* Bounded failure categories are persisted by synchronizeConnection. */
  } finally {
    clearInterval(renewal);
    await db
      .prepare(
        "UPDATE iam_sync_jobs SET next_run_at=?,lease_until=NULL,lease_token=NULL WHERE connection_id=? AND lease_token=?",
      )
      .bind(Date.now() + 300000, connection.id, token)
      .run();
  }
}

export async function drainDirectoryJobs(db: PostgresDatabase) {
  let remaining = 48;
  await Promise.all(
    Array.from({ length: 3 }, async () => {
      while (remaining-- > 0) {
        if ((await drainOneDirectoryJob(db)) === false) break;
      }
    }),
  );
}

export { refreshDirectoryMember } from "./iam-directory-refresh";
