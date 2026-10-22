import inventory from "@erpx/contracts/ownership.json";

export const moduleIds = ["brand", "commercial", "identity_access", "organization", "platform_administration", "customer_360", "customer_authority", "support", "finance", "pos", "sales", "assets", "inventory", "maintenance", "operations", "procurement", "quality", "legal", "people_payroll", "crm", "marketing", "manufacturing", "planning", "projects", "ecommerce", "benefits", "learning", "talent", "field_service", "agent", "collaboration", "pms", "manage_erpx"] as const;
export type ModuleId = (typeof moduleIds)[number];
export type CapabilityMigration={id:string;migration:"legacy"|"partial"|"clean";source:string[];regression:string[]};
export type ServiceOwner = {
  id: string;
  modules: ModuleId[];
  source: string[];
  migration: "legacy" | "partial" | "clean";
  kind: "business" | "kernel" | "projection" | "infrastructure";
  capabilities?:CapabilityMigration[];
};

export const serviceOwnership = inventory as {
  version: number;
  modules: ModuleId[];
  owners: ServiceOwner[];
};

/** Only * and ** are supported; an unmatched source must select full regression. */
export function ownershipPatternMatches(pattern: string, source: string): boolean {
  const normalized = source.replaceAll("\\", "/").replace(/^\.\//, "");
  const expression = pattern
    .split("**")
    .map((part) => part.split("*").map((piece) => piece.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join("[^/]*"))
    .join(".*");
  return new RegExp(`^${expression}$`).test(normalized);
}

export function ownersForSource(source: string): ServiceOwner[] {
  return serviceOwnership.owners.filter((owner) =>
    owner.source.some((pattern) => ownershipPatternMatches(pattern, source)),
  );
}

export function modulesForSource(source: string): ModuleId[] {
  return [...new Set(ownersForSource(source).flatMap((owner) => owner.modules))];
}
