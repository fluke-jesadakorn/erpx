/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { taxIdentitySchema, taxIdentityDecisionSchema, requireCompanyTaxAuthority, taxSourceDigest, prepareTaxIdentity, decideTaxIdentity, reviewedTaxIdentity, taxIdentityGuards, currentTaxSignerGuards, taxIdentityWorkspace } from "@/server/modules/finance/adapters/finance-tax-identities";
export type { TaxIdentity } from "@/server/modules/finance/adapters/finance-tax-identities";
