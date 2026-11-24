import "server-only";
import { formAssetReadHead } from './form-asset-read-head';

/** Fixed active-form point read shared by the marketing source-form and
 * public form adapters. The projection and the company/version scope are
 * closed caller-chosen variants; table, id key and kind/status scope stay
 * fixed in the shared head. No raw SQL is accepted from a caller. */
export function activeFormSql(projection:'source'|'public',company=false,version=false):string{
  return formAssetReadHead({ kind:'form', projection, company, version });
}
