"use client";

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
        <button
          className="button secondary"
          disabled={busy}
          onClick={() => void send(`${path}/retry`, {})}
        >
          Retry delivery
        </button>
      )}
      {invitationStatus === "pending" && (
        <button
          className="button secondary"
          disabled={busy}
          onClick={() => void send(`${path}/revoke`, {})}
        >
          Revoke invitation
        </button>
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
