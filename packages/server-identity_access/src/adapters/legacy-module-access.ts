import "server-only";
import { createHash } from "node:crypto";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { identitySnapshotSources } from "@erpx/server-organization/public/identity-snapshot-sources";
import { readIamAuthorityFacts } from "@erpx/server-identity_access/adapters/iam-store";
import { betaAccess } from "@erpx/server-identity_access/adapters/beta-access";
import { effectiveWorkModules } from "@erpx/server-identity_access/domain/work-modules";
import { legacyModuleCompatibility } from "@erpx/server-identity_access/domain/legacy-module-compatibility";
import { effectiveAuthorityPermissions } from "@erpx/server-identity_access/application/authority";
import type { LegacyCompanyModuleAccess, LegacyModuleWitness, LegacyModuleAccessLease } from "@erpx/server-identity_access/domain/legacy-module-access";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { lockLegacyOrganizationAccess, nextLegacyOrganizationTransition } from "@erpx/server-organization/public/commercial-companies";
import { lockLegacyPeopleAccess, nextLegacyPeopleTransition } from "@erpx/server-people_payroll/public/legacy-license-access";
import { lockLegacyBetaAccess } from "@erpx/server-manage_erpx/public/legacy-license-access";
import {legacyRoutineAccess,lockLegacyRoutineAccess,nextLegacyRoutineTransition} from '@erpx/server-operations/public/legacy-routine-access';
const org = identitySnapshotSources();

/** Must precede all source reads in the migration's owner transaction. Table
 * locks cover newly inserted assignments/members as well as existing witnesses.
 * This is a rare provider migration, never a normal workspace read path. */
export async function lockLegacyModuleAccess(database: SqlDatabase) {
  await database.prepare("SET LOCAL lock_timeout='5s'").run();
  await database.prepare(`LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,
    iam_access_subjects,um_attributes,um_values,um_groups,um_memberships,um_policies,um_attachments,um_candidates IN SHARE MODE`).run();
  await lockLegacyOrganizationAccess(database);
  await lockLegacyPeopleAccess(database);
  await lockLegacyBetaAccess(database);
  await lockLegacyRoutineAccess(database);
}

/** Acquire only after the source locks, inside the caller's Holding transaction.
 * No provider refresh is attempted under these locks. Stale directory evidence
 * requires the normal IAM refresh rather than silently omitting licensed work. */
async function legacyModuleAccessLease(owner: PostgresDatabase, database: SqlDatabase, companyId: string, now: () => number): Promise<LegacyModuleAccessLease> {
  owner.assertActiveTransaction(database);
  const observedAt = now();
  const stale = await database.prepare(`SELECT 1 FROM assignments a JOIN iam_members m
      ON m.company_id=a.company_id AND m.user_id=a.user_id
    JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id
    JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id
    WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
      AND m.status='active' AND m.login_policy='directory' AND c.status='active' AND i.enabled
      AND i.observed_at<=? LIMIT 1`).bind(companyId, observedAt, observedAt, observedAt - 300_000).first();
  if (stale) throw new ApiError(409, "LEGACY_DIRECTORY_REFRESH_REQUIRED", "Refresh this Company's directory through IAM before reviewing legacy package coverage.");
  const window = await database.prepare(`WITH boundaries AS (
    SELECT valid_from AS stamp FROM assignments WHERE company_id=? AND revoked_at IS NULL
    UNION ALL SELECT valid_until FROM assignments WHERE company_id=? AND revoked_at IS NULL
    UNION ALL SELECT i.observed_at+300000 FROM iam_members m
      JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id
      WHERE m.company_id=? AND m.status='active' AND m.login_policy='directory'
    UNION ALL SELECT expires_at FROM iam_access_subjects WHERE company_id=? AND status='active' AND kind='external'
    UNION ALL SELECT CASE WHEN (definition->>'validUntil') ~ '^[0-9]+$' THEN (definition->>'validUntil')::bigint END
      FROM um_candidates WHERE company_id=? AND status IN ('pending','active','rejected','blocked')
  ) SELECT min(stamp) AS deadline FROM boundaries WHERE stamp>?`)
    .bind(companyId, companyId, companyId, companyId, companyId, observedAt).first<{ deadline: number | null }>();
  const boundaries = [window?.deadline == null ? null : Number(window.deadline),
    await nextLegacyOrganizationTransition(database, companyId, observedAt), await nextLegacyPeopleTransition(database, companyId, observedAt),nextLegacyRoutineTransition(observedAt)]
    .filter((value): value is number => value !== null);
  if (boundaries.some(value => !Number.isSafeInteger(value))) throw new ApiError(409, "STALE_LEGACY_PREVIEW", "Legacy eligibility could not be verified. Preview again.");
  const deadline = Math.min(Number.MAX_SAFE_INTEGER, ...boundaries);
  const facts = await legacyCompanyModuleAccess(database, companyId);
  const witnesses = JSON.stringify(facts.witnesses.map(witness => ({ id: witness.assignmentId, version: witness.assignmentVersion })));
  async function assertCurrent() {
    owner.assertActiveTransaction(database);
    if (now() >= deadline) throw new ApiError(409, "STALE_LEGACY_PREVIEW", "Legacy access changed or expired. Preview again.");
    const row = await database.prepare(`SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint < ?::bigint
      AND NOT EXISTS(SELECT 1 FROM jsonb_to_recordset(?::jsonb) AS witness(id text,version integer)
        LEFT JOIN assignments a ON a.id=witness.id AND a.company_id=?
        WHERE a.id IS NULL OR a.version<>witness.version
          OR NOT iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)) AS valid`)
      .bind(deadline, witnesses, companyId).first<{ valid: boolean }>();
    if (!row?.valid || now() >= deadline) throw new ApiError(409, "STALE_LEGACY_PREVIEW", "Legacy access changed or expired. Preview again.");
    // The same owned receipt can execute deferred policy/lifecycle effects.
    // Assignment validity alone does not preserve published role, effective
    // modules, HR appointment, identity version, scope or the Beta ceiling.
    if ((await legacyCompanyModuleAccess(database, companyId)).revision !== facts.revision || now() >= deadline)
      throw new ApiError(409, "STALE_LEGACY_PREVIEW", "The reviewed Company access changed. Preview again.");
  }
  await assertCurrent();
  return { facts, assertCurrent };
}

/** Own the first snapshot: source locks precede every data read. Lazy source
 * loading lets a committed receipt replay without renewing historical grants.
 * Both the reader and its lease reject use after this transaction closes. */
export function withLegacyModuleAccessLease<T>(owner: PostgresDatabase, companyId: string,
  run: (database: SqlDatabase, source: () => Promise<LegacyModuleAccessLease>) => Promise<T>, now = Date.now) {
  return owner.transaction(async database => {
    await lockLegacyModuleAccess(database);
    return run(database, () => legacyModuleAccessLease(owner, database, companyId, now));
  });
}

/** Company license evidence, not an acting-user authorization or a role assignment. */
export async function legacyCompanyModuleAccess(database: SqlDatabase, companyId: string): Promise<LegacyCompanyModuleAccess> {
  if (!await database.prepare(`SELECT 1 ${org.authorization_organization_nodes_from} WHERE id=? AND kind='company' AND status='active'`).bind(companyId).first())
    throw new ApiError(404, "COMPANY_NOT_FOUND", "Select an active Company.");
  const rows = (await database.prepare(`SELECT a.id,a.user_id,a.role,a.modules_json,a.scope_kind,a.scope_node_id,a.version,a.valid_until
    FROM assignments a LEFT ${org.authorization_organization_nodes_n_join} ON n.id=a.scope_node_id
    WHERE a.company_id=? AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND (a.scope_kind='holding' OR (n.company_id=a.company_id AND n.status='active' AND n.kind=a.scope_kind
        AND (n.branch_id IS NULL OR n.kind='branch' OR EXISTS(SELECT 1 ${org.authorization_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active'))))
    ORDER BY a.id`).bind(companyId).all<{ id: string; user_id: string; role: string; modules_json: string | string[]; scope_kind: string; scope_node_id: string | null; version: number; valid_until: number }>()).results;
  const beta = await betaAccess(database, companyId, new Headers());
  const witnesses: LegacyModuleWitness[] = [];
  for (const row of rows) {
    const facts = await readIamAuthorityFacts(database, companyId, row.user_id, row.role);
    if (!facts.role?.scopes.includes(row.scope_kind)) continue;
    const permissions = effectiveAuthorityPermissions(row.role, facts.role.permissions, !!facts.hr_appointment);
    const assigned = typeof row.modules_json === "string" ? JSON.parse(row.modules_json) as string[] : row.modules_json;
    const modules = effectiveWorkModules(row.role, assigned, permissions, beta).sort();
    witnesses.push({ assignmentId: row.id, assignmentVersion: Number(row.version), roleId: row.role, roleRevision: facts.role.revision,
      scopeKind: row.scope_kind, scopeNodeId: row.scope_node_id, modules,
      validUntil: Number(row.valid_until), identityVersion: Number(facts.member?.version ?? 0), hrAppointmentVersion: facts.hr_appointment?.version ?? null,
      ...(beta?{}:{routineAccess:await legacyRoutineAccess(database,companyId,{id:row.id,userId:row.user_id})}),
      compatibility: beta ? [] : legacyModuleCompatibility({ role: row.role, permissions, assignmentScope: row.scope_kind }, modules) });
  }
  const revision = createHash("sha256").update(JSON.stringify({ companyId, beta, witnesses })).digest("hex");
  return { companyId, beta, observedAt: Date.now(), revision, witnesses };
}
