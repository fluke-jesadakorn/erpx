import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { type Row } from "@erpx/server-commerce/public/commerce";
export function RecordDetailsWorkflowOwnedFacts(database: SqlDatabase) { return { readCommerceMasterDetailHistory(id: string) { return database.prepare("SELECT action,actor_id,created_at,record_version FROM audit_events WHERE record_id=? ORDER BY created_at DESC LIMIT 50").bind(id).all<Row>(); } }; }
