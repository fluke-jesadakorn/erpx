import { runtimeEnv, holdingResources } from "./runtime";
import { canManagePeople, directoryPerson } from "./people-directory";
import type { WorkContext } from "./authorization";
export async function companyMembers(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId),
    env = await runtimeEnv(),
    manager = canManagePeople(actor);
  const members = (
    await write
      .prepare(
        `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IS NOT NULL AND ((revoked_at IS NULL AND valid_from<=? AND valid_until>?) OR ?=1) AND (?::text IS NULL OR scope_kind='holding' OR COALESCE(branch_id,(SELECT branch_id FROM organization_nodes WHERE id=scope_node_id))=?) ORDER BY user_id LIMIT 250`,
      )
      .bind(
        actor.companyId,
        Date.now(),
        Date.now(),
        manager ? 1 : 0,
        actor.branchId,
        actor.branchId,
      )
      .all<{ user_id: string }>()
  ).results;
  const rows: {
    id: string;
    name: string;
    email: string;
    personId: string | null;
  }[] = [];
  for (let offset = 0; offset < members.length; offset += 80) {
    const ids = members.slice(offset, offset + 80).map((m) => m.user_id);
    const accounts = (
      await env.CONTROL_DB.prepare(
        `SELECT id,name,email FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`,
      )
        .bind(...ids)
        .all<{ id: string; name: string; email: string }>()
    ).results;
    for (const account of accounts) {
      const person = await write
        .prepare(
          "SELECT id FROM hr_people WHERE company_id=? AND (user_id=? OR (?=1 AND user_id IS NULL AND lower(email)=?)) AND merged_into_id IS NULL ORDER BY CASE WHEN user_id IS NOT NULL THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(
          actor.companyId,
          account.id,
          manager ? 1 : 0,
          account.email.toLowerCase(),
        )
        .first<{ id: string }>();
      rows.push({
        ...account,
        email: manager || account.id === actor.userId ? account.email : "",
        personId: person?.id ?? null,
      });
    }
  }
  return rows.sort((a, b) => a.name.localeCompare(b.name));
}
export async function memberProfile(
  actor: WorkContext,
  personId: string | null,
) {
  return personId ? directoryPerson(actor, personId) : null;
}
