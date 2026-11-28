import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { nativeBrandRecordBranch, resolveDocumentBrand } from "@erpx/brand-server/public/branding";
import { createBusinessDocumentBrandQuery } from "@erpx/brand-server/application/business-document";

export function businessDocumentBrand(actor: WorkContext, input: Parameters<ReturnType<typeof createBusinessDocumentBrandQuery>>[0]) {
  return createBusinessDocumentBrandQuery({
    prepare: async () => {},
    recordBranch: (kind, id) => nativeBrandRecordBranch(actor, kind, id),
    document: scope => resolveDocumentBrand(actor, scope),
  })(input);
}
