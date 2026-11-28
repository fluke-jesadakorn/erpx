import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeFilesRead } from "./native-files-command";

type LogoPublicationRow = { object_key: string; media_type: string };

/**
 * Front app storefront logo-publication fact. The read is owned by the native
 * Files read kind `RD_FRONT_LOGO_PUBLICATION`; this module emits no SQL and
 * fails closed without the native transport. The caller's single-row shape is
 * preserved and all three binds stay static in the owner statement.
 */
export function frontLogoFacts(db: SqlDatabase) {
    return {
        async logoPublication(companyId: string, companyId2: string, logoPublicationId: string) {
            return nativeFilesRead<LogoPublicationRow>(
                'RD_FRONT_LOGO_PUBLICATION',
                [companyId, companyId2, logoPublicationId],
                db,
                companyId,
            );
        },
    };
}
