import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { readableRecordSourceIds as posSources } from "@erpx/server-pos/public/records-source-facts";
import { readableRecordSourceIds as assetSources } from "@erpx/server-assets/public/records-source-facts";
import { readablePmsSourceIds } from "@erpx/server-pms/public/pms/source";

/** Aggregate readers must apply the same source authority as ticket detail. */
export async function supportTicketSourceReadPredicate(actor: WorkContext, alias = "t") {
  const source = `${alias}.source_record_json::jsonb`, definitions = [
    ["pos_sale", await posSources(actor)], ["asset", await assetSources(actor)], ["pms_record", await readablePmsSourceIds(actor)],
  ] as const;
  return { sql: `(${alias}.source_record_json IS NULL OR ${definitions.map(([kind, projection]) =>
    `(${source}->>'kind'='${kind}' AND ${source}->>'id' IN (${projection.sql}))`).join(" OR ")})`,
    bindings: definitions.flatMap(([, projection]) => projection.bindings) };
}
