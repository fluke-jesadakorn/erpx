import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import {principalDisplaySql} from './read-tail-family-r27';
export function RecordsIdentityAccessOwnedFacts(control: SqlDatabase) {
    return { readSearchReferencesPeople(value: string) {
            return control.prepare(`${principalDisplaySql('lower(display_name) LIKE ?')} ORDER BY display_name LIMIT 8`).bind(value).all<{
                user_id: string;
                display_name: string;
            }>();
        } };
}
