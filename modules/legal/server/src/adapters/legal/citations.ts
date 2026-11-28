import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { access } from "@erpx/legal-server/adapters/legal/access";
import { documentAccess } from "@erpx/legal-server/public/legal/documents";
import { lawCitation, legalEvidenceDocumentAvailableSql } from "@erpx/legal-server/adapters/legal/knowledge";
import { legalCorpusSourceCleanHead } from '../legal-read-tail-r48-sources';
import { createLegalCitationQuery } from "@erpx/legal-server/application/citations";
import { nativeLegalCitationChunk } from '../native-lane-26/client';
import { legalAccessEvent } from '@erpx/collaboration-server/adapters/native-legal-access-event';
export function legalCitation(db: SqlDatabase, actor: WorkContext, sourceId: string, asOf?: unknown) {
  return createLegalCitationQuery({ law: (id, asOf) => lawCitation(db, actor, id, asOf), document: id => documentAccess(db, actor, id), item: id => access(db, actor, id), clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, error: (status, code, message) => new ApiError(status, code, message),
    chunk: id => nativeLegalCitationChunk(actor, id),
    available: async id => Boolean(await db.prepare(legalCorpusSourceCleanHead(legalEvidenceDocumentAvailableSql)).bind(id).first()),
    recordRead: async (itemId, documentId, identifier, now) => { await (await legalAccessEvent(db, actor.companyId, actor.userId).record(identifier, "document.citation.read", itemId, documentId, now)).run(); },
  })(sourceId, asOf);
}
