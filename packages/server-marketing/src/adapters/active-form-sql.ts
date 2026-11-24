import "server-only";

/** Fixed active-form point read shared by the marketing source-form and
 * public form adapters. The projection and the company/version scope are
 * closed caller-chosen variants; table, id key and kind/status scope stay
 * fixed here. No raw SQL is accepted from a caller. */
export function activeFormSql(projection:'source'|'public',company=false,version=false):string{
  const columns=projection==='source'?'configuration_json':'id,company_id,name,version,preview_json,configuration_json';
  return `SELECT ${columns} FROM marketing_assets WHERE id=?${company?' AND company_id=?':''}${version?' AND version=?':''} AND kind='form' AND status='active'`;
}
