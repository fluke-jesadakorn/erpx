import Link from "next/link";
import type { WorkContext } from "@/server/authorization";
import { runtimeEnv } from "@/server/runtime";

export async function AccountInvitations({
  actor,
  base,
}: {
  actor: WorkContext;
  base: string;
}) {
  if (actor.role !== "holding_owner") return null;
  const env = await runtimeEnv();
  const invitations = await env.CONTROL_DB.prepare(
    "SELECT id,person_id,email,CASE WHEN status='pending' AND expires_at<=(floor(extract(epoch FROM clock_timestamp()))::bigint*1000) THEN 'Expired' ELSE status END status,expires_at FROM account_invitations WHERE holding_id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY created_at DESC LIMIT 100",
  )
    .bind(actor.holdingId, actor.companyId, actor.branchId, actor.branchId)
    .all<{
      id: string;
      person_id: string;
      email: string;
      status: string;
      expires_at: number;
    }>();
  return (
    <section className="card">
      <h2>Activation invitations</h2>
      {!invitations.results.length && <p>No activation invitations yet.</p>}
      {invitations.results.map((invite) => (
        <div className="row" key={invite.id}>
          <Link href={`${base}/people/${invite.person_id}?tab=access`}>
            {invite.email}
          </Link>
          <span>{invite.status}</span>
        </div>
      ))}
    </section>
  );
}
