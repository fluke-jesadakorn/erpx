import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { type Row } from "@erpx/server-commerce/public/commerce";
export function RecordDetailsCommerceOwnedFacts(database: SqlDatabase) { return { readCommerceMasterDetailRow(id: string, companyId: string) { return database.prepare("SELECT * FROM commerce_products WHERE id=? AND company_id=?").bind(id, companyId).first<Row>(); } }; }
