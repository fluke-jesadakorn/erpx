import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { loadPms } from "../public/pms";
import { resolveDocumentBrand } from "@erpx/brand-server/public/branding";
import { pmsImportBatches } from "@erpx/pms-server/public/pms/import";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function pmsPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    loadPms: (...args: ReadArguments<typeof loadPms>) => loadPms(write, ...args),
    resolveDocumentBrand: (...args: Parameters<typeof resolveDocumentBrand>) => resolveDocumentBrand(...args),
    pmsImportBatches: (...args: ReadArguments<typeof pmsImportBatches>) => pmsImportBatches(write, ...args),
  };
}
