import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import {type Row} from '@erpx/server-commerce/public/commerce';
export function RecordDetailsCommerceOwnedFacts(database: SqlDatabase) { return { readCommerceMasterDetailRow(id: string, companyId: string) { return database.prepare("SELECT 1").bind().first<Row>(); } }; }
