import type { Kind, PmsRecord } from "@erpx/server-pms/domain/pms-model";
export type PmsStoredRecord = {
    id: string;
    kind: Kind;
    data_json: string;
    version: number;
    updated_at: number;
};
export function decodePmsRecord(row: PmsStoredRecord): PmsRecord { return { id: row.id, kind: row.kind, data: JSON.parse(row.data_json), version: Number(row.version), updatedAt: Number(row.updated_at) } as PmsRecord; }
