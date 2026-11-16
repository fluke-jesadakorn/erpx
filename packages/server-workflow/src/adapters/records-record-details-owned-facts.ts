import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import {type Row} from '@erpx/server-commerce/public/commerce';
export function RecordDetailsWorkflowOwnedFacts(database: SqlDatabase) { return { readCommerceMasterDetailHistory(id: string) { return database.prepare("SELECT 1").bind().all<Row>(); } }; }
