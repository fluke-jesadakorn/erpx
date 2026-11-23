import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeFilesRead } from "./native-files-command";

type ImageSourceRow = {
    object_key: string;
    preview_object_key: string;
    preview_checksum_sha256: string;
};

/**
 * POS product-image source fact. The read is owned by the native Files read
 * kind `RD_POS_IMAGE_SOURCE`; this module emits no SQL and fails closed without
 * the native transport. The caller's single-row shape is preserved.
 */
export function posImageSourceFacts(db: SqlDatabase) {
    return {
        readAttachments1: (facts: {
            sourceId: string;
            companyId: string;
        }) => nativeFilesRead<ImageSourceRow>(
            'RD_POS_IMAGE_SOURCE',
            [facts.sourceId, facts.companyId],
            db,
            facts.companyId,
        ),
    };
}
