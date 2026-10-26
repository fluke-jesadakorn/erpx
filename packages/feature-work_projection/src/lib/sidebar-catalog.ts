import type { ProductId } from "@erpx/contracts/product-registry";
import type { SidebarTone } from "@erpx/ui/components/sidebar-navigation";
import type { NavigationCategory, NavigationResolution, WorkplaceNavigationModel } from "./workplace-navigation";

/** Display categories only; caller-supplied eligible entries remain authoritative. */
export const appsSidebarCategories = [
  { id: "people-organization", en: "People & organization", th: "บุคลากรและองค์กร", tone: "violet", products: ["people", "recruit"] },
  { id: "customers-sales", en: "Customers & sales", th: "ลูกค้าและการขาย", tone: "blue", products: ["sales", "support"] },
  { id: "supply", en: "Purchasing & inventory", th: "จัดซื้อและคลัง", tone: "amber", products: ["supply"] },
  { id: "projects-routines", en: "Projects & routines", th: "โครงการและงานประจำ", tone: "brand", products: ["projects"] },
  { id: "services", en: "Services", th: "บริการ", tone: "teal", products: ["assets", "service", "legal"] },
  { id: "finance", en: "Finance", th: "การเงิน", tone: "green", products: ["finance"] },
  { id: "pos", en: "Point of sale", th: "ขายหน้าร้าน", tone: "amber", products: ["pos"] },
  { id: "pms", en: "Hotel", th: "โรงแรม", tone: "blue", products: ["pms"] },
  { id: "production", en: "Production", th: "การผลิต", tone: "amber", products: ["manufacturing"] },
  { id: "knowledge", en: "Knowledge", th: "ความรู้", tone: "violet", products: ["learn"] },
] as const satisfies readonly { id: string; en: string; th: string; tone: SidebarTone; products: readonly ProductId[] }[];

export type AppsSidebarItem = { id: string; label: string; href: string; productId?: ProductId; groupId?: "administration" };

export function groupAppsSidebar(items: readonly AppsSidebarItem[], language: "th" | "en") {
  const unique = items.filter((item, index) => items.findIndex(other => other.id === item.id) === index);
  const groups: { id: string; label: string; tone: SidebarTone; items: AppsSidebarItem[] }[] = appsSidebarCategories.flatMap(category => {
    const entries = category.products.flatMap(id => unique.filter(item => item.productId === id));
    return entries.length ? [{ id: category.id as string, label: category[language], tone: category.tone as SidebarTone, items: entries }] : [];
  });
  const administration = unique.filter(item => item.groupId === "administration");
  if (administration.length) groups.push({ id: "administration", label: language === "th" ? "การดูแลระบบ" : "Administration", tone: "brand", items: administration });
  return { primary: unique.filter(item => item.id === "applications" || item.productId === "apps"), groups, account: unique.find(item => item.id === "account") };
}

export function sidebarCategoryTone(id?: string): SidebarTone {
  if (id === "purchasing" || id === "inventory-quality") return "amber";
  if (id === "insights") return "green";
  return appsSidebarCategories.find(category => category.id === id)?.tone ?? "brand";
}

/** Owner routes can resolve to a deeper destination than the visible sidebar
 * root. Select the eligible visible parent without rewriting its destination. */
export function sidebarCategoryItems(_model: WorkplaceNavigationModel, category: NavigationCategory) {
  return [...category.apps, ...category.secondaryItems ?? []];
}

export function sidebarSelection(model: WorkplaceNavigationModel, active: NavigationResolution, _pathname: string) {
  if (model.primary.some(item => item.id === active.activeId)) return { itemId: null, categoryId: null };
  const entries = model.categories.flatMap(category => sidebarCategoryItems(model,category).map(item=>({item,category})));
  const selected = entries.find(entry=>entry.item.id===active.sidebarId) ?? entries.find(entry=>entry.item.id===active.activeId);
  return {itemId:selected?.item.id ?? null, categoryId:selected?.category.id ?? null};
}

export const hospitalitySidebarCategories = {
  pos: [
    { id: "daily", en: "Daily work", th: "งานประจำวัน", tone: "amber", items: ["sale", "kitchen", "shifts", "history"] },
    { id: "management", en: "Management", th: "การจัดการ", tone: "teal", items: ["catalog", "customers", "reports", "settings", "closing"] },
  ],
  pms: [
    { id: "stays", en: "Stays", th: "การเข้าพัก", tone: "blue", items: ["dashboard", "reservations", "front-desk", "guests"] },
    { id: "rooms", en: "Rooms", th: "ห้องพัก", tone: "teal", items: ["rooms", "housekeeping", "inventory"] },
    { id: "finance", en: "Finance", th: "การเงิน", tone: "green", items: ["guest-accounts", "cashier", "night-audit"] },
    { id: "management", en: "Management", th: "การจัดการ", tone: "violet", items: ["reports", "settings"] },
  ],
} as const;

export function groupHospitalitySidebar<T extends { id: string }>(app: "pos" | "pms", items: readonly T[], language: "th" | "en") {
  return hospitalitySidebarCategories[app].flatMap(category => {
    const entries = items.filter(item => (category.items as readonly string[]).includes(item.id));
    return entries.length ? [{ id: category.id, label: category[language], tone: category.tone, items: entries }] : [];
  });
}
