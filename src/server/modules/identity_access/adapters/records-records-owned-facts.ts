import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
export function RecordsIdentityAccessOwnedFacts(control: SqlDatabase) {
    return { readSearchReferencesPeople(value: string) {
            return control.prepare(`SELECT user_id,display_name FROM principals WHERE status='active' AND lower(display_name) LIKE ? ORDER BY display_name LIMIT 8`).bind(value).all<{
                user_id: string;
                display_name: string;
            }>();
        },
        readAddPrincipalMentionPrincipal(targetId: string) {
            return control.prepare(`SELECT display_name FROM principals WHERE user_id=? AND status='active'`).bind(targetId).first<{
                display_name: string;
            }>();
        } };
}
