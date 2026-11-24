/** Marketing form/content asset point-read head (r38).
 * The three owner-local form/content asset reads share one byte-identical
 * renderer: a caller picks a closed variant and this helper owns the table,
 * projection and predicate. Every emitted statement is byte-identical to the
 * original reader, so binds, ordering and locks are unchanged. FOR SHARE
 * locks, settlement command_receipts and native read ports stay untouched. */
export type FormAssetRead =
  | { kind: 'content'; projection: string }
  | { kind: 'form'; projection: 'source' | 'public'; company?: boolean; version?: boolean }
  | { kind: 'form-row' };

export function formAssetReadHead(read: FormAssetRead): string {
  let projection: string;
  let predicate: string;
  if (read.kind === 'content') {
    projection = read.projection;
    predicate = "id=? AND company_id=? AND kind='content' AND status='active' AND version=?";
  } else if (read.kind === 'form') {
    projection = read.projection === 'source'
      ? 'configuration_json'
      : 'id,company_id,name,version,preview_json,configuration_json';
    predicate = `id=?${read.company ? ' AND company_id=?' : ''}${read.version ? ' AND version=?' : ''} AND kind='form' AND status='active'`;
  } else {
    projection = 'id,company_id,version,status,configuration_json';
    predicate = "id=? AND company_id=? AND kind='form'";
  }
  return `SELECT ${projection} FROM marketing_assets WHERE ${predicate}`;
}
