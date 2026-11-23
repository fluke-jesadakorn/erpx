export { brandIdentitySchema } from "@erpx/server-brand/domain/identity";
export type { BrandIdentity } from "@erpx/server-brand/domain/identity";
// Runtime reads delegate to the mounted native identity HTTP route; no Brand
// identity SQL remains in this package.
export { publishedBrandIdentity, brandIdentityWorkspace } from "@erpx/server-brand/adapters/identity";
