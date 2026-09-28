import "server-only";

import { frontVisiblePath } from "@/lib/front-host";
import type { QuickAccessItem } from "@/lib/quick-access";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "@/server/authorization";
import { frontCompanyApp, type FrontAppKind } from "@/server/front-app";
import { holdingBindingBySlug, holdingResources } from "@/server/runtime";
import { workspaceChoices } from "@/server/workspace-choices";

type FrontAction = {
  id: string;
  title: string;
  permission: string[];
  tone: QuickAccessItem["tone"];
  icon: string;
};

const posActions: FrontAction[] = [
  { id: "sale", title: "Sale", permission: ["pos.sell"], tone: "blue", icon: "sale" },
  { id: "kitchen", title: "Kitchen", permission: ["pos.kitchen"], tone: "teal", icon: "kitchen" },
  { id: "shifts", title: "Cash shifts", permission: ["pos.sell"], tone: "amber", icon: "cash-shifts" },
  { id: "history", title: "History", permission: ["pos.read"], tone: "blue", icon: "history" },
];

const pmsActions: FrontAction[] = [
  { id: "dashboard", title: "Overview", permission: ["pms.read"], tone: "brand", icon: "pms" },
  { id: "reservations", title: "Reservations", permission: ["pms.reservations"], tone: "blue", icon: "reservations" },
  { id: "front-desk", title: "Front desk", permission: ["pms.frontdesk"], tone: "teal", icon: "front-desk" },
  { id: "rooms", title: "Rooms", permission: ["pms.housekeeping", "pms.maintenance"], tone: "green", icon: "rooms" },
  { id: "housekeeping", title: "Housekeeping", permission: ["pms.housekeeping", "pms.inspect"], tone: "green", icon: "housekeeping" },
  { id: "folios", title: "Payments", permission: ["pms.cashier"], tone: "amber", icon: "folios" },
  { id: "cashier", title: "Cashier", permission: ["pms.cashier"], tone: "amber", icon: "cashier" },
  { id: "night-audit", title: "Night audit", permission: ["pms.audit"], tone: "violet", icon: "night-audit" },
];

/** A Front shelf can only point at daily views in its current app. */
export function frontActionItems(app: FrontAppKind, actor: WorkContext, frontHref: string): QuickAccessItem[] {
  return (app === "pos" ? posActions : pmsActions)
    .filter((action) => action.permission.some((permission) => hasPermission(actor, permission)))
    .map((action) => ({
      id: action.id,
      title: action.title,
      icon: action.icon,
      tone: action.tone,
      meta: app.toUpperCase(),
      href: app === "pos"
        ? frontHref + "?tab=" + action.id
        : action.id === "dashboard" ? frontHref : frontHref + action.id,
    }));
}

export type FrontCompanyChoice = {
  id: string;
  label: string;
  holding: string;
  slug: string;
};

/** Same authorization used by the Front company picker, not the ERPX menu. */
export async function frontCompanyChoices(app: FrontAppKind): Promise<FrontCompanyChoice[]> {
  const choices = (await workspaceChoices()).filter((choice) => choice.kind === "holding");
  const candidates = (await Promise.all(choices.map(async (choice) => {
    const binding = await holdingBindingBySlug(choice.holdingSlug);
    const { write } = await holdingResources(binding.id);
    const ids = choice.companies.map((company) => company.id);
    if (!ids.length) return [];
    const rows = (await write.prepare(
      "SELECT id,slug FROM organization_nodes WHERE kind='company' AND status='active' AND id IN (" + ids.map(() => "?").join(",") + ")",
    ).bind(...ids).all<{ id: string; slug: string }>()).results;
    return choice.companies.flatMap((company) => {
      const row = rows.find((candidate) => candidate.id === company.id);
      return row ? [{
        id: "company:" + choice.holdingSlug + ":" + row.slug,
        label: company.label,
        holding: choice.holdingSlug,
        slug: row.slug,
      }] : [];
    });
  }))).flat();
  const companies = await Promise.all(candidates.map(async (company) => {
    const entry = await frontCompanyApp(company.holding, company.slug, app).catch(() => null);
    return entry?.locations.length ? company : null;
  }));
  return companies.filter((company): company is FrontCompanyChoice => !!company);
}

export function frontCompanyItems(companies: FrontCompanyChoice[]): QuickAccessItem[] {
  return companies.map((company) => ({
    id: company.id,
    title: company.label,
    href: frontVisiblePath(company.holding, company.slug),
    tone: "brand",
    icon: "company",
  }));
}
