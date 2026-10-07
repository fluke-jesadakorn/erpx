import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
import { type Row } from "@/server/modules/commerce/public/commerce";
export function RecordDetailsCommerceOwnedFacts(database: SqlDatabase) { return { readCommerceMasterDetailRow(id: string, companyId: string) { return database.prepare("SELECT * FROM commerce_products WHERE id=? AND company_id=?").bind(id, companyId).first<Row>(); } }; }
