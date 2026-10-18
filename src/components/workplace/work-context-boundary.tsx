"use client";
import { Button } from "@/components/ui/foundation";

import { useEffect, useRef, useState } from "react";
import { useContextSwitch } from "./context-switch-provider";
import { type UiAppearance } from "@/components/ui/foundation";

/** Minimal public SSR identity for deduplicating this mount only. It is never
 * command authority, a session token, IAM data or a cross-request cache. */
export type InitialWorkContextSnapshot = {
  version: 1;
  base: string;
  userId: string;
  companyId: string;
  assignmentId: string;
  revision: number;
  checkedAt: number;
  frontApp?: "pos" | "pms";
  frontLocation?: string;
  frontRoleEndpoint?: string;
};
const initialSnapshotMaxAgeMs = 30_000;

export function WorkContextBoundary({ appearance = "legacy",
  base,
  userId,
  companyId,
  assignmentId,
  revision,
  frontApp,
  frontLocation,
  frontRoleEndpoint,
  initialCurrent,
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
  initialCurrent?: InitialWorkContextSnapshot;
  children: React.ReactNode;
}) & { appearance?: UiAppearance }) {
  const { busy, switchContext } = useContextSwitch();
  const [stale, setStale] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const initialSnapshot = useRef(initialCurrent);
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
    // Only the original mount snapshot may suppress a duplicate read. Later
    // SSR props cannot clear stale state or reset the live dirty form tracking.
    const initial = initialSnapshot.current, now = Date.now();
    const matchesInitial = initial?.version === 1 && !!userId &&
      initial.base === base && initial.userId === userId &&
      initial.companyId === companyId && initial.assignmentId === assignmentId &&
      initial.revision === revision && initial.frontApp === frontApp &&
      initial.frontLocation === frontLocation && initial.frontRoleEndpoint === frontRoleEndpoint &&
      Number.isSafeInteger(initial.checkedAt) && initial.checkedAt > 0 &&
      now >= initial.checkedAt && now - initial.checkedAt <= initialSnapshotMaxAgeMs;
    if (!matchesInitial) {
      // A rejected seed can never become eligible again in this mounted
      // lifetime, even if the user/source returns to its original values.
      initialSnapshot.current = undefined;
      void check();
    }
    const node = root.current;
    const editedForms = new Set<HTMLFormElement>();
    let nonFormEdits = false;
    function syncDirty() {
      for (const form of editedForms) if (!node?.contains(form)) editedForms.delete(form);
      if (nonFormEdits || editedForms.size) node?.setAttribute("data-work-dirty", "true");
      else node?.removeAttribute("data-work-dirty");
    }
    function dirty(event: Event) {
      const target = event.target as HTMLElement;
      if (
        target.matches('input[type="search"], [role="combobox"]') ||
        target.closest('[data-work-dirty="false"]')
      )
        return;
      const form = target.closest("form");
      if (form) {
        form.setAttribute("data-erpx-edit-version", crypto.randomUUID());
        editedForms.add(form);
        syncDirty();
      } else if (target.matches("textarea, [contenteditable=true]")) {
        nonFormEdits = true;
        syncDirty();
      }
    }
    function saved(event:Event) {
      const detail=(event as CustomEvent<{form?:HTMLFormElement;formEditVersion?:string|null}>).detail;
      if (detail?.form && detail.formEditVersion && detail.formEditVersion===detail.form.getAttribute("data-erpx-edit-version")) editedForms.delete(detail.form);
      syncDirty();
    }
    const removedForms = new MutationObserver(syncDirty);
    if(node)removedForms.observe(node,{childList:true,subtree:true});
    node?.addEventListener("input", dirty);
    window.addEventListener("erpx:record-command-saved", saved);
    return () => {
      active = false;
      channel?.close();
      window.removeEventListener("focus", check);
      window.removeEventListener("erpx:work-context-changed", check);
      node?.removeEventListener("input", dirty);
      removedForms.disconnect();
      window.removeEventListener("erpx:record-command-saved", saved);
    };
  }, [base, userId, companyId, assignmentId, revision, frontApp, frontRoleEndpoint, frontLocation]);
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
