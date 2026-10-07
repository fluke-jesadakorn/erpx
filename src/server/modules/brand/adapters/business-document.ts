import "server-only";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { holdingResources } from "../../runtime/public/runtime";
import { recordBranchId, resolveDocumentBrand } from "../public/branding";
import { createBusinessDocumentBrandQuery } from "../application/business-document";

export function businessDocumentBrand(actor: WorkContext, input: Parameters<ReturnType<typeof createBusinessDocumentBrandQuery>>[0]) {
  let db: SqlDatabase;
  return createBusinessDocumentBrandQuery({
    prepare: async () => { db = (await holdingResources(actor.holdingId)).write; },
    recordBranch: (kind, id) => recordBranchId(db, actor, kind, id),
    document: scope => resolveDocumentBrand(db, actor, scope),
  })(input);
}
