"use client";
import { Button } from "@/components/ui/foundation";

import { useRouter } from "next/navigation";
import { useSocialCommand } from "@/components/collaboration/social-command";
import { type UiAppearance } from "@/components/ui/foundation";

export function RevokeCompanyAccess({ appearance = "legacy",
  base,
  userId,
  onRevoked,
}: ({
  base: string;
  userId: string;
  onRevoked?: () => void;
}) & { appearance?: UiAppearance }) {
  const router = useRouter();
  const { run, send, busy, message } = useSocialCommand();
  return (
    <details data-ui-appearance={appearance}>
      <summary data-ui-appearance={appearance}>Company access</summary>
      <p data-ui-appearance={appearance}>
        Revoke every role in this company. The employee record and history remain; reactivation does
        not restore revoked roles.
      </p>
      <Button appearance={appearance}
        type="submit"
        variant="secondary"

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
      </Button>
      {message && <p data-ui-appearance={appearance} role="alert">{message}</p>}
    </details>
  );
}
