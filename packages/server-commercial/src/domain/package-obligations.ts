import type { ProductId } from "@erpx/contracts/product-registry";
import { packageDefinition } from "@erpx/server-commercial/domain/packages";

/** Module IDs remain owner identities, including package-included handoffs. */
const owners: Record<ProductId, readonly string[]> = {
  work: ["operations", "collaboration"], pos: ["pos", "finance", "inventory"],
  pms: ["pms", "pos", "finance", "inventory"], finance: ["finance"],
  people: ["people_payroll", "benefits", "finance"], sales: ["sales", "crm", "inventory", "finance"],
  supply: ["procurement", "inventory", "quality", "finance"], assets: ["assets", "maintenance", "finance"],
  projects: ["projects", "people_payroll", "finance"], recruit: ["talent", "people_payroll"],
  learn: ["learning", "people_payroll"], support: ["support"], service: ["field_service", "support", "inventory", "assets", "finance"],
  legal: ["legal"], manufacturing: ["manufacturing", "planning", "quality", "inventory", "finance"],
};
export function packageObligationModules(packageId: string, packageVersion: number) {
  const definition = packageDefinition(packageId, packageVersion);
  if (!definition) throw new Error("UNKNOWN_PACKAGE_VERSION");
  // Hospitality add-ons retain only their actual sale/charge/movement lineage.
  if (packageId === "pos-restaurant" && packageVersion === 1) return ["pos", "finance", "inventory"] as const;
  if (packageId === "pos-hotel-charge" && packageVersion === 1) return ["pos", "pms", "finance", "inventory"] as const;
  if (packageId === "pms-minibar-linen" && packageVersion === 1) return ["pms", "finance", "inventory"] as const;
  // Payroll/Expenses settle only their actual People and source-owned Finance duties.
  if (["people-payroll", "people-expenses"].includes(packageId) && packageVersion === 1) return ["people_payroll", "finance"] as const;
  // Evaluation hands off a native wage revision; no Benefit/Finance clearing owner is invented.
  if(packageId === "people-performance" && packageVersion === 1) return ["people_payroll"] as const;
  // Messages have atomic Collaboration history, not a synthetic zero Operations reader.
  if(packageId === "work-messages" && packageVersion === 1) return ["collaboration"] as const;
  if(packageId === "supply-quality" && packageVersion === 1) return ["inventory", "quality", "finance"] as const;
  if(packageId === "supply-advanced" && packageVersion === 1) return ["inventory", "finance"] as const;
  if(packageId === "sales-marketing" && packageVersion === 1) return ["marketing", "crm", "finance"] as const;
  if(packageId === "sales-ecommerce" && packageVersion === 1) return ["ecommerce", "sales", "inventory", "finance"] as const;
  if(packageId === "sales-portal" && packageVersion === 1) return ["customer_authority", "customer_360"] as const;
  return owners[definition.productId];
}
export type PackageObligationObservation = {
  companyId: string;
  packageId: string;
  moduleId: string;
  revision: string;
  observedAt: number;
  outstanding: number;
};
