import Link from "next/link";
import type { WorkContext } from "@/server/authorization";
import { runtimeEnv } from "@/server/runtime";
import { StatusBadge } from "@/components/ui/primitives";
import { AccountInvitationActions } from "./account-invitation-actions";
import { TimeText } from "@/components/time/time-preference-context";

const deliveryLabels: Record<string, string> = {
  awaiting_approval: "Waiting for independent approval",
  queued: "Queued for email delivery",
  running: "Sending email",
  sent: "Email sent",
  failed: "Email delivery failed",
  cancelled: "Delivery cancelled",
};

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
    "SELECT i.id,i.person_id,i.email,CASE WHEN i.status='pending' AND i.expires_at<=(floor(extract(epoch FROM clock_timestamp()))::bigint*1000) THEN 'expired' ELSE i.status END status,i.expires_at,d.state delivery_state,d.attempts,d.error,d.sent_at FROM account_invitations i LEFT JOIN account_invitation_delivery_jobs d ON d.invitation_id=i.id WHERE i.holding_id=? AND i.company_id=? AND (?::text IS NULL OR i.branch_id=?) ORDER BY i.created_at DESC LIMIT 100",
  )
    .bind(actor.holdingId, actor.companyId, actor.branchId, actor.branchId)
    .all<{
      id: string;
      person_id: string;
      email: string;
      status: string;
      expires_at: number;
      delivery_state: string | null;
      attempts: number | null;
      error: string | null;
      sent_at: number | null;
    }>();
  return (
    <section className="card">
      <h2>Activation invitations</h2>
      {!invitations.results.length && <p>No activation invitations yet.</p>}
      {invitations.results.map((invite) => (
        <div className="row" key={invite.id}>
          <div className="row-main">
            <Link href={`${base}/people/${invite.person_id}?tab=access`}>
              {invite.email}
            </Link>
            <p className="muted">
              {invite.delivery_state
                ? deliveryLabels[invite.delivery_state] ?? invite.delivery_state
                : "Legacy invitation — no delivery job"}
              {invite.attempts ? ` · ${invite.attempts} attempt${invite.attempts === 1 ? "" : "s"}` : ""}
              {invite.sent_at ? <> · <TimeText value={invite.sent_at} /></> : ""}
            </p>
            {invite.error && <p role="alert">{invite.error.replaceAll("_", " ")}</p>}
          </div>
          <StatusBadge status={invite.status} />
          <AccountInvitationActions
            base={base}
            personId={invite.person_id}
            invitationId={invite.id}
            invitationStatus={invite.status}
            deliveryState={invite.delivery_state}
          />
        </div>
      ))}
    </section>
  );
}
