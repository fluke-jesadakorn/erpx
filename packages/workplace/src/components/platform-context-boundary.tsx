"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button } from "@erpx/ui/components/foundation";

import { useEffect, useState } from "react";
import { useContextSwitch } from "@erpx/workplace/components/context-switch-provider";

export function PlatformContextBoundary({
  userId,
  assignmentId,
  children,
}: {
  userId: string;
  assignmentId: string;
  children: React.ReactNode;
}) {
  const [stale, setStale] = useState(false);
  const { switchContext, busy } = useContextSwitch();
  useEffect(() => {
    let active = true;
    const channel =
      typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("erpx-work-context");
    async function check() {
      try {
        const response = await fetch("/api/v1/platform/assignments/active", { cache: "no-store" });
        const body = await response.json();
        if (
          active &&
          (!response.ok || body.data.userId !== userId || body.data.assignmentId !== assignmentId)
        )
          setStale(true);
      } catch {
        /* Server validation guards writes; focus retries. */
      }
    }
    channel?.addEventListener("message", check);
    window.addEventListener("focus", check);
    // A context change can arrive before this server-rendered view hydrates.
    void check();
    return () => {
      active = false;
      channel?.close();
      window.removeEventListener("focus", check);
    };
  }, [userId, assignmentId]);
  return (
    <div
      data-platform-user={userId}
      data-platform-assignment={assignmentId}
      data-platform-stale={String(stale)}
    >
      {stale && (
        <InlineNotice tone="info" role="status" className="content">
          Platform role changed. Reload and review before continuing.{" "}
          <Button appearance="standard"
            variant="ghost"
            type="button"
            disabled={busy}
            onClick={() =>
              void switchContext({
                companyId:
                  document.querySelector<HTMLElement>("[data-work-company]")?.dataset.workCompany,
                action: async () => {},
              })
            }
          >
            Reload
          </Button>
        </InlineNotice>
      )}
      <div hidden={stale} inert={stale}>
        {children}
      </div>
    </div>
  );
}
