import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

export type SeedPositionInput = {
  title: string;
  branchId: string | null;
  departmentId: string | null;
  levelId: string;
  reportsToPositionId: string | null;
  isCeo: boolean;
  rosterRequired?: boolean;
  expectedVersion: number;
  reason: string;
  requirements?: unknown;
};

/** Native-shape position seeding for integration fixtures. The production
 * command is served by the native Organization position router. */
export async function savePosition(
  database: SqlDatabase,
  actor: { companyId: string; userId: string },
  id: string | null,
  input: SeedPositionInput,
  commandKey: string,
) {
  const recordId = id ?? `position-${commandKey}`,
    now = Date.now();
  await database
    .prepare(
      `INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,roster_required,active,version,created_by,created_at,updated_at,requirements)
       VALUES(?,?,?,?,?,?,?,?,?,1,1,?,?,?,?)
       ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,title=excluded.title,level_id=excluded.level_id,reports_to_position_id=excluded.reports_to_position_id,is_ceo=excluded.is_ceo,roster_required=excluded.roster_required,active=1,version=organization_positions.version+1,updated_at=excluded.updated_at,requirements=excluded.requirements`,
    )
    .bind(
      recordId,
      actor.companyId,
      input.branchId,
      input.departmentId,
      input.title,
      input.levelId,
      input.reportsToPositionId,
      input.isCeo ? 1 : 0,
      input.rosterRequired ? 1 : 0,
      actor.userId,
      now,
      now,
      input.requirements === undefined
        ? null
        : typeof input.requirements === "string"
          ? input.requirements
          : JSON.stringify(input.requirements),
    )
    .run();
  return { id: recordId, version: 1 };
}
