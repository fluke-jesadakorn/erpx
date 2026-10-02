"use client";

import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { WorkspaceMenu } from "./workspace-menu";
import { confirmContextSwitch, setContextSwitching, announceContextSwitch } from "@/lib/context-switch";
import { waitForWorkspaceMemory } from "@/lib/workspace-memory-client";
import type { AccountWorkContext } from "@/lib/account-work-context";

type SwitchRequest = {
  action: () => Promise<void>;
  companyId?: string;
  from?: string;
  returnTo?: string;
  onCommitted?: () => void;
  scroll?: { x: number; y: number };
  inspect?: boolean;
};
type SwitchContext = { busy: boolean; switchContext: (request: SwitchRequest) => Promise<boolean> };
const Context = createContext<SwitchContext | null>(null);

export function ContextSwitchProvider({ children }: { children: React.ReactNode }) {
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
        setGeneration(value => value + 1);
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
      for (const event of ["wheel", "touchstart", "pointerdown", "keydown"]) window.removeEventListener(event, stop);
      if (restoreScroll.current === position) restoreScroll.current = null;
    }
    function restore() {
      if (stopped) return;
      if (window.location.href !== source) { stop(); return; }
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
    for (const event of ["wheel", "touchstart", "pointerdown", "keydown"]) window.addEventListener(event, stop, { passive: true });
    restore();
    return stop;
  }, [generation]);

  async function switchContext({ action, companyId, from, returnTo, onCommitted, scroll, inspect = true }: SwitchRequest) {
    if (lock.current || !confirmContextSwitch()) return false;
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
      const source = from ?? window.location.pathname + window.location.search + window.location.hash;
      const query = new URLSearchParams({ from: source });
      if (companyId) query.set("companyId", companyId);
      const response = await fetch(`/api/v1/account/work-context?${query}`, { cache: "no-store" });
      if (response.status === 401) {
        window.location.replace(`/login?${new URLSearchParams({ reason: "expired", next: source })}`);
        return true;
      }
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Could not check the selected account's access.");
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
        setError(cause instanceof Error ? cause.message : "Access could not be checked. Try again.");
        startTransition(() => router.refresh());
        return true;
      }
      throw cause;
    } finally {
      setWorking(false);
    }
  }

  return <Context.Provider value={{ busy, switchContext }}>
    <div ref={view} key={generation} inert={busy} aria-busy={busy} style={busy ? { visibility: "hidden" } : undefined}>{children}</div>
    {busy && <div role="status" aria-live="polite" style={{ position: "fixed", inset: 0, zIndex: 10000, display: "grid", placeItems: "center", background: "var(--paper)", color: "var(--ink)" }}>Updating account and permissions…</div>}
    {!busy && error && <div role="alert" className="content">{error} <button type="button" onClick={() => startTransition(() => router.refresh())}>Check again</button></div>}
    {!busy && resolution?.requiresWorkspaceSelection && <div className="content">
      <p>Choose a workspace to continue with this account or role.</p>
      <WorkspaceMenu key={`${resolution.user.id}:${resolution.assignmentId}`} label="Choose workspace" choices={resolution.choices} current={resolution.current ?? { kind: "platform" }} canManageErpx={resolution.canManageErpx} defaultOpen onDismiss={() => setResolution(null)} />
    </div>}
  </Context.Provider>;
}

export function useContextSwitch() {
  const value = useContext(Context);
  if (!value) throw new Error("ContextSwitchProvider is required.");
  return value;
}
