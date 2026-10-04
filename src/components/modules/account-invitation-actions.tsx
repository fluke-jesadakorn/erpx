"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { useStaffCommand } from "./staff-work-controls";

export function AccountInvitationActions({
  base,
  personId,
  invitationId,
  invitationStatus,
  deliveryState,
}: {
  base: string;
  personId: string;
  invitationId: string;
  invitationStatus: string;
  deliveryState: string | null;
}) {
  const { send, busy, error } = useStaffCommand(base);
  const path = `people/directory/${personId}/invitations/${invitationId}`;
  return (
    <div>
      {deliveryState === "failed" && invitationStatus === "pending" && (
        <Button appearance="standard"
          type="submit"
          variant="secondary"

          disabled={busy}
          onClick={() => void send(`${path}/retry`, {})}
        >
          Retry delivery
        </Button>
      )}
      {invitationStatus === "pending" && (
        <Button appearance="standard"
          type="submit"
          variant="secondary"

          disabled={busy}
          onClick={() => void send(`${path}/revoke`, {})}
        >
          Revoke invitation
        </Button>
      )}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </div>
  );
}
