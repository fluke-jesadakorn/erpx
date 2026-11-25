import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { type Row } from '@erpx/server-commerce/public/commerce';
import { nativePosRecordSupplementSalesStatement } from './native-pos-list-reads';
export function RecordDetailsPosOwnedFacts(database: SqlDatabase) { return { readRecordSupplementDataSales(companyId: string, branchId: string | null, branchId2: string | null, source_id: string | number | null, source_id2: string | number | null, source_id3: string | number | null) { return nativePosRecordSupplementSalesStatement<Row>(database, { companyId, branchId, branchId2, sourceId: source_id, sourceId2: source_id2, sourceId3: source_id3 }); } }; }
