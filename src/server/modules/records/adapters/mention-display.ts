import "server-only";
import type { WorkContext } from "../../identity_access/public/authorization";
import { getRecord } from "./records";

/** The original record owner authorizes first; discussion UI needs no context,
 * AI provenance, principal mentions or command-history snapshots. */
export async function recordMentionDisplay(actor: WorkContext, moduleId: string, id: string) {
  const record = await getRecord(actor, moduleId, id);
  return { title: record.title, description: record.description, status: record.status, version: record.version,
    fields: record.fields.map(({ key, label, value }) => ({ key, label, value })) };
}
