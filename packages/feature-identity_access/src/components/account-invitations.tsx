import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { accountInvitationList } from "@erpx/server-identity_access/public/account-invitation-list";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { AccountInvitationActions } from "@erpx/feature-identity_access/components/account-invitation-actions";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { Card } from "@erpx/ui/components/foundation";

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
  const invitations = await accountInvitationList(actor);
  if (invitations === null) return null;
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Activation invitations</h2>
      {!invitations.length && <p>No activation invitations yet.</p>}
      {invitations.map((invite) => (
        <div className="ui-record-item" key={invite.id}>
          <div className="row-main">
            <Link href={canonicalUiHref(`${base}/technology?view=resources&section=accounts&subject=${encodeURIComponent(invite.person_id)}`)}>
              {invite.email}
            </Link>
            <p className="muted">
              {invite.delivery_state
                ? deliveryLabels[invite.delivery_state] ?? invite.delivery_state
                : "Legacy invitation — no delivery job"}
              {invite.attempts ? ` · ${invite.attempts} attempt${invite.attempts === 1 ? "" : "s"}` : ""}
              {invite.sent_at ? <> · <TimeText appearance="standard" value={invite.sent_at} /></> : ""}
            </p>
            {invite.error && <InlineNotice tone="danger" role="alert">{invite.error.replaceAll("_", " ")}</InlineNotice>}
          </div>
          <StatusBadge appearance="standard" status={invite.status} />
          <AccountInvitationActions
            base={base}
            personId={invite.person_id}
            invitationId={invite.id}
            invitationStatus={invite.status}
            deliveryState={invite.delivery_state}
          />
        </div>
      ))}
    </Card>
  );
}
