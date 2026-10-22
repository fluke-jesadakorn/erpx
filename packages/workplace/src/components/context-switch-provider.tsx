"use client";
import { Button } from "@erpx/ui/components/foundation";

import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useTransition,
} from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { ContextSwitchContext as Context, type SwitchRequest } from "@erpx/workplace/components/context-switch-context";
export { useContextSwitch } from "@erpx/workplace/components/context-switch-context";
import { WorkspaceMenu } from "@erpx/workplace/components/workspace-menu";
import {
  confirmContextSwitch,
  contextSwitchBlockReason,
  setContextSwitching,
  announceContextSwitch,
} from "@erpx/feature-work_projection/lib/context-switch";
import { waitForWorkspaceMemory } from "@erpx/feature-work_projection/lib/workspace-memory-client";
import type { AccountWorkContext } from "@erpx/feature-identity_access/lib/account-work-context";
import { type UiAppearance } from "@erpx/ui/components/foundation";

export function ContextSwitchProvider({ appearance = "legacy", children }: ({ children: React.ReactNode }) & { appearance?: UiAppearance }) {
  const router = useRouter();
  const lock = useRef(false);
  const resetAfterRefresh = useRef(false);
  const restoreScroll = useRef<{ x: number; y: number } | null>(null);
  const view = useRef<HTMLDivElement>(null);
  const [generation, setGeneration] = useState(0);
  const [working, setWorking] = useState(false);
  const [refreshing, startTransition] = useTransition();
  const [resolution, setResolution] = useState<AccountWorkContext | null>(null);
  const [error, setError] = useState("");
  const busy = working || refreshing;
  useEffect(() => {
    if (!busy && lock.current) {
      lock.current = false;
      setContextSwitching(false);
      if (resetAfterRefresh.current) {
        resetAfterRefresh.current = false;
        // Next's forbidden boundary otherwise retains its 403 on the same URL.
        // Remount only after the new server payload has committed.
        setGeneration((value) => value + 1);
      }
      window.dispatchEvent(new Event("erpx:context-switch-finished"));
    }
  }, [busy]);
  useLayoutEffect(() => {
    const position = restoreScroll.current;
    if (!position) return;
    const { x, y } = position;
    const source = window.location.href;
    let stopped = false;
    let frame = 0;
    const observer = new ResizeObserver(restore);
    function stop() {
      stopped = true;
      observer.disconnect();
      cancelAnimationFrame(frame);
      window.removeEventListener("scroll", restore);
      for (const event of ["wheel", "touchstart", "pointerdown", "keydown"])
        window.removeEventListener(event, stop);
      if (restoreScroll.current === position) restoreScroll.current = null;
    }
    function restore() {
      if (stopped) return;
      if (window.location.href !== source) {
        stop();
        return;
      }
      if (Math.abs(window.scrollX - x) <= 1 && Math.abs(window.scrollY - y) <= 1) return;
      window.scrollTo(x, y);
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (stopped) return;
        window.scrollTo(x, y);
      });
    }
    // Async client loading and router focus can still alter scroll after the
    // first committed frame. Preserve it until navigation or user interaction.
    if (view.current) observer.observe(view.current);
    window.addEventListener("scroll", restore, { passive: true });
    for (const event of ["wheel", "touchstart", "pointerdown", "keydown"])
      window.addEventListener(event, stop, { passive: true });
    restore();
    return stop;
  }, [generation]);

  async function switchContext({
    action,
    companyId,
    from,
    returnTo,
    onCommitted,
    scroll,
    inspect = true,
  }: SwitchRequest) {
    if (lock.current) return false;
    const blockedReason = contextSwitchBlockReason();
    if (blockedReason) { setError(blockedReason); return false; }
    setError("");
    if (!confirmContextSwitch()) return false;
    lock.current = true;
    setWorking(true);
    setError("");
    setResolution(null);
    setContextSwitching(true);
    let committed = false;
    try {
      await waitForWorkspaceMemory();
      await action();
      committed = true;
      resetAfterRefresh.current = true;
      restoreScroll.current = scroll ?? { x: window.scrollX, y: window.scrollY };
      announceContextSwitch();
      onCommitted?.();
      if (!inspect) {
        startTransition(() => {
          if (returnTo) router.replace(returnTo, { scroll: false });
          router.refresh();
        });
        return true;
      }
      const source =
        from ?? window.location.pathname + window.location.search + window.location.hash;
      const query = new URLSearchParams({ from: source });
      if (companyId) query.set("companyId", companyId);
      const response = await fetch(`/api/v1/account/work-context?${query}`, { cache: "no-store" });
      if (response.status === 401) {
        window.location.replace(
          `/login?${new URLSearchParams({ reason: "expired", next: source })}`,
        );
        return true;
      }
      const body = await response.json();
      if (!response.ok)
        throw new Error(body.error?.message ?? "Could not check the selected account's access.");
      const state = body.data as AccountWorkContext;
      setResolution(state);
      startTransition(() => {
        if (returnTo && state.canUseWorkspace) router.replace(returnTo, { scroll: false });
        else router.refresh();
      });
      return true;
    } catch (cause) {
      if (committed) {
        // The session changed even if revalidation failed: discard the old view.
        setError(
          cause instanceof Error ? cause.message : "Access could not be checked. Try again.",
        );
        startTransition(() => router.refresh());
        return true;
      }
      throw cause;
    } finally {
      setWorking(false);
    }
  }

  return (
    <Context.Provider value={{ busy, switchContext }}>
      <div data-ui-appearance={appearance}
        ref={view}
        key={generation}
        inert={busy}
        aria-busy={busy}
        style={busy ? { visibility: "hidden" } : undefined}
      >
        {children}
      </div>
      {busy && (
        <div data-ui-appearance={appearance}
          role="status"
          aria-live="polite"
          style={{
            position: "fixed",
            inset: 0,
            zIndex: 10000,
            display: "grid",
            placeItems: "center",
            background: "var(--ui-paper)",
            color: "var(--ui-ink)",
          }}
        >
          Updating account and permissions…
        </div>
      )}
      {!busy && error && (
        <div data-ui-appearance={appearance} role="alert" className="content">
          {error}{" "}
          <Button appearance={appearance}
            variant="plain"
            type="button"
            onClick={() => startTransition(() => router.refresh())}
          >
            Check again
          </Button>
        </div>
      )}
      {!busy && resolution?.requiresWorkspaceSelection && (
        <div data-ui-appearance={appearance} className="content">
          <p data-ui-appearance={appearance}>Choose a workspace to continue with this account or role.</p>
          <WorkspaceMenu appearance={appearance}
            key={`${resolution.user.id}:${resolution.assignmentId}`}
            label="Choose workspace"
            choices={resolution.choices}
            current={resolution.current ?? { kind: "platform" }}
            canManageErpx={resolution.canManageErpx}
            defaultOpen
            onDismiss={() => setResolution(null)}
          />
        </div>
      )}
    </Context.Provider>
  );
}
