import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeFilesRead } from "./native-files-command";

/** PMS front attachment routing/upload facts. The three SELECTs are owned by
 * native Files read kinds; this module carries no SQL and fails closed without
 * the native transport. `setTarget` reuses the existing target read kind
 * (`RD_PROJECT_ATTACHMENT_TARGET`) because its owner SQL is byte-identical. */
export function pmsFrontAttachmentFacts(db: SqlDatabase) {
    return {
        setTarget: (id: string, companyId: string) => nativeFilesRead<{
            target_type: string;
        }>("RD_PROJECT_ATTACHMENT_TARGET", [id, companyId], db, companyId),
        linkSet: (id: string, companyId: string) => nativeFilesRead<{
            set_id: string;
        }>("RD_PMS_ATTACHMENT_LINK_SET", [id, companyId], db, companyId),
        upload: (id: string, companyId: string, userId: string) => nativeFilesRead<{
            attachment_set_id: string;
            declared_bytes: number;
            media_type: string;
        }>("RD_PMS_UPLOAD_FACT", [id, companyId, userId], db, companyId)
    };
}
