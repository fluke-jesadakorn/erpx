import type { QuickAccessItem } from "@erpx/feature-work_projection/lib/quick-access";

type CustomerDocument = {
  binding_id: string;
  number: string;
  kind: string;
  state: string;
  due_date: string;
};

type LegalShare = Record<string, unknown>;

/** These IDs are scoped to the current portal grant, never to employee Work. */
export function customerPortalQuickAccess(
  holdingSlug: string,
  documents: CustomerDocument[],
): QuickAccessItem[] {
  return documents.map((document) => ({
    id: `order:${document.binding_id}`,
    title: `${document.kind === "quote" ? "Quote" : "Order"} ${document.number}`,
    description: `${document.state.replaceAll("_", " ")} · Due ${document.due_date}`,
    href: `/customer-portal/${encodeURIComponent(holdingSlug)}?order=${encodeURIComponent(document.binding_id)}#customer-order-${encodeURIComponent(document.binding_id)}`,
    icon: "orders",
    tone: "blue",
  }));
}

export function legalPortalQuickAccess(
  holdingSlug: string,
  shares: LegalShare[],
): QuickAccessItem[] {
  return shares.map((share) => ({
    id: `share:${String(share.id)}`,
    title: String(share.name || "Shared document"),
    description: `Version ${String(share.version)}`,
    href: `/legal-portal/${encodeURIComponent(holdingSlug)}?share=${encodeURIComponent(String(share.id))}`,
    icon: "documents",
    tone: "brand",
  }));
}
