import "server-only";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { pmsDocumentQueries } from "@/server/modules/pms/public/entrypoint-queries";
import { loadPms } from "@/server/modules/pms/public/pms";
import { resolveDocumentBrand } from "@/server/modules/brand/public/branding";
import { pmsImportBatches } from "@/server/modules/pms/public/pms/import";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function pmsPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    pmsDocumentQueries: (...args: ReadArguments<typeof pmsDocumentQueries>) => pmsDocumentQueries(write, ...args),
    loadPms: (...args: ReadArguments<typeof loadPms>) => loadPms(write, ...args),
    resolveDocumentBrand: (...args: ReadArguments<typeof resolveDocumentBrand>) => resolveDocumentBrand(write, ...args),
    pmsImportBatches: (...args: ReadArguments<typeof pmsImportBatches>) => pmsImportBatches(write, ...args),
  };
}
