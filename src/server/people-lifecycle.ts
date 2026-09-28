import { eligibleClearanceOwner } from "./retirement-clearance";
import { effectiveGpoRules } from "./gpo-rules";
import { clearanceKinds, clearanceLabels, clearancePermissions, type ClearanceKind } from "@/lib/retirement";
import "server-only";
import { calendarCommand } from "./calendar/commands";
import { createHash } from "node:crypto";
import type { PoolClient } from "pg";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import {
  packageInput,
  type AccessPackage,
  type AccessPackageDefinition,
  type LifecycleCase,
  type LifecycleInput,
  type LifecyclePreview,
  type PeopleLifecycleData,
  type PreparationTask,
} from "@/lib/people-lifecycle";
import type { WorkItem } from "@/lib/work-item";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { PostgresDatabase } from "./runtime/postgres";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { iamDb } from "./iam-db";
import { iamActorFromWork, type IamActor } from "./iam-context";
import { iamCommand } from "./iam-transactions";
import { requireFreshMfa } from "./iam-mfa";
import {
  grantInput,
  validateIamGrant,
  insertIamGrant,
  proposeIamReview,
} from "./iam-service";
import { assertion, executeCommand, replayCommand } from "./commands";
import { organizationGuard, validateStaffOrganization } from "./organization";

function fail(message: string, code = "PEOPLE_CHANGED", status = 409): never {
  throw new ApiError(status, code, message);
}
const digest = (value: unknown) =>
  createHash("sha256").update(JSON.stringify(value)).digest("hex");
const parse = <T>(v: T | string): T =>
  typeof v === "string" ? JSON.parse(v) : v;
function company(actor: WorkContext) {
  if (actor.branchId || actor.departmentId)
    fail(
      "Choose the Company workspace to prepare shared access.",
      "COMPANY_SCOPE_REQUIRED",
      403,
    );
}
function readable(actor: WorkContext) {
  if (
    !["people.read", "people.manage", "iam.read", "organization.manage"].some(
      (p) => hasPermission(actor, p),
    )
  )
    fail(
      "People access is unavailable for this role.",
      "PERMISSION_REQUIRED",
      403,
    );
}
const visible = (
  actor: WorkContext,
  row: { branch_id: string | null; department_id: string | null },
) =>
  (!actor.branchId || row.branch_id === actor.branchId) &&
  (!actor.departmentId || row.department_id === actor.departmentId);

type LegacyCaseToSupersede = {
  id: string;
  version: number;
  status: string;
  kind: "onboarding" | "transfer";
  effectiveAt: number;
};

async function legacyCaseForPlacement(
  db: SqlDatabase,
  actor: WorkContext,
  personId: string,
): Promise<LegacyCaseToSupersede | null> {
  const row = await db.prepare(`SELECT c.id,c.version,c.status,c.kind,c.effective_at,e.branch_id,e.department_id
    FROM people_lifecycle_cases c JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
    WHERE c.company_id=? AND c.person_id=? AND c.workflow='legacy' AND c.status IN ('waiting_it','pending_review','scheduled')`)
    .bind(actor.companyId, personId)
    .first<{id:string;version:number;status:string;kind:LifecycleCase["kind"];effective_at:number;branch_id:string|null;department_id:string|null}>();
  if (!row) return null;
  if (!visible(actor, row)) fail("This person's existing case is outside your workspace.", "LEGACY_CASE_SCOPE", 403);
  if (row.kind !== "onboarding" && row.kind !== "transfer")
    fail("Cancel the existing departure or suspension through its own case before placing this person.", "LEGACY_DEPARTURE_ACTIVE");
  if (row.status === "scheduled" && row.effective_at <= Date.now())
    fail("This existing access case has reached its effective time. Refresh after it completes.", "LEGACY_CASE_EFFECTIVE");
  return {id:row.id,version:row.version,status:row.status,kind:row.kind,effectiveAt:row.effective_at};
}

export async function peopleLifecycleData(
  raw: SqlDatabase,
  actor: WorkContext,
): Promise<PeopleLifecycleData> {
  readable(actor);
  const db = iamDb(raw);
  const positions = (
    await db
      .prepare(
        "SELECT p.id,p.title,p.version,p.requirements,p.branch_id,p.department_id,p.level_id,p.active,b.package_id,b.revision package_revision FROM organization_positions p LEFT JOIN people_position_packages b ON b.company_id=p.company_id AND b.position_id=p.id WHERE p.company_id=? ORDER BY p.title,p.id",
      )
      .bind(actor.companyId)
      .all<PeopleLifecycleData["positions"][number]>()
  ).results.filter((p) => visible(actor, p));
  const gpoPolicies = await effectiveGpoRules(db, actor.companyId);
  const gpoByPosition = new Map(positions.map(position => [position.id, gpoPolicies.filter(policy => policy.position_id === position.id)]));
  for (const position of positions) {
    const policies = gpoByPosition.get(position.id) ?? [];
    position.gpo = { mapped: policies.length > 0, directoryStatus: !policies.length ? null
      : policies.some(policy => policy.directory_config_status === "blocked") ? "blocked"
      : policies.every(policy => policy.directory_config_status === "directory_applied") ? "directory_applied" : "pending" };
  }
  const allPackages = (
    await db
      .prepare(
        "SELECT * FROM people_access_packages WHERE company_id=? ORDER BY definition->>'name',id",
      )
      .bind(actor.companyId)
      .all<AccessPackage>()
  ).results;
  const packages =
    actor.branchId || actor.departmentId
      ? allPackages.filter((p) => positions.some((x) => x.package_id === p.id))
      : allPackages;
  const cases = (
    await db
      .prepare(
        "SELECT c.*,CASE WHEN NOT EXISTS(SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE m.company_id=c.company_id AND m.person_id=c.person_id AND m.status='active') THEN 'not_required' WHEN NOT EXISTS(SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE m.company_id=c.company_id AND m.person_id=c.person_id AND m.status='active' AND NOT EXISTS(SELECT 1 FROM people_directory_operations o WHERE o.company_id=c.company_id AND o.connection_id=i.connection_id AND o.status='completed' AND o.result->>'verified'='true' AND o.definition->'action'->>'kind'='account.disable' AND o.definition->'action'->>'caseId'=c.id AND o.definition->'action'->>'objectId'=i.external_id)) THEN 'verified' ELSE 'pending' END AS \"directoryStatus\" FROM people_lifecycle_cases c JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id WHERE c.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY c.updated_at DESC,c.id",
      )
      .bind(
        actor.companyId,
        actor.branchId,
        actor.branchId,
        actor.departmentId,
        actor.departmentId,
      )
      .all<LifecycleCase>()
  ).results;
  for (const c of cases)
    c.cutoffEffective =
      ["scheduled", "completed"].includes(c.status) &&
      c.effective_at <= Date.now();
  const tasks = (
    await db
      .prepare(
        "SELECT * FROM people_preparation_tasks WHERE company_id=? ORDER BY updated_at DESC,id",
      )
      .bind(actor.companyId)
      .all<PreparationTask>()
  ).results.filter((t) =>
    t.position_id
      ? positions.some((p) => p.id === t.position_id)
      : t.application_id ? !actor.branchId && !actor.departmentId && hasPermission(actor,"iam.directory.manage") : cases.some((c) => c.id === t.case_id),
  );
  const settings = await db
    .prepare(
      "SELECT version,definition FROM people_lifecycle_settings WHERE company_id=?",
    )
    .bind(actor.companyId)
    .first<{
      version: number;
      definition: Omit<PeopleLifecycleData["settings"], "version">;
    }>();
  const roster = (
    await db
      .prepare(
        `SELECT p.id,p.legal_name "name",e.id "engagementId",e.position_id "positionId",e.manager_person_id "managerPersonId",e.status FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.merged_into_id IS NULL AND (e.status!='ended' OR EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id)) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY p.legal_name,e.id`,
      )
      .bind(
        actor.companyId,
        actor.branchId,
        actor.branchId,
        actor.departmentId,
        actor.departmentId,
      )
      .all<PeopleLifecycleData["roster"][number]>()
  ).results;
  const preparation = (await db.prepare(`SELECT ap.engagement_id,ap.position_id,ap.package_id,ap.package_revision,people_application_active(ap.id,?) active,op.status,EXISTS(SELECT 1 FROM people_provision_grants g JOIN iam_reviews r ON r.id=g.review_id WHERE g.application_id=ap.id AND r.status='pending') OR EXISTS(SELECT 1 FROM people_preparation_tasks t WHERE t.application_id=ap.id AND t.status='pending') AS pending FROM people_applied_packages ap LEFT JOIN people_provision_operations op ON op.id=ap.operation_id WHERE ap.company_id=? AND ap.ended_at IS NULL`).bind(Date.now(),actor.companyId).all<{engagement_id:string;position_id:string|null;package_id:string;package_revision:number;active:boolean;status:string|null;pending:boolean}>()).results;
  for(const employee of roster) {
    const applied=preparation.filter(a=>a.engagement_id===employee.engagementId),position=positions.find(p=>p.id===employee.positionId);
    employee.itStatus=applied.some(a=>a.status==="blocked")?"problem":applied.some(a=>["queued","running","retry"].includes(a.status??"") || a.pending)?"working":applied.some(a=>a.active && a.position_id===employee.positionId && a.package_id===position?.package_id && a.package_revision===position?.package_revision)?"ready":"waiting";
  }
  const gpoAccounts = (await db.prepare(`SELECT DISTINCT m.person_id,i.connection_id,i.external_id
    FROM iam_members m JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id AND i.user_id=m.user_id
    WHERE m.company_id=? AND m.status='active' AND m.person_id IS NOT NULL AND i.enabled=true`)
    .bind(actor.companyId)
    .all<{person_id:string;connection_id:string;external_id:string}>()).results;
  const gpoJobs = (await db.prepare(`SELECT j.position_id,j.person_id,j.engagement_id,j.account_id,j.status,j.connection_id,
      COALESCE(j.rule_id,'legacy-position:'||j.position_id) rule_id
    FROM position_gpo_jobs j JOIN organization_gpo_rules r ON r.company_id=j.company_id
      AND r.id=COALESCE(j.rule_id,'legacy-position:'||j.position_id) AND r.revision=j.revision AND r.active
    WHERE j.company_id=? AND j.desired_action='add' AND j.status<>'superseded'`)
    .bind(actor.companyId)
    .all<{position_id:string;person_id:string;engagement_id:string|null;account_id:string|null;status:string;connection_id:string;rule_id:string}>()).results;
  for (const employee of roster) {
    const policies = employee.positionId ? gpoByPosition.get(employee.positionId) ?? [] : [];
    if (employee.status !== "active" || !policies.length) { employee.gpoDelivery = "none"; continue; }
    const deliveries = policies.map(policy => {
      if (policy.directory_config_status === "blocked") return "blocked";
      const accounts = gpoAccounts.filter(account => account.person_id === employee.id && account.connection_id === policy.connection_id);
      const jobs = gpoJobs.filter(job => job.rule_id === policy.id && job.connection_id === policy.connection_id && job.position_id === employee.positionId && job.person_id === employee.id && job.engagement_id === employee.engagementId);
      if (jobs.some(job => job.status === "blocked")) return "blocked";
      if (!accounts.length) return "awaiting_identity";
      return policy.directory_config_status === "directory_applied" && accounts.every(account => jobs.some(job => job.account_id === account.external_id && job.status === "directory_applied")) ? "directory_applied" : "queued";
    });
    employee.gpoDelivery = deliveries.includes("blocked") ? "blocked" : deliveries.includes("awaiting_identity") ? "awaiting_identity" : deliveries.includes("queued") ? "queued" : "directory_applied";
  }
  const managers = (
    await db
      .prepare(
        "SELECT DISTINCT p.id,p.legal_name name FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND e.status='active' ORDER BY p.legal_name,p.id",
      )
      .bind(actor.companyId)
      .all<{ id: string; name: string }>()
  ).results;
  const nodes = (
    await db
      .prepare(
        "SELECT id,display_name name,kind FROM organization_nodes WHERE company_id=? AND status='active' ORDER BY display_name,id",
      )
      .bind(actor.companyId)
      .all<{ id: string; name: string; kind: string }>()
  ).results;
  const clearanceOwners = (await db.prepare(`SELECT a.user_id id,coalesce(max(p.legal_name),a.user_id) name,bool_or(iam_role_allows(a.role,a.company_id,'payroll.prepare') AND a.scope_kind='holding') payroll,bool_or(iam_role_allows(a.role,a.company_id,'finance.write') AND a.scope_kind='holding') finance,true equipment FROM assignments a LEFT JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id AND p.merged_into_id IS NULL WHERE a.company_id=? AND iam_assignment_active(a.id,?) GROUP BY a.user_id ORDER BY name`).bind(actor.companyId,Date.now()).all<NonNullable<PeopleLifecycleData["clearanceOwners"]>[number]>()).results;
  return {
    clearanceOwners,
    roster,
    managers,
    nodes,
    positions,
    packages,
    cases,
    tasks: tasks.map(task => ({...task,document_references:[],completion_reason:["finance","payroll"].includes(task.clearance_type ?? "") ? undefined : task.completion_reason})),
    settings: {
      version: settings?.version ?? 0,
      itAssigneeId: null,
      onboardingTasks: [],
      offboardingTasks: [
        "Hand over work and pending approvals",
        "Return assigned equipment",
      ],
      ...settings?.definition,
    },
    canPrepare:
      hasPermission(actor, "iam.directory.manage") &&
      !actor.branchId &&
      !actor.departmentId,
    canManage: hasPermission(actor, "people.manage"),
    canReview: hasPermission(actor, "iam.manage"),
  };
}

async function packageImpact(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  definition: AccessPackageDefinition,
) {
  const positions = (
    await db
      .prepare(
        "SELECT p.id,p.title,p.version FROM people_position_packages b JOIN organization_positions p ON p.id=b.position_id AND p.company_id=b.company_id WHERE b.company_id=? AND b.package_id=? ORDER BY p.id",
      )
      .bind(actor.companyId, id)
      .all<{ id: string; title: string; version: number }>()
  ).results;
  const people = (
    await db
      .prepare(
        "SELECT e.id,e.version,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id JOIN people_position_packages b ON b.company_id=e.company_id AND b.position_id=e.position_id WHERE b.company_id=? AND b.package_id=? AND e.status!='ended' ORDER BY e.id",
      )
      .bind(actor.companyId, id)
      .all<{ id: string; version: number; legal_name: string }>()
  ).results;
  const snapshots = (
    await iamDb(db)
      .prepare(
        "SELECT definition FROM people_package_revisions WHERE company_id=? AND package_id=? ORDER BY revision",
      )
      .bind(actor.companyId, id)
      .all<{ definition: AccessPackageDefinition }>()
  ).results;
  const computers = [
    ...new Set(
      [
        ...snapshots.flatMap((r) => r.definition.windowsPolicies),
        ...definition.windowsPolicies,
      ].flatMap((p) => (p.computerTarget ? [p.computerTarget] : [])),
    ),
  ];
  return { positions, people, computers };
}
export async function previewPackage(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof packageInput>,
) {
  requirePermission(actor, "iam.directory.manage");
  company(actor);
  const id = input.id ?? "new",
    impact = await packageImpact(db, actor, id, input.definition);
  const current = input.id
    ? await iamDb(db)
        .prepare(
          "SELECT version,definition FROM people_access_packages WHERE company_id=? AND id=?",
        )
        .bind(actor.companyId, id)
        .first()
    : null;
  return {
    token: digest({ company: actor.companyId, input, current, impact }),
    affectedPositions: impact.positions.map((p) => p.title),
    affectedPeople: impact.people.map((p) => p.legal_name),
    affectedComputers: impact.computers,
  };
}
async function validatePackage(
  c: PoolClient,
  actor: IamActor,
  definition: AccessPackageDefinition,
) {
  for (const id of definition.groupIds) {
    if (
      !(
        await c.query(
          "SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2 AND NOT (definition->>'archived')::boolean",
          [actor.companyId, id],
        )
      ).rowCount
    )
      fail("A selected group is unavailable.");
  }
  for (const grant of definition.grants) {
    const role = (
      await c.query(
        "SELECT r.published_revision,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.status='active'",
        [actor.companyId, grant.roleId],
      )
    ).rows[0];
    if (
      !role ||
      role.published_revision !== grant.expectedRoleRevision ||
      !role.scopes_json.includes(grant.scopeKind) ||
      grant.validUntil <= Date.now()
    )
      fail("Refresh the role revision, scope, and expiry before publishing.");
    if (
      grant.scopeKind === "holding"
        ? grant.scopeNodeId !== null
        : !(
            await c.query(
              "SELECT 1 FROM organization_nodes WHERE company_id=$1 AND id=$2 AND kind=$3 AND status='active'",
              [actor.companyId, grant.scopeNodeId, grant.scopeKind],
            )
          ).rowCount
    )
      fail("Choose an access scope in this Company.");
  }
  if (definition.connectionId) {
    if (
      !(
        await c.query(
          "SELECT 1 FROM iam_connections WHERE company_id=$1 AND id=$2 AND status='active' AND ($3='local' OR provider=$3)",
          [actor.companyId, definition.connectionId, definition.account],
        )
      ).rowCount
    )
      fail("Choose an active connection matching the account provider.");
    for (const id of definition.directoryGroupIds)
      if (
        !(
          await c.query(
            "SELECT 1 FROM iam_groups WHERE connection_id=$1 AND external_id=$2",
            [definition.connectionId, id],
          )
        ).rowCount
      )
        fail("A directory group is missing from the latest snapshot.");
  }
}
export async function savePackage(
  db: PostgresDatabase,
  actor: WorkContext,
  input: z.infer<typeof packageInput>,
  key: string,
  publish: boolean,
  token?: string,
) {
  requirePermission(actor, "iam.directory.manage");
  company(actor);
  const id = input.id ?? `package-${key}`,
    iam = iamActorFromWork(actor);
  return iamCommand(
    db,
    iam,
    key,
    publish ? "people.package.publish" : "people.package.draft",
    { input, publish, token },
    "iam.directory.manage",
    async (c) => {
      const current = (
        await c.query(
          "SELECT * FROM people_access_packages WHERE company_id=$1 AND id=$2 FOR UPDATE",
          [actor.companyId, id],
        )
      ).rows[0];
      if (
        Number(current?.version ?? 0) !== input.expectedVersion ||
        (input.id && !current)
      )
        fail("The access package changed. Refresh before saving.");
      if (publish) {
        // Preview reads and row writes share this SERIALIZABLE transaction.
        const adapter = clientDatabase(c);
        if (
          !token ||
          token !== (await previewPackage(adapter, actor, input)).token
        )
          fail(
            "Review the current positions, people, and computer targets before publishing.",
            "PREVIEW_CHANGED",
          );
        await validatePackage(c, iam, input.definition);
      }
      const version = input.expectedVersion + 1,
        revision = publish
          ? Number(current?.published_revision ?? 0) + 1
          : Number(current?.published_revision ?? 0);
      await c.query(
        "INSERT INTO people_access_packages(company_id,id,version,published_revision,definition) VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id,id) DO UPDATE SET version=excluded.version,published_revision=excluded.published_revision,definition=excluded.definition",
        [
          actor.companyId,
          id,
          version,
          revision,
          JSON.stringify(input.definition),
        ],
      );
      if (publish) {
        await c.query(
          "INSERT INTO people_package_revisions VALUES($1,$2,$3,$4,$5,$6)",
          [
            actor.companyId,
            id,
            revision,
            JSON.stringify(input.definition),
            actor.userId,
            Date.now(),
          ],
        );
        await c.query(
          "UPDATE people_position_packages SET revision=$3 WHERE company_id=$1 AND package_id=$2",
          [actor.companyId, id, revision],
        );
      }
      await lifecycleOutbox(
        c,
        actor,
        key,
        "people.package.changed",
        id,
        version,
      );
      return { id, version, revision };
    },
  );
}

/** A transaction-local adapter preserves the same preview snapshot as the committing write. */
function clientDatabase(c: PoolClient): SqlDatabase {
  return {
    prepare(sql: string) {
      const make = (args: unknown[]) => ({
        bind: (...v: unknown[]) => make(v),
        all: async () => {
          index = 0;
          return {
            results: (
              await c.query(
                sql.replace(/\?/g, () => `$${++index}`),
                args,
              )
            ).rows,
          };
        },
        first: async () => {
          index = 0;
          return (
            (
              await c.query(
                sql.replace(/\?/g, () => `$${++index}`),
                args,
              )
            ).rows[0] ?? null
          );
        },
      });
      let index = 0;
      return make([]);
    },
  } as unknown as SqlDatabase;
}
async function lifecycleOutbox(
  c: PoolClient,
  actor: WorkContext,
  key: string,
  event: string,
  id: string,
  version: number,
) {
  await c.query(
    "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,'people_lifecycle',$4,$5,$6,$7)",
    [
      crypto.randomUUID(),
      key,
      event,
      id,
      version,
      JSON.stringify({ id, companyId: actor.companyId, version }),
      Date.now(),
    ],
  );
}
export async function bindPositionPackage(
  db: PostgresDatabase,
  actor: WorkContext,
  input: {
    positionId: string;
    packageId: string;
    revision: number;
    expectedVersion: number;
    previewToken?: string;
  },
  key: string,
  previewOnly = false,
) {
  requirePermission(actor, "iam.directory.manage");
  company(actor);
  return iamCommand(
    db,
    iamActorFromWork(actor),
    key,
    previewOnly ? "people.position.preview" : "people.position.bind",
    input,
    "iam.directory.manage",
    async (c) => {
      const position = (
        await c.query(
          "SELECT * FROM organization_positions WHERE company_id=$1 AND id=$2 AND active=1 FOR UPDATE",
          [actor.companyId, input.positionId],
        )
      ).rows[0];
      const pkg = (
        await c.query(
          "SELECT r.* FROM people_package_revisions r JOIN people_access_packages p ON p.company_id=r.company_id AND p.id=r.package_id AND p.published_revision=r.revision WHERE r.company_id=$1 AND r.package_id=$2 AND r.revision=$3",
          [actor.companyId, input.packageId, input.revision],
        )
      ).rows[0];
      if (!position || position.version !== input.expectedVersion || !pkg)
        fail("Refresh the position and choose a published access package.");
      const people = (
        await c.query(
          "SELECT p.legal_name,e.id,e.version FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=$1 AND e.position_id=$2 AND e.status!='ended' ORDER BY e.id",
          [actor.companyId, position.id],
        )
      ).rows;
      const token = digest({ position, pkg, people });
      if (previewOnly)
        return {
          token,
          people: people.map((p) => p.legal_name),
          definition: pkg.definition,
        };
      if (input.previewToken !== token)
        fail(
          "Review who uses this position before applying the package.",
          "PREVIEW_CHANGED",
        );
      await c.query(
        "INSERT INTO people_position_packages VALUES($1,$2,$3,$4) ON CONFLICT(company_id,position_id) DO UPDATE SET package_id=excluded.package_id,revision=excluded.revision",
        [actor.companyId, position.id, input.packageId, input.revision],
      );
      await c.query(
        "UPDATE organization_positions SET version=version+1 WHERE company_id=$1 AND id=$2",
        [actor.companyId, position.id],
      );
      await c.query(
        "UPDATE people_preparation_tasks SET status='completed',version=version+1,updated_at=$3 WHERE company_id=$1 AND position_id=$2 AND kind='position'",
        [actor.companyId, position.id, Date.now()],
      );
      await lifecycleOutbox(
        c,
        actor,
        key,
        "people.position.prepared",
        position.id,
        position.version + 1,
      );
      return { id: position.id, version: position.version + 1 };
    },
  );
}

async function engagement(
  db: SqlDatabase,
  actor: WorkContext,
  input: LifecycleInput,
) {
  const row = await db
    .prepare(
      "SELECT e.*,p.legal_name,p.user_id,p.status person_status FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.id=? AND e.person_id=? AND e.company_id=? AND p.merged_into_id IS NULL",
    )
    .bind(input.engagementId, input.personId, actor.companyId)
    .first<{
      id: string;
      person_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      position_id: string | null;
      level_id: string | null;
      manager_person_id: string | null;
      job_title: string;
      status: string;
      legal_name: string;
      user_id: string | null;
      person_status: string;
      start_date: string;
      end_date: string | null;
    }>();
  if (!row || !visible(actor, row))
    fail(
      "Choose an engagement in your workspace.",
      "ENGAGEMENT_NOT_FOUND",
      404,
    );
  return row;
}
async function linkedAccounts(
  db: SqlDatabase,
  actor: WorkContext,
  personId: string,
  activeOnly = false,
  definition?: AccessPackageDefinition,
) {
  return (
    await db
      .prepare(
        "SELECT DISTINCT u.user_id FROM (SELECT user_id FROM hr_people WHERE company_id=? AND id=? UNION SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?) u LEFT JOIN iam_members m ON m.company_id=? AND m.user_id=u.user_id WHERE u.user_id IS NOT NULL AND (NOT ?::boolean OR COALESCE(m.status,'active')='active') AND (?::text IS NULL OR CASE WHEN ?='local' THEN COALESCE(m.login_policy,'local')!='directory' ELSE m.login_policy='directory' AND m.connection_id=? END) ORDER BY u.user_id",
      )
      .bind(
        actor.companyId,
        personId,
        actor.companyId,
        personId,
        actor.companyId,
        activeOnly,
        definition?.account ?? null,
        definition?.account ?? null,
        definition?.connectionId ?? null,
      )
      .all<{ user_id: string }>()
  ).results.map((r) => r.user_id);
}
export async function previewLifecycle(
  raw: SqlDatabase,
  actor: WorkContext,
  input: LifecycleInput,
  placementOnly = false,
): Promise<LifecyclePreview> {
  readable(actor);
  if (input.departureReason === "retirement" && input.kind !== "offboarding") fail("Retirement uses offboarding.","RETIREMENT_KIND",422);
  const db = iamDb(raw),
    e = await engagement(db, actor, input),
    blockers: string[] = [];
  const position = input.positionId
    ? await db
        .prepare(
          "SELECT p.*,b.package_id,b.revision FROM organization_positions p LEFT JOIN people_position_packages b ON b.position_id=p.id AND b.company_id=p.company_id WHERE p.company_id=? AND p.id=? AND p.active=1",
        )
        .bind(actor.companyId, input.positionId)
        .first<{
          id: string;
          branch_id: string | null;
          department_id: string | null;
          level_id: string;
          package_id: string | null;
          revision: number | null;
        }>()
    : null;
  const incoming = ["onboarding", "transfer"].includes(input.kind);
  if(input.acceptRecommendedPay&&!hasPermission(actor,"people.compensation.read"))throw new ApiError(403,"PAY_ACCESS_REQUIRED","Compensation access is required.");
  const recommendedPay=incoming&&position&&hasPermission(actor,"people.compensation.read")?await db.prepare("SELECT d.currency,d.monthly_amount_minor FROM position_pay_defaults d JOIN organization_positions p ON p.id=d.position_id WHERE d.company_id=? AND d.position_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY d.version DESC LIMIT 1").bind(actor.companyId,position.id,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<{currency:string;monthly_amount_minor:number}>():null;
  const recommendedLeave=incoming&&position?await db.prepare(`SELECT l.name level_name,max(CASE WHEN t.code='annual' THEN r.grant_units END) annual_days,max(CASE WHEN t.code='sick' THEN r.grant_units END) sick_days
    FROM level_leave_defaults d JOIN staff_levels l ON l.id=d.level_id JOIN leave_policy_rules r ON r.policy_version_id=d.policy_version_id JOIN leave_types t ON t.id=r.leave_type_id
    WHERE d.company_id=? AND d.level_id=? AND d.version=(SELECT max(x.version) FROM level_leave_defaults x WHERE x.company_id=d.company_id AND x.level_id=d.level_id) GROUP BY l.name`)
    .bind(actor.companyId,position.level_id).first<{level_name:string;annual_days:number;sick_days:number}>():null;
  if(input.acceptRecommendedPay&&!recommendedPay)blockers.push("This Position has no default pay. Review the position before submitting.");
  if(input.acceptRecommendedLeave&&!recommendedLeave)blockers.push("This Level has no default leave policy. Review People settings before submitting.");
  if (
    incoming &&
    (!position ||
      !visible(actor, {
        branch_id: position.branch_id ?? e.branch_id,
        department_id: position.department_id ?? e.department_id,
      }))
  )
    fail("Choose a position in your workspace.", "POSITION_REQUIRED", 422);
  const revision = position?.package_id
    ? await db
        .prepare(
          "SELECT definition FROM people_package_revisions WHERE company_id=? AND package_id=? AND revision=?",
        )
        .bind(actor.companyId, position.package_id, position.revision)
        .first<{ definition: AccessPackageDefinition }>()
    : null;
  if (incoming && !placementOnly && !revision)
    blockers.push("IT has not prepared this position's access package.");
  if (
    incoming &&
    !placementOnly &&
    revision &&
    !(await db
      .prepare(
        "SELECT 1 FROM people_preparation_tasks WHERE company_id=? AND position_id=? AND kind='position' AND status='completed'",
      )
      .bind(actor.companyId, input.positionId)
      .first())
  )
    blockers.push("IT must review the position's updated work requirements.");
  if (incoming && e.status === "ended")
    blockers.push("Create a new engagement before rehiring this person.");
  if (incoming) {
    try {
      await validateStaffOrganization(
        raw,
        actor.companyId,
        {
          person_id: input.personId,
          branch_id: position!.branch_id ?? e.branch_id,
          department_id: position!.department_id ?? e.department_id,
          position_id: input.positionId,
          level_id: position!.level_id,
          manager_person_id: input.managerPersonId,
        },
        true,
      );
    } catch (error) {
      if (error instanceof ApiError) blockers.push(error.message);
      else throw error;
    }
  }
  const accounts = await linkedAccounts(
    db,
    actor,
    input.personId,
    incoming,
    incoming ? revision?.definition : undefined,
  );
  if (incoming && !placementOnly && !accounts.length)
    blockers.push(
      "Identity verification and account linking are required before requesting access.",
    );
  const old = (
    await db
      .prepare(
        "SELECT a.id,a.role,a.source,a.version FROM assignments a WHERE a.company_id=? AND people_person_user(?, ?,a.user_id) AND a.revoked_at IS NULL ORDER BY a.id",
      )
      .bind(actor.companyId, actor.companyId, input.personId)
      .all<{ id: string; role: string; source: string; version: number }>()
  ).results;
  const removed = old
    .filter((g) => !incoming || (!placementOnly && g.source === "lifecycle"))
    .map((g) => g.role);
  const definition = incoming && !placementOnly ? (revision?.definition ?? null) : null;
  const legacyCaseToSupersede = placementOnly && incoming
    ? await legacyCaseForPlacement(db, actor, input.personId)
    : null;
  return {
    recommendation:incoming?{levelName:recommendedLeave?.level_name??null,currency:recommendedPay?.currency??null,monthlyAmountMinor:recommendedPay?.monthly_amount_minor??null,annualDays:recommendedLeave?.annual_days??null,sickDays:recommendedLeave?.sick_days??null}:undefined,
    token: digest(placementOnly && incoming ? {input, e, position: position && {id:position.id,branch_id:position.branch_id,department_id:position.department_id,level_id:position.level_id}, placementOnly, legacyCaseToSupersede} : { input, e, position, revision, accounts, old }),
    ready: !blockers.length,
    blockers,
    legacyCaseToSupersede,
    definition,
    removed,
    added: definition?.grants.map((g) => g.roleId) ?? [],
    accounts,
    affectedPositions: [],
    affectedPeople: [e.legal_name],
    affectedComputers:
      definition?.windowsPolicies.flatMap((p) =>
        p.computerTarget ? [p.computerTarget] : [],
      ) ?? [],
  };
}
export async function saveLifecycle(
  db: SqlDatabase,
  actor: WorkContext,
  input: LifecycleInput,
  key: string,
  submit: boolean,
  token?: string,
  placementOnly = false,
  supersedeLegacyCase?: {id:string;expectedVersion:number},
) {
  actor = requirePermission(actor, "people.manage");
  const id = input.id ?? `lifecycle-${key}`,
    operation = submit ? "people.lifecycle.submit" : "people.lifecycle.draft",
    payload = { input, submit, token, placementOnly, supersedeLegacyCase };
  const prior = await replayCommand(db, actor, key, operation, id, payload);
  if (prior) return prior;
  const graphGuard = await organizationGuard(db, actor.companyId, key);
  const e = await engagement(db, actor, input),
    preview = await previewLifecycle(db, actor, input, placementOnly);
  const current = input.id
    ? await db
        .prepare(
          "SELECT * FROM people_lifecycle_cases WHERE id=? AND company_id=?",
        )
        .bind(id, actor.companyId)
        .first<LifecycleCase>()
    : null;
  if (placementOnly && current?.workflow === "legacy")
    fail("Start a new HR placement and explicitly supersede the legacy access case.", "LEGACY_CASE_REQUIRES_NEW_PLACEMENT");
  if (supersedeLegacyCase && (!placementOnly || !submit || !["onboarding","transfer"].includes(input.kind)))
    fail("Legacy access cases can be superseded only when submitting an HR placement.", "LEGACY_SUPERSEDE_INVALID", 422);
  const legacy = placementOnly && submit && ["onboarding","transfer"].includes(input.kind)
    ? preview.legacyCaseToSupersede
    : null;
  if (legacy && !supersedeLegacyCase)
    fail("Confirm that the existing access case will be cancelled before submitting this HR placement.", "LEGACY_SUPERSEDE_REQUIRED", 422);
  if (supersedeLegacyCase && (!legacy || supersedeLegacyCase.id !== legacy.id || supersedeLegacyCase.expectedVersion !== legacy.version || id === legacy.id))
    fail("The existing access case changed. Preview the HR placement again.", "LEGACY_CASE_CHANGED");
  if (
    input.kind === "onboarding" &&
    (await db
      .prepare(
        "SELECT 1 FROM people_lifecycle_cases WHERE company_id=? AND engagement_id=? AND kind IN ('onboarding','transfer') AND status IN ('scheduled','completed') AND id<>?",
      )
      .bind(actor.companyId, input.engagementId, legacy?.id ?? "")
      .first())
  )
    fail(
      "This engagement has already been onboarded. Use a transfer to review replacement access.",
      "TRANSFER_REQUIRED",
    );
  if (
    input.id &&
    (!current ||
      !["draft", "waiting_it"].includes(current.status) ||
      current.person_id !== input.personId ||
      current.kind !== input.kind ||
      current.engagement_id !== input.engagementId)
  )
    fail("Only this person's draft or waiting case can be edited.");
  if (submit && token !== preview.token)
    fail(
      "Review the current access and affected accounts before submitting.",
      "PREVIEW_CHANGED",
    );
  if (input.departureReason === "retirement" && input.kind !== "offboarding") fail("Retirement uses offboarding.","RETIREMENT_KIND",422);
  const outgoing = input.kind === "offboarding" || input.kind === "suspend";
  if (outgoing) {
    company(actor);
    if (e.status === "ended")
      fail(
        "This engagement has already ended. Choose the current engagement.",
        "ENGAGEMENT_ENDED",
      );
    if (input.lastWorkingDate && input.lastWorkingDate < e.start_date)
      fail(
        "The last working date cannot precede the engagement start date.",
        "INVALID_DATES",
        422,
      );
    if (preview.accounts.includes(actor.userId))
      fail(
        "Another HR administrator must end or suspend your access.",
        "SELF_ACCESS_CHANGE",
        403,
      );
    if (
      await db
        .prepare(
          "SELECT 1 FROM assignments a LEFT JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id WHERE a.company_id=? AND people_person_user(?,?,a.user_id) AND a.revoked_at IS NULL AND (a.role='holding_owner' OR m.login_policy='emergency')",
        )
        .bind(actor.companyId, actor.companyId, input.personId)
        .first()
    )
      fail(
        "Use the protected administrator workflow before ending this person's access.",
        "PROTECTED_ADMINISTRATOR",
      );
    if (input.kind === "offboarding" && !input.lastWorkingDate)
      fail("Specify the last working date.", "LAST_WORKING_DATE_REQUIRED", 422);
  }
  if (submit && placementOnly && !preview.ready) fail(preview.blockers.join(" "), "PLACEMENT_NOT_READY", 422);
  const state = !submit
    ? "draft"
    : outgoing || placementOnly
      ? "scheduled"
      : preview.ready
        ? "pending_review"
        : "waiting_it";
  const binding = input.positionId && !placementOnly
    ? await db
        .prepare(
          "SELECT package_id,revision FROM people_position_packages WHERE company_id=? AND position_id=?",
        )
        .bind(actor.companyId, input.positionId)
        .first<{ package_id: string; revision: number }>()
    : null;
  const position = input.positionId
    ? await db
        .prepare(
          "SELECT level_id,branch_id,department_id FROM organization_positions WHERE company_id=? AND id=?",
        )
        .bind(actor.companyId, input.positionId)
        .first<{
          level_id: string;
          branch_id: string | null;
          department_id: string | null;
        }>()
    : null;
  const snapshot = {
    engagementVersion: e.version,
    engagementPlacement: {
      position_id: e.position_id,
      branch_id: e.branch_id,
      department_id: e.department_id,
      level_id: e.level_id,
      manager_person_id: e.manager_person_id,
      job_title: e.job_title,
      status: e.status,
      start_date: e.start_date,
      end_date: e.end_date,
    },
    levelId: position?.level_id ?? null,
    positionBranch: position?.branch_id ?? null,
    positionDepartment: position?.department_id ?? null,
  };
  const version = input.expectedVersion + 1,
    now = Date.now(),
    steps = [graphGuard];
  if (legacy) steps.push(
    assertion(db,key,"legacy-case-version",
      "EXISTS(SELECT 1 FROM people_lifecycle_cases WHERE id=? AND company_id=? AND person_id=? AND workflow='legacy' AND kind IN ('onboarding','transfer') AND version=? AND status IN ('waiting_it','pending_review','scheduled') AND (status!='scheduled' OR effective_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))",
      [legacy.id,actor.companyId,input.personId,legacy.version]),
    db.prepare("UPDATE people_lifecycle_cases SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=?")
      .bind(now,legacy.id,actor.companyId),
    db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id IN (SELECT assignment_id FROM people_lifecycle_grants WHERE case_id=?) AND revoked_at IS NULL")
      .bind(now,legacy.id),
    db.prepare("UPDATE iam_reviews SET status='failed',version=version+1 WHERE company_id=? AND payload->>'caseId'=? AND kind IN ('people.lifecycle.grant','people.lifecycle.activate') AND status='pending'")
      .bind(actor.companyId,legacy.id),
    db.prepare("UPDATE people_preparation_tasks SET status='cancelled',version=version+1,updated_at=? WHERE company_id=? AND case_id=? AND status='pending'")
      .bind(now,actor.companyId,legacy.id),
    db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,'people_lifecycle',?,?,?,?,(SELECT authority_json FROM command_receipts WHERE command_id=?))")
      .bind(crypto.randomUUID(),key,actor.userId,actor.assignmentId,"people.lifecycle.supersede",legacy.id,legacy.version+1,`Superseded by HR placement ${id}`,now,key),
  );
  steps.push(
    assertion(
      db,
      key,
      "case-version",
      current
        ? "EXISTS(SELECT 1 FROM people_lifecycle_cases WHERE id=? AND version=? AND status IN ('draft','waiting_it'))"
        : "?=0",
      current ? [id, input.expectedVersion] : [input.expectedVersion],
    ),
  );
  steps.push(
    db
      .prepare(
        "INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,package_id,package_revision,definition,created_by,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET status=excluded.status,version=excluded.version,effective_at=excluded.effective_at,package_id=excluded.package_id,package_revision=excluded.package_revision,definition=excluded.definition,updated_at=excluded.updated_at",
      )
      .bind(
        id,
        actor.companyId,
        input.personId,
        input.engagementId,
        input.kind,
        state,
        version,
        input.effectiveAt,
        binding?.package_id ?? null,
        binding?.revision ?? null,
        JSON.stringify(input),
        actor.userId,
        now,
      ),
  );
  steps.push(
    db
      .prepare(
        "UPDATE people_lifecycle_cases SET review_snapshot=?,authorizing_assignment_id=?,workflow=? WHERE id=?",
      )
      .bind(JSON.stringify(snapshot), actor.assignmentId, placementOnly ? "placement" : "legacy", id),
  );
  if (submit && outgoing)
    steps.push(
      assertion(
        db,
        key,
        "protected-person",
        "NOT people_person_user(?,?,?) AND NOT EXISTS(SELECT 1 FROM assignments a LEFT JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id WHERE a.company_id=? AND people_person_user(?,?,a.user_id) AND a.revoked_at IS NULL AND (a.role='holding_owner' OR m.login_policy='emergency'))",
        [
          actor.companyId,
          input.personId,
          actor.userId,
          actor.companyId,
          actor.companyId,
          input.personId,
        ],
      ),
      db
        .prepare(
          "INSERT INTO people_access_cutoffs(company_id,person_id,case_id,effective_at) VALUES(?,?,?,?)",
        )
        .bind(actor.companyId, input.personId, id, input.effectiveAt),
    );
  if (submit && binding)
    steps.push(
      assertion(
        db,
        key,
        "prepared-package",
        "EXISTS(SELECT 1 FROM people_position_packages WHERE company_id=? AND position_id=? AND package_id=? AND revision=?)",
        [
          actor.companyId,
          input.positionId!,
          binding.package_id,
          binding.revision,
        ],
      ),
    );
  steps.push(db.prepare("UPDATE people_lifecycle_cases SET departure_reason=? WHERE id=? AND company_id=?").bind(input.departureReason ?? "other",id,actor.companyId));
  if (submit) {
    const settings = await iamDb(db)
      .prepare(
        "SELECT definition FROM people_lifecycle_settings WHERE company_id=?",
      )
      .bind(actor.companyId)
      .first<{ definition: PeopleLifecycleData["settings"] }>();
    if(input.departureReason === "retirement") {
      const manager = await db.prepare("SELECT p.user_id FROM hr_engagements e JOIN hr_people p ON p.id=e.manager_person_id AND p.company_id=e.company_id WHERE e.id=? AND e.company_id=? AND p.status='active' AND p.merged_into_id IS NULL").bind(input.engagementId,actor.companyId).first<{user_id:string|null}>();
      const owners: Record<ClearanceKind,string|null> = {hr:actor.userId,handover:manager?.user_id??null,equipment:input.clearanceOwners?.equipment??null,directory:settings?.definition.itAssigneeId??null,payroll:input.clearanceOwners?.payroll??null,finance:input.clearanceOwners?.finance??null};
      for(const kind of clearanceKinds) {
        let owner = owners[kind];
        if(owner && !await eligibleClearanceOwner(db,actor.companyId,owner,kind)) {
          if(["equipment","payroll","finance"].includes(kind)) fail("Choose a currently eligible clearance owner.","CLEARANCE_ASSIGNEE",422);
          owner = null;
        }
        if(owner) steps.push(assertion(db,key,`clearance-owner-${kind}`,"EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (?::text IS NULL OR iam_role_allows(role,company_id,?)) AND (? NOT IN ('payroll','finance','directory') OR scope_kind='holding'))",[actor.companyId,owner,clearancePermissions[kind],clearancePermissions[kind],kind]));
        const taskId = `${id}-clearance-${kind}`;
        steps.push(db.prepare("INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,assignee_id,updated_at,privileged,due_date,responsibility,clearance_type) VALUES(?,?,?,'lifecycle',?,?,?,?,?,?,?)").bind(taskId,actor.companyId,id,clearanceLabels[kind],owner,now,kind === "directory",input.lastWorkingDate,kind,kind));
        if(owner)steps.push(db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'retirement_clearance',?,1,?,?) ON CONFLICT DO NOTHING").bind(`${taskId}-notification`,actor.companyId,owner,taskId,`${clearanceLabels[kind]} · ${e.legal_name}`,now));
      }
    } else {
    const tasks: {title:string;privileged:boolean}[] = [
      ...(outgoing ? (settings?.definition.offboardingTasks ?? ["Hand over work and pending approvals", "Return assigned equipment"]) : (settings?.definition.onboardingTasks ?? [])).map(title => ({title, privileged:false})),
      ...(!outgoing && placementOnly ? [{title:"Prepare access for employee placement",privileged:true}] : []),
      ...(preview.definition?.tasks.map(t => ({title:t.label,privileged:!["equipment","handover"].includes(t.kind)})) ?? []),
      ...(preview.definition?.directoryGroupIds.map(id => ({title:`Prepare directory membership · ${id}`,privileged:true})) ?? []),
      ...(preview.definition?.windowsPolicies.map(p => ({title:`Apply and verify GPO · ${p.name} · ${p.targetOu}${p.computerTarget ? ` · Computers: ${p.computerTarget}` : ""}`,privileged:true})) ?? []),
      ...(outgoing ? [{title:"Review linked directory accounts; ERPX cutoff is independent of directory completion",privileged:true}] : []),
    ];
    tasks.forEach((task, i) =>
      steps.push(
        db
          .prepare(
            "INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,assignee_id,updated_at,privileged) VALUES(?,?,?,'lifecycle',?,?,?,?) ON CONFLICT(id) DO NOTHING",
          )
          .bind(
            `${id}-task-${i}`,
            actor.companyId,
            id,
            task.title,
            settings?.definition.itAssigneeId ?? null,
            now,
            task.privileged,
          ),
      ),
    );
  }
  }
  return executeCommand(
    db,
    actor,
    key,
    operation,
    payload,
    id,
    version,
    { id, version, status: state },
    steps,
    {
      companyId: actor.companyId,
      branchId: e.branch_id,
      departmentId: e.department_id,
    },
  );
}
export async function cancelLifecycle(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  expectedVersion: number,
  key: string,
) {
  actor = requirePermission(actor, "people.manage");
  const replay = await replayCommand(
    db,
    actor,
    key,
    "people.lifecycle.cancel",
    id,
    { id, expectedVersion },
  );
  if (replay) return replay;
  const row = await db
    .prepare("SELECT * FROM people_lifecycle_cases WHERE company_id=? AND id=?")
    .bind(actor.companyId, id)
    .first<LifecycleCase>();
  if (!row) fail("Lifecycle case not found.", "CASE_NOT_FOUND", 404);
  await engagement(db, actor, parse(row.definition));
  const now = Date.now();
  if (
    row.status === "completed" ||
    row.status === "cancelled" ||
    (row.status === "scheduled" && row.effective_at <= now)
  )
    fail(
      "This change has taken effect. Request new access to restore it.",
      "ALREADY_EFFECTIVE",
    );
  return executeCommand(
    db,
    actor,
    key,
    "people.lifecycle.cancel",
    { id, expectedVersion },
    id,
    expectedVersion + 1,
    { id, status: "cancelled" },
    [
      assertion(
        db,
        key,
        "case-version",
        "EXISTS(SELECT 1 FROM people_lifecycle_cases WHERE id=? AND company_id=? AND version=? AND status NOT IN ('completed','cancelled') AND (status!='scheduled' OR effective_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))",
        [id, actor.companyId, expectedVersion],
      ),
      db
        .prepare(
          "UPDATE people_lifecycle_cases SET status='cancelled',version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id),
      db
        .prepare(
          "UPDATE people_access_cutoffs SET cancelled_at=? WHERE case_id=?",
        )
        .bind(now, id),
      db
        .prepare(
          "UPDATE assignments SET revoked_at=?,version=version+1 WHERE id IN (SELECT assignment_id FROM people_lifecycle_grants WHERE case_id=?) AND revoked_at IS NULL",
        )
        .bind(now, id),
      db
        .prepare(
          "UPDATE iam_reviews SET status='failed',version=version+1 WHERE id IN (SELECT review_id FROM people_lifecycle_grants WHERE case_id=?) AND status='pending'",
        )
        .bind(id),
      db
        .prepare(
          "UPDATE people_preparation_tasks SET status='cancelled',version=version+1 WHERE case_id=? AND status='pending'",
        )
        .bind(id),
    ],
  );
}

export async function requestLifecycleAccess(
  db: PostgresDatabase,
  actor: WorkContext,
  id: string,
  version: number,
  key: string,
) {
  requirePermission(actor, "iam.manage");
  const iam = iamActorFromWork(actor);
  await requireFreshMfa(iam);
  return iamCommand(
    db,
    iam,
    key,
    "people.lifecycle.request-access",
    { id, version },
    "iam.manage",
    async (c) => {
      const row = (
        await c.query(
          "SELECT * FROM people_lifecycle_cases WHERE company_id=$1 AND id=$2 FOR UPDATE",
          [actor.companyId, id],
        )
      ).rows[0] as LifecycleCase | undefined;
      if (!row || row.version !== version || row.status !== "pending_review")
        fail("Refresh the lifecycle case before requesting personal access.");
      if (
        (
          await c.query(
            "SELECT 1 FROM iam_reviews WHERE company_id=$1 AND payload->>'caseId'=$2 AND status IN ('pending','approved')",
            [actor.companyId, id],
          )
        ).rowCount
      )
        fail(
          "Individual reviews have already been requested. Follow them in Work.",
          "REVIEWS_ALREADY_REQUESTED",
        );
      const e = await engagement(clientDatabase(c), actor, row.definition);
      const readiness = await previewLifecycle(
        clientDatabase(c),
        actor,
        row.definition,
      );
      if (!readiness.ready)
        fail(readiness.blockers.join(" "), "LIFECYCLE_NOT_READY");
      const definition = (
        await c.query(
          "SELECT definition FROM people_package_revisions WHERE company_id=$1 AND package_id=$2 AND revision=$3",
          [actor.companyId, row.package_id, row.package_revision],
        )
      ).rows[0]?.definition as AccessPackageDefinition | undefined;
      if (!definition) fail("IT must publish this position's package first.");
      const users = await linkedAccounts(
        clientDatabase(c),
        actor,
        e.person_id,
        true,
        definition,
      );
      if (!users.length) fail("Verify and link an account first.");
      if (users.includes(actor.userId))
        fail(
          "Another administrator must request your access.",
          "SELF_ACCESS_CHANGE",
          403,
        );
      const reviews: string[] = [];
      for (const userId of users)
        for (const [index, g] of definition.grants.entries()) {
          const grant = grantInput.parse({
            ...g,
            userId,
            reason: row.definition.reason,
          });
          if (grant.validUntil <= row.effective_at)
            fail("Access expires before the requested start date.");
          await validateIamGrant(c, iam, grant);
          const grantId = `${id}-${userId}-${index}`;
          const review = await proposeIamReview(
            c,
            iam,
            "people.lifecycle.grant",
            { ...grant, caseId: id, grantId, parentId: actor.assignmentId },
            `Review individual access · ${e.legal_name}`,
          );
          await c.query(
            "INSERT INTO people_lifecycle_grants(id,company_id,case_id,user_id,grant_index,definition,review_id) VALUES($1,$2,$3,$4,$5,$6,$7)",
            [
              grantId,
              actor.companyId,
              id,
              userId,
              index,
              JSON.stringify(grant),
              review.id,
            ],
          );
          reviews.push(review.id);
        }
      // Even an empty recipe needs a separate reviewer before activating the engagement.
      if (!reviews.length) {
        const review = await proposeIamReview(
          c,
          iam,
          "people.lifecycle.activate",
          { caseId: id, parentId: actor.assignmentId },
          `Review onboarding · ${e.legal_name}`,
        );
        reviews.push(review.id);
      }
      await c.query(
        "UPDATE people_lifecycle_cases SET version=version+1,updated_at=$2 WHERE id=$1",
        [id, Date.now()],
      );
      await lifecycleOutbox(
        c,
        actor,
        key,
        "people.lifecycle.review-requested",
        id,
        version + 1,
      );
      return { id, reviews };
    },
  );
}
export async function applyLifecycleReview(
  c: PoolClient,
  actor: IamActor,
  kind: string,
  p: Record<string, unknown>,
  decision: "approve" | "reject" = "approve",
) {
  const row = (
    await c.query(
      "SELECT * FROM people_lifecycle_cases WHERE company_id=$1 AND id=$2 AND status='pending_review' FOR UPDATE",
      [actor.companyId, p.caseId],
    )
  ).rows[0] as LifecycleCase | undefined;
  if (!row) fail("The lifecycle request was changed or cancelled.");
  if (decision === "reject") {
    await c.query(
      "UPDATE people_lifecycle_cases SET status='blocked',last_error='ACCESS_REVIEW_REJECTED',version=version+1,updated_at=$2 WHERE id=$1",
      [row.id, Date.now()],
    );
    await c.query(
      "UPDATE assignments SET revoked_at=$2,version=version+1 WHERE id IN (SELECT assignment_id FROM people_lifecycle_grants WHERE case_id=$1) AND revoked_at IS NULL",
      [row.id, Date.now()],
    );
    await c.query(
      "UPDATE iam_reviews SET status='failed',version=version+1 WHERE company_id=$1 AND payload->>'caseId'=$2 AND status='pending'",
      [actor.companyId, row.id],
    );
    return;
  }
  if (
    (
      await c.query("SELECT people_person_user($1,$2,$3) same", [
        actor.companyId,
        row.person_id,
        actor.userId,
      ])
    ).rows[0].same
  )
    fail(
      "You cannot approve your own employment access.",
      "SELF_ACCESS_CHANGE",
      403,
    );
  if (
    (
      await c.query("SELECT iam_same_person($1,$2,$3) same", [
        actor.userId,
        row.created_by,
        actor.companyId,
      ])
    ).rows[0].same
  )
    fail(
      "An independent reviewer must approve this HR request.",
      "INDEPENDENT_REVIEW_REQUIRED",
      403,
    );
  if (kind === "people.lifecycle.grant") {
    const g = (
      await c.query(
        "SELECT * FROM people_lifecycle_grants WHERE id=$1 AND company_id=$2 AND case_id=$3 FOR UPDATE",
        [p.grantId, actor.companyId, row.id],
      )
    ).rows[0];
    if (!g || g.assignment_id)
      fail("This access request was already processed.");
    const grant = grantInput.parse(g.definition);
    if (grant.validUntil <= Math.max(Date.now(), row.effective_at))
      fail("Refresh the expired access request.");
    if (
      !(
        await c.query("SELECT people_person_user($1,$2,$3) ok", [
          actor.companyId,
          row.person_id,
          grant.userId,
        ])
      ).rows[0].ok
    )
      fail("The verified account link changed.");
    await validateIamGrant(c, actor, grant, String(p.parentId));
    const assignment = await insertIamGrant(
      c,
      actor,
      grant,
      String(p.parentId),
      "lifecycle",
      g.id,
    );
    await c.query(
      "UPDATE assignments SET valid_from=GREATEST(valid_from,$2) WHERE id=$1",
      [assignment, row.effective_at],
    );
    await c.query(
      "UPDATE people_lifecycle_grants SET assignment_id=$2 WHERE id=$1",
      [g.id, assignment],
    );
  }
  if (
    !(
      await c.query(
        "SELECT 1 FROM people_lifecycle_grants WHERE case_id=$1 AND assignment_id IS NULL",
        [row.id],
      )
    ).rowCount
  )
    await c.query(
      "UPDATE people_lifecycle_cases SET status='scheduled',version=version+1,updated_at=$2 WHERE id=$1",
      [row.id, Date.now()],
    );
}

export async function peopleLifecycleWork(
  db: SqlDatabase,
  actor: WorkContext,
): Promise<WorkItem[]> {
  if (
    !["people.manage", "iam.read", "organization.manage"].some((p) =>
      hasPermission(actor, p),
    )
  )
    return [];
  const data = await peopleLifecycleData(db, actor);
  const tasks: WorkItem[] = data.tasks
    .filter((t) => t.status === "pending" && !t.clearance_type)
    .map((t) => ({
      id: t.id,
      type: "people_lifecycle",
      title: t.title,
      detail: t.assignee_id ? "IT preparation" : "Unassigned IT queue",
      href: t.position_id
        ? `/people/organization?position=${encodeURIComponent(t.position_id)}`
        : `/people?case=${encodeURIComponent(t.case_id!)}`,
      status: t.status,
      updatedAt: t.updated_at,
      canAct: data.canPrepare && t.assignee_id === actor.userId,
      category: t.assignee_id ? "operational" : "setup",
      setupReason: t.assignee_id ? null : "Assign an IT owner",
      queue:
        data.canPrepare && (!t.assignee_id || t.assignee_id === actor.userId)
          ? "todo"
          : "waiting",
      responsibleQueue: t.assignee_id ?? "Unassigned IT",
      relationships: t.assignee_id === actor.userId ? ["assigned"] : ["team"],
    }));
  tasks.push(
    ...data.cases
      .filter((c) => !["completed", "cancelled"].includes(c.status))
      .map((c) => ({
        id: c.id,
        type: "people_lifecycle" as const,
        title: `${c.kind} · ${c.definition.reason}`,
        blocker: c.last_error ?? null,
        detail:
          c.status === "waiting_it"
            ? "Waiting for position preparation or verified identity"
            : c.status.replaceAll("_", " "),
        href: `/people?case=${encodeURIComponent(c.id)}`,
        status: c.status,
        updatedAt: c.updated_at,
        dueAt: c.effective_at,
        queue:
          c.status === "pending_review" && data.canReview
            ? ("todo" as const)
            : ("waiting" as const),
        responsibleQueue:
          c.status === "pending_review" ? "Access reviewers" : "HR / IT",
        relationships:
          c.created_by === actor.userId
            ? ["requested" as const]
            : ["team" as const],
      })),
  );
  if (
    hasPermission(actor, "iam.review") &&
    !actor.branchId &&
    !actor.departmentId
  ) {
    const reviews = (
      await iamDb(db)
        .prepare(
          "SELECT id,reason,created_at FROM iam_reviews WHERE company_id=? AND status='pending' ",
        )
        .bind(actor.companyId)
        .all<{ id: string; reason: string; created_at: number }>()
    ).results;
    tasks.push(
      ...reviews.map((r) => ({
        id: r.id,
        type: "people_lifecycle" as const,
        title: r.reason,
        detail: "Independent access review",
        href: "/people/settings?section=reviews",
        status: "pending_review",
        updatedAt: r.created_at,
        queue: "todo" as const,
        responsibleQueue: "Access reviewers",
        relationships: ["approval" as const],
      })),
    );
  }
  if (
    hasPermission(actor, "iam.read") &&
    !actor.branchId &&
    !actor.departmentId
  ) {
    const operations = (
      await iamDb(db)
        .prepare(
          "SELECT id,status,definition,last_error,updated_at,created_by FROM people_directory_operations WHERE company_id=? AND status NOT IN ('completed','cancelled','rejected') ORDER BY created_at",
        )
        .bind(actor.companyId)
        .all<{
          id: string;
          status: string;
          definition: { reason: string; action: { kind: string } };
          last_error: string | null;
          updated_at: number;
          created_by: string;
        }>()
    ).results;
    tasks.push(
      ...operations.map((op) => ({
        id: op.id,
        type: "people_lifecycle" as const,
        title: op.definition.reason,
        detail: "Directory · " + op.definition.action.kind + " · " + op.status,
        blocker: op.last_error,
        href: "/people/access?view=devices",
        status: op.status,
        updatedAt: op.updated_at,
        queue:
          op.status === "blocked" && data.canPrepare
            ? ("todo" as const)
            : ("waiting" as const),
        responsibleQueue:
          op.status === "pending_review" ? "Access reviewers" : "IT directory",
        relationships:
          op.created_by === actor.userId
            ? ["requested" as const]
            : ["team" as const],
      })),
    );
  }
  return tasks;
}

export const lifecycleSettingsInput = z.object({
  expectedVersion: z.number().int().nonnegative(),
  itAssigneeId: z.string().min(1).nullable(),
  onboardingTasks: z.array(z.string().min(2).max(300)).max(30),
  offboardingTasks: z.array(z.string().min(2).max(300)).max(30),
});
export const rehireInput = z.object({
  personId: z.string().min(1),
  engagementId: z.string().min(1),
  positionId: z.string().min(1),
  startDate: z.iso.date(),
  reason: z.string().min(3).max(1000),
});
export async function createRehireEngagement(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof rehireInput>,
  key: string,
) {
  actor = requirePermission(actor, "people.manage");
  const id = `eng-rehire-${key}`,
    operation = "people.lifecycle.rehire";
  const prior = await replayCommand(db, actor, key, operation, id, input);
  if (prior) return prior;
  const guard = await organizationGuard(db, actor.companyId, key);
  const old = await db
    .prepare(
      "SELECT e.*,p.status person_status FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=? AND e.id=? AND e.person_id=? AND p.merged_into_id IS NULL",
    )
    .bind(actor.companyId, input.engagementId, input.personId)
    .first<{
      version: number;
      status: string;
      person_status: string;
      branch_id: string | null;
      department_id: string | null;
      worker_type: string;
      employment_type: string;
      currency: string;
    }>();
  if (!old || !visible(actor, old))
    fail(
      "Choose the previous engagement in your workspace.",
      "ENGAGEMENT_NOT_FOUND",
      404,
    );
  if (old.status !== "ended" || old.person_status === "archived")
    fail(
      "End the previous engagement and restore an archived profile before rehiring.",
    );
  const position = await db
    .prepare(
      "SELECT * FROM organization_positions WHERE company_id=? AND id=? AND active=1",
    )
    .bind(actor.companyId, input.positionId)
    .first<{
      branch_id: string | null;
      department_id: string | null;
      level_id: string;
      title: string;
    }>();
  if (
    !position ||
    !visible(actor, {
      branch_id: position.branch_id ?? old.branch_id,
      department_id: position.department_id ?? old.department_id,
    })
  )
    fail("Choose a position in your workspace.", "POSITION_REQUIRED", 422);
  const now = Date.now();
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    1,
    { id, personId: input.personId, status: "onboarding" },
    [
      guard,
      assertion(
        db,
        key,
        "new-engagement",
        "NOT EXISTS(SELECT 1 FROM hr_engagements WHERE company_id=? AND person_id=? AND status!='ended')",
        [actor.companyId, input.personId],
      ),
      db
        .prepare(
          "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,position_id,level_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'onboarding',?,?,?,?,?,?)",
        )
        .bind(
          id,
          input.personId,
          actor.companyId,
          position.branch_id ?? old.branch_id,
          position.department_id ?? old.department_id,
          input.positionId,
          position.level_id,
          old.worker_type,
          old.employment_type,
          input.startDate,
          position.title,
          old.currency,
          actor.userId,
          now,
          now,
        ),
      db
        .prepare(
          "UPDATE hr_people SET status='active',version=version+1,updated_at=? WHERE company_id=? AND id=?",
        )
        .bind(now, actor.companyId, input.personId),
    ],
  );
}
export async function saveLifecycleSettings(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof lifecycleSettingsInput>,
  key: string,
) {
  actor = requirePermission(actor, "organization.manage");
  company(actor);
  return executeCommand(
    db,
    actor,
    key,
    "people.lifecycle.settings",
    input,
    actor.companyId,
    input.expectedVersion + 1,
    { version: input.expectedVersion + 1 },
    [
      assertion(
        db,
        key,
        "settings-version",
        "COALESCE((SELECT version FROM people_lifecycle_settings WHERE company_id=?),0)=?",
        [actor.companyId, input.expectedVersion],
      ),
      assertion(
        db,
        key,
        "it-assignee",
        "?::text IS NULL OR EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))",
        [input.itAssigneeId, actor.companyId, input.itAssigneeId],
      ),
      db
        .prepare(
          "INSERT INTO people_lifecycle_settings VALUES(?,?,?) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,definition=excluded.definition",
        )
        .bind(
          actor.companyId,
          input.expectedVersion + 1,
          JSON.stringify(input),
        ),
      db
        .prepare(
          "UPDATE people_preparation_tasks SET assignee_id=?,version=version+1 WHERE company_id=? AND assignee_id IS NULL AND status='pending' AND clearance_type IS NULL",
        )
        .bind(input.itAssigneeId, actor.companyId),
    ],
  );
}
export async function completePreparationTask(
  db: PostgresDatabase,
  actor: WorkContext,
  id: string,
  version: number,
  key: string,
) {
  return calendarCommand(db, actor, {action:"task.complete",id,expectedVersion:version},key);
}
