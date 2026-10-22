import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { recordBranchId, resolveDocumentBrand } from "@erpx/server-brand/public/branding";
import { createBusinessDocumentBrandQuery } from "@erpx/server-brand/application/business-document";

export function businessDocumentBrand(actor: WorkContext, input: Parameters<ReturnType<typeof createBusinessDocumentBrandQuery>>[0]) {
  let db: SqlDatabase;
  return createBusinessDocumentBrandQuery({
    prepare: async () => { db = (await holdingResources(actor.holdingId)).write; },
    recordBranch: (kind, id) => recordBranchId(db, actor, kind, id),
    document: scope => resolveDocumentBrand(db, actor, scope),
  })(input);
}
