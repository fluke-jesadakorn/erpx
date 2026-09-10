"use client";
import { useRouter } from "next/navigation";
import { useSocialCommand } from "@/components/beta/social-command";

export function RevokeCompanyAccess({
  base,
  userId,
  onRevoked,
}: {
  base: string;
  userId: string;
  onRevoked?: () => void;
}) {
  const router = useRouter();
  const { run, send, busy, message } = useSocialCommand();
  return (
    <details>
      <summary>Company access</summary>
      <p>
        Revoke every role in this company. The employee record and history
        remain; reactivation does not restore revoked roles.
      </p>
      <button
        className="button secondary"
        disabled={busy}
        onClick={() =>
          void run(async () => {
            await send(`/api/v1${base}/beta/revoke`, { userId });
            onRevoked?.();
            router.refresh();
          })
        }
      >
        Revoke company access
      </button>
      {message && <p role="alert">{message}</p>}
    </details>
  );
}
