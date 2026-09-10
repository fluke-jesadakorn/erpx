import type { SqlDatabase } from "./runtime/types";
import { z } from "zod";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import {
  activeStaff,
  authorityGuard,
  companyAuthority,
  requireCompanyAuthority,
  staffPeople,
  staffGuard,
  type StaffActor,
} from "./staff-policy";

export const levelSchema = z.object({
  code: z
    .string()
    .trim()
    .min(1)
    .max(30)
    .regex(/^[A-Za-z0-9_-]+$/)
    .transform((s) => s.toUpperCase()),
  name: z.string().trim().min(1).max(100),
  rank: z.number().int().min(-10000).max(10000),
  active: z.boolean().default(true),
  expectedVersion: z.number().int().min(0).default(0),
});
export const levelAssignmentSchema = z.object({
  levelId: z.string().min(1),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(1000),
});
export const headSchema = z.object({
  personId: z.string().min(1),
  expectedVersion: z.number().int().min(0),
  reason: z.string().trim().min(3).max(1000),
});
export async function listLevels(db: SqlDatabase, companyId: string) {
  return (
    await db
      .prepare(
        "SELECT * FROM staff_levels WHERE company_id=? ORDER BY rank DESC,code",
      )
      .bind(companyId)
      .all<{
        id: string;
        company_id: string;
        code: string;
        name: string;
        rank: number;
        active: number;
        version: number;
      }>()
  ).results;
}
export async function staffSettings(db: SqlDatabase, actor: StaffActor) {
  const [levels, people, departments, canManage] = await Promise.all([
    listLevels(db, actor.companyId),
    staffPeople(db, actor.companyId),
    db
      .prepare(
        "SELECT n.id,n.display_name,n.branch_id,h.person_id,h.version,COALESCE(p.legal_name,'Not configured') head_name FROM organization_nodes n LEFT JOIN department_heads h ON h.department_id=n.id AND h.company_id=n.company_id LEFT JOIN hr_people p ON p.id=h.person_id WHERE n.company_id=? AND n.kind='department' AND n.status='active' ORDER BY n.display_name",
      )
      .bind(actor.companyId)
      .all<{
        id: string;
        display_name: string;
        branch_id: string | null;
        person_id: string | null;
        version: number | null;
        head_name: string;
      }>(),
    companyAuthority(db, actor),
  ]);
  return { levels, people, departments: departments.results, canManage };
}
export async function saveLevel(
  db: SqlDatabase,
  actor: StaffActor,
  id: string | null,
  input: z.infer<typeof levelSchema>,
  key: string,
) {
  await requireCompanyAuthority(db, actor);
  const recordId = id ?? `level-${key}`,
    operation = id ? "staff.level.update" : "staff.level.create";
  const prior = await replayCommand(db, actor, key, operation, recordId, input);
  if (prior) return prior;
  const now = Date.now(),
    version = id ? input.expectedVersion + 1 : 1;
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, version },
    [
      authorityGuard(db, key, actor),
      ...(id
        ? [
            assertion(
              db,
              key,
              "level-version",
              "EXISTS(SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND version=?)",
              [id, actor.companyId, input.expectedVersion],
            ),
          ]
        : []),
      id
        ? db
            .prepare(
              "UPDATE staff_levels SET code=?,name=?,rank=?,active=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(
              input.code,
              input.name,
              input.rank,
              input.active ? 1 : 0,
              now,
              id,
            )
        : db
            .prepare(
              "INSERT INTO staff_levels(id,company_id,code,name,rank,active,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              actor.companyId,
              input.code,
              input.name,
              input.rank,
              input.active ? 1 : 0,
              now,
              now,
            ),
    ],
    { companyId: actor.companyId, branchId: null, departmentId: null },
  );
}
export function levelChangeStatements(
  db: SqlDatabase,
  key: string,
  actor: StaffActor,
  engagementId: string,
  levelId: string,
  previous: string | null,
  reason: string,
) {
  return [
    assertion(
      db,
      key,
      "level-active",
      "EXISTS(SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND active=1)",
      [levelId, actor.companyId],
    ),
    ...(levelId === previous
      ? []
      : [
          db
            .prepare(
              "INSERT INTO staff_level_history(id,engagement_id,previous_level_id,level_id,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?)",
            )
            .bind(
              `level-history-${key}`,
              engagementId,
              previous,
              levelId,
              actor.userId,
              reason,
              key,
              Date.now(),
            ),
        ]),
  ];
}
export async function assignLevel(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  input: z.infer<typeof levelAssignmentSchema>,
  key: string,
) {
  await requireCompanyAuthority(db, actor);
  const prior = await replayCommand(
    db,
    actor,
    key,
    "staff.level.assign",
    id,
    input,
  );
  if (prior) return prior;
  const engagement = await db
    .prepare("SELECT * FROM hr_engagements WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<{
      level_id: string | null;
      branch_id: string | null;
      department_id: string | null;
    }>();
  if (!engagement)
    throw new ApiError(404, "ENGAGEMENT_NOT_FOUND", "Engagement not found.");
  return executeCommand(
    db,
    actor,
    key,
    "staff.level.assign",
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1 },
    [
      authorityGuard(db, key, actor),
      assertion(
        db,
        key,
        "engagement-version",
        "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=?)",
        [id, actor.companyId, input.expectedVersion],
      ),
      ...levelChangeStatements(
        db,
        key,
        actor,
        id,
        input.levelId,
        engagement.level_id,
        input.reason,
      ),
      db
        .prepare(
          "UPDATE hr_engagements SET level_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(input.levelId, Date.now(), id),
    ],
    {
      companyId: actor.companyId,
      branchId: engagement.branch_id,
      departmentId: engagement.department_id,
    },
  );
}
export async function setDepartmentHead(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  input: z.infer<typeof headSchema>,
  key: string,
) {
  await requireCompanyAuthority(db, actor);
  const prior = await replayCommand(
    db,
    actor,
    key,
    "staff.department.head",
    id,
    input,
  );
  if (prior) return prior;
  const people = await staffPeople(db, actor.companyId),
    person = people.find((p) => p.person_id === input.personId);
  if (!person)
    throw new ApiError(
      422,
      "HEAD_UNAVAILABLE",
      "Choose active company staff with a linked account.",
    );
  await activeStaff(db, actor.companyId, person.user_id);
  const department = await db
    .prepare(
      "SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=? AND kind='department'",
    )
    .bind(id, actor.companyId)
    .first<{ branch_id: string | null }>();
  return executeCommand(
    db,
    actor,
    key,
    "staff.department.head",
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1 },
    [
      authorityGuard(db, key, actor),
      staffGuard(db, key, actor.companyId, person.user_id),
      assertion(
        db,
        key,
        "department",
        "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active')",
        [id, actor.companyId],
      ),
      assertion(
        db,
        key,
        "head-version",
        "(?=0 AND NOT EXISTS(SELECT 1 FROM department_heads WHERE department_id=?)) OR EXISTS(SELECT 1 FROM department_heads WHERE department_id=? AND version=?)",
        [input.expectedVersion, id, id, input.expectedVersion],
      ),
      db
        .prepare(
          "INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(department_id) DO UPDATE SET person_id=excluded.person_id,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
        )
        .bind(
          id,
          actor.companyId,
          input.personId,
          input.expectedVersion + 1,
          actor.userId,
          Date.now(),
        ),
    ],
    {
      companyId: actor.companyId,
      branchId: department?.branch_id ?? null,
      departmentId: id,
    },
  );
}
