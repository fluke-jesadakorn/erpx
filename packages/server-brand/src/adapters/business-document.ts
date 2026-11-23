import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { nativeBrandRecordBranch, resolveDocumentBrand } from "@erpx/server-brand/public/branding";
import { createBusinessDocumentBrandQuery } from "@erpx/server-brand/application/business-document";

export function businessDocumentBrand(actor: WorkContext, input: Parameters<ReturnType<typeof createBusinessDocumentBrandQuery>>[0]) {
  return createBusinessDocumentBrandQuery({
    prepare: async () => {},
    recordBranch: (kind, id) => nativeBrandRecordBranch(actor, kind, id),
    document: scope => resolveDocumentBrand(actor, scope),
  })(input);
}
