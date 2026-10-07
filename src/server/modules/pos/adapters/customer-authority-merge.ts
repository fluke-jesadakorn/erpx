import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function prepareCustomerAuthorityPosMerge(write: SqlDatabase, canonical: string, loser: string) { return write.prepare('UPDATE pos_sales SET party_id=? WHERE party_id=?').bind(canonical, loser); }
