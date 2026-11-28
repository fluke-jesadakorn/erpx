import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { legalData,legalRecordData } from "@erpx/legal-server/public/legal/service";
import { legalWorkspaceDetail as detail } from "@erpx/legal-server/public/legal/service";
import { legalCitation } from "@erpx/legal-server/public/legal/citations";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function legalPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    recordData: (...args:ReadArguments<typeof legalRecordData>)=>legalRecordData(write,...args),
    legalData: (...args: ReadArguments<typeof legalData>) => legalData(write, ...args),
    detail: (...args: ReadArguments<typeof detail>) => detail(write, ...args),
    legalCitation: (...args: ReadArguments<typeof legalCitation>) => legalCitation(write, ...args),
  };
}
