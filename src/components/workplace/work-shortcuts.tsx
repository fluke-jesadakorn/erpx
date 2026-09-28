"use client";

import { QuickAccessShelf } from "@/components/quick-access/shelf";
import { defaultWorkCardIds, newWorkHref, workCardIds, type WorkActionCard, type WorkCardId } from "@/lib/work-cards";
import { workCardTone } from "@/lib/work-category-tone";
import type { QuickAccessItem } from "@/lib/quick-access";
import { selfServiceActions } from "@/lib/self-service";
import { usePathname, useSearchParams } from "next/navigation";
import { useNavigationCounts } from "./navigation-counts";

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

/** The one holding-wide shelf, backed by the existing company-wide Work pins. */
export function WorkShortcuts({ base }: { base: string }) {
  const { quickAccess, quickAccessError, refresh, saveQuickAccess } = useNavigationCounts();
  const pathname = usePathname();
  const search = useSearchParams();
  const current = pathname === `${base}/work` ? search.toString() : "";
  const cards = quickAccess?.cards ?? [];
  const items: QuickAccessItem[] = cards.map((card) => ({
    id: card.id,
    title: card.title,
    description: selfServiceActions.find((action) => action.id === (card.id === "purchases" ? "purchase_request" : card.id))?.description,
    href: card.href,
    icon: card.icon,
    tone: workCardTone[card.id],
    meta: cardMeta(card),
    ...(card.create ? {
      secondary: {
        label: card.create.kind === "conversation" ? card.create.label : `New ${card.create.label.toLowerCase()}`,
        href: newWorkHref(base, card.create, current),
      },
    } : {}),
  }));
  return <QuickAccessShelf
    items={items}
    initialPreferences={quickAccess?.preferences ?? { pinnedIds: null, version: 0 }}
    defaultIds={defaultWorkCardIds(cards)}
    endpoint={`/api/v1${base}/work/card-preferences`}
    title="Your shortcuts"
    placement="rail"
    embedded
    requestMode="work"
    loading={!quickAccess && !quickAccessError}
    error={quickAccessError || (quickAccess?.preferencesUnavailable ? "Your saved shortcuts are unavailable. Showing the default arrangement." : "")}
    onRetry={() => void refresh()}
    onSaved={(preference) => {
      if (preference.pinnedIds?.some((id) => !workCardIds.includes(id as WorkCardId))) {
        void refresh();
        return;
      }
      saveQuickAccess({ pinnedIds: preference.pinnedIds as WorkCardId[] | null, version: preference.version });
    }}
  />;
}
