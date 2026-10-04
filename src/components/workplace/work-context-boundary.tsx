"use client";
import { Button } from "@/components/ui/foundation";

import { useEffect, useRef, useState } from "react";
import { useContextSwitch } from "./context-switch-provider";
import { type UiAppearance } from "@/components/ui/foundation";

export function WorkContextBoundary({ appearance = "legacy",
  base,
  userId,
  companyId,
  assignmentId,
  revision,
  frontApp,
  frontLocation,
  frontRoleEndpoint,
  children,
}: ({
  base: string;
  userId?: string;
  companyId: string;
  assignmentId: string;
  revision: number;
  frontApp?: "pos" | "pms";
  frontLocation?: string;
  frontRoleEndpoint?: string;
  children: React.ReactNode;
}) & { appearance?: UiAppearance }) {
  const { busy, switchContext } = useContextSwitch();
  const [stale, setStale] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let active = true;
    const channel =
      typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("erpx-work-context");
    async function check() {
      try {
        const response = await fetch(
          frontRoleEndpoint ? `${frontRoleEndpoint}/roles` : `/api/v1${base}/assignments/active`,
          {
            cache: "no-store",
            headers: frontRoleEndpoint
              ? { "x-erpx-front-location": frontLocation ?? "holding" }
              : { "x-erpx-context-company": companyId },
          },
        );
        if (!active) return;
        if (response.status === 401) {
          window.location.replace(
            "/login?reason=expired&next=" +
              encodeURIComponent(
                window.location.pathname + window.location.search + window.location.hash,
              ),
          );
          return;
        }
        if (!response.ok) {
          setStale(true);
          return;
        }
        const { data } = await response.json();
        if (
          active &&
          (!data.valid ||
            data.revision !== revision ||
            data.assignmentId !== assignmentId ||
            data.companyId !== companyId ||
            (userId && data.userId !== userId))
        )
          setStale(true);
      } catch {
        /* Focus retries revalidation; server context checks still guard writes. */
      }
    }
    channel?.addEventListener("message", check);
    window.addEventListener("focus", check);
    window.addEventListener("erpx:work-context-changed", check);
    void check();
    const node = root.current;
    function dirty(event: Event) {
      const target = event.target as HTMLElement;
      if (
        target.matches('input[type="search"], [role="combobox"]') ||
        target.closest('[data-work-dirty="false"]')
      )
        return;
      if (target.closest("form") || target.matches("textarea, [contenteditable=true]"))
        node?.setAttribute("data-work-dirty", "true");
    }
    node?.addEventListener("input", dirty);
    return () => {
      active = false;
      channel?.close();
      window.removeEventListener("focus", check);
      window.removeEventListener("erpx:work-context-changed", check);
      node?.removeEventListener("input", dirty);
    };
  }, [base, userId, companyId, assignmentId, revision, frontRoleEndpoint, frontLocation]);
  return (
    <div data-ui-appearance={appearance}
      ref={root}
      data-work-user={userId}
      data-work-company={companyId}
      data-work-assignment={assignmentId}
      data-work-revision={revision}
      data-work-stale={String(stale)}
      data-front-app={frontApp}
      data-front-location={frontLocation}
    >
      {stale && (
        <div data-ui-appearance={appearance}
          role="status"
          style={{
            padding: "8px 20px",
            fontSize: 12,
            borderBottom: "1px solid var(--ui-line)",
            color: "var(--ui-muted)",
          }}
        >
          {frontApp
            ? "สิทธิ์หรือบทบาทงานเปลี่ยนไป รายการที่เปิดอยู่ยังไม่ได้ส่ง กรุณาโหลดหน้าใหม่เพื่อตรวจสอบ"
            : "Work role changed. Your open form has not been submitted. Reload and review before continuing."}{" "}
          <Button appearance={appearance}
            type="submit"
            variant="plain"
            disabled={busy}
            onClick={() =>
              void switchContext({ companyId, inspect: !frontApp, action: async () => {} })
            }
          >
            {frontApp ? "โหลดใหม่" : "Reload"}
          </Button>
        </div>
      )}
      <div data-ui-appearance={appearance} hidden={stale} inert={stale}>
        {children}
      </div>
    </div>
  );
}
