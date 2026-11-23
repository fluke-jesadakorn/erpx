import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeFilesRead } from "./native-files-command";

/**
 * Legal law-corpus clean-source link fact. The read is owned by the native
 * Files read kind `RD_LEGAL_CORPUS_SOURCE_LINK`; this module emits no SQL and
 * fails closed without the native transport. The three binds stay static and
 * the caller's single-row shape is preserved, including a null link id.
 */
export function legalCorpusSourceFacts(db: SqlDatabase) {
    return {
        cleanSourceLink: (facts: {
            attachment_link_id: string | null | undefined;
            companyId: string;
            checksum: string;
        }) => nativeFilesRead<Record<string, unknown>>(
            'RD_LEGAL_CORPUS_SOURCE_LINK',
            [facts.attachment_link_id ?? null, facts.companyId, facts.checksum],
            db,
            facts.companyId,
        ),
    };
}
