"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";

import { QuickAccessShelf } from "@erpx/feature-work_projection/components/quick-access/shelf";
import { defaultWorkCardIds, newWorkHref, workCardIds, type WorkActionCard, type WorkCardId } from "@erpx/feature-work_projection/lib/work-cards";
import { workCardTone } from "@erpx/feature-work_projection/lib/work-category-tone";
import type { QuickAccessItem } from "@erpx/feature-work_projection/lib/quick-access";
import { selfServiceActions } from "@erpx/feature-work_projection/lib/self-service";
import { usePathname } from "next/navigation";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { useCallback, useEffect, useRef, useState } from "react";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { scheduleWorkRefresh } from "@erpx/feature-work_projection/lib/work-refresh";
import type { WorkSummary } from "@erpx/server-work_projection/public/work-summary";

function useShortcutSummary(base: string) {
  const [summary, setSummary] = useState<WorkSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const request = useRef<AbortController | null>(null);
  const refresh = useCallback(async () => {
    request.current?.abort();
    const controller = new AbortController();
    request.current = controller;
    try {
      const response = await workFetch(`/api/v1${base}/work/summary`, { cache: "no-store", signal: controller.signal });
      if (!response.ok) {
        if ([401, 403, 404, 409].includes(response.status)) setSummary(null);
        throw Error("Your shortcuts could not refresh.");
      }
      const { data } = await response.json() as { data: WorkSummary };
      if (!controller.signal.aborted) { setSummary(data); setError(""); }
    } catch (error) {
      if (!controller.signal.aborted) setError(error instanceof Error ? error.message : "Your shortcuts could not refresh.");
    } finally {
      if (request.current === controller) { request.current = null; setLoading(false); }
    }
  }, [base]);
  useEffect(() => {
    const timer = setTimeout(() => void refresh(), 0);
    const stop = scheduleWorkRefresh(() => void refresh());
    const clear = () => {
      request.current?.abort(); request.current = null;
      setSummary(null); setLoading(false); setError("Your work role changed. Reload to review shortcuts.");
    };
    window.addEventListener("erpx:context-switch-started", clear);
    window.addEventListener("erpx:work-context-changed", clear);
    return () => {
      clearTimeout(timer); stop(); request.current?.abort();
      window.removeEventListener("erpx:context-switch-started", clear);
      window.removeEventListener("erpx:work-context-changed", clear);
    };
  }, [refresh]);
  return { quickAccess: summary, quickAccessLoading: loading, quickAccessError: error, refresh,
    saveQuickAccess: (preferences: WorkSummary["preferences"]) => setSummary(previous =>
      previous && preferences.version >= previous.preferences.version ? { ...previous, preferences } : previous) };
}

function cardMeta(card: WorkActionCard) {
  if (card.id === "shifts") return undefined;
  if (card.incomplete) return "Summary incomplete";
  const counts = [
    card.toDo ? `${card.toDo} to do` : null,
    card.waiting ? `${card.waiting} waiting` : null,
  ].filter(Boolean);
  if (card.blocked) counts.push(`${card.blocked} need attention`);
  return counts.length ? counts.join(" · ") : "No pending work";
}

/** Shared versioned pins over the current authorized action catalogue. */
export function WorkShortcuts({ base, role, placement = "sidebar", onNavigate }: {
  base: string; role?: string; placement?: "sidebar" | "panel"; onNavigate?: () => void;
}) {
  const { quickAccess, quickAccessLoading, quickAccessError, refresh, saveQuickAccess } = useShortcutSummary(base);
  const pathname = usePathname();
  const search = useSearchParams();
  const current = pathname === canonicalUiHref(`${base}/work`) ? search.toString() : "";
  const cards = quickAccess?.cards ?? [];
  const items: QuickAccessItem[] = cards.map((card) => ({
    id: card.id,
    title: card.title,
    description: card.objective ?? selfServiceActions.find((action) => action.id === (card.id === "purchases" ? "purchase_request" : card.id))?.description,
    href: card.href,
    icon: card.icon,
    tone: workCardTone[card.id] ?? "brand",
    meta: card.destination ? undefined : cardMeta(card),
    ...(card.create ? {
      secondary: {
        label: card.create.kind === "conversation" ? card.create.label : `New ${card.create.label.toLowerCase()}`,
        href: newWorkHref(base, card.create, current),
      },
    } : {}),
  }));
  return <QuickAccessShelf appearance="standard"
    allowCustomization={false}
    items={items}
    initialPreferences={quickAccess?.preferences ?? { pinnedIds: null, version: 0 }}
    defaultIds={defaultWorkCardIds(cards, role)}
    endpoint={`/api/v1${base}/work/card-preferences`}
    title="My shortcuts"
    placement={placement}
    embedded
    requestMode="work"
    loading={!quickAccess || quickAccessLoading}
    error={quickAccessError || (quickAccess?.preferencesUnavailable ? "Your saved shortcuts could not refresh." : "")}
    onRetry={() => void refresh()}
    onSaved={preference => {
      const pinnedIds = preference.pinnedIds?.filter((id): id is WorkCardId => workCardIds.some(known => known === id)) ?? null;
      if (preference.pinnedIds === null || pinnedIds?.length === preference.pinnedIds.length)
        saveQuickAccess({ ...preference, pinnedIds });
      else void refresh();
    }}
    onNavigate={onNavigate}
  />;
}
