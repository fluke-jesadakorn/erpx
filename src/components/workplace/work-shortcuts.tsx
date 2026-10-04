"use client";

import { QuickAccessShelf } from "@/components/quick-access/shelf";
import { defaultWorkCardIds, newWorkHref, type WorkActionCard } from "@/lib/work-cards";
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

/** Device-local pins; the authorized catalogue is available with the shell. */
export function WorkShortcuts({ base, role }: { base: string; role?: string }) {
  const { quickAccess, shortcutStorageKey } = useNavigationCounts();
  const pathname = usePathname();
  const search = useSearchParams();
  const current = pathname === `${base}/work` ? search.toString() : "";
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
    items={items}
    initialPreferences={quickAccess?.preferences ?? { pinnedIds: null, version: 0 }}
    defaultIds={defaultWorkCardIds(cards, role)}
    endpoint={`/api/v1${base}/work/card-preferences`}
    localStorageKey={shortcutStorageKey}
    title="My shortcuts"
    placement="sidebar"
    embedded
    requestMode="work"
  />;
}
