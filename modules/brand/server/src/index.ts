export { brandIdentitySchema } from "@erpx/brand-server/domain/identity";
export type { BrandIdentity } from "@erpx/brand-server/domain/identity";
// Runtime reads delegate to the mounted native identity HTTP route; no Brand
// identity SQL remains in this package.
export { publishedBrandIdentity, brandIdentityWorkspace } from "@erpx/brand-server/adapters/identity";
