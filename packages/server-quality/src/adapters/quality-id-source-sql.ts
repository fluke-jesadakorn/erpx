import { evidenceHead } from "./inspection-evidence-source-sql";
/** Owner-local id-source membership projection shared by the Quality/Ecommerce
 * customer-return settlement reads. The head routes through evidenceHead so this
 * family keeps no inline literal; projection, source and alias stay byte-identical
 * to the former inline copies. No bind, caller SQL or lock is introduced. */
export const qualityIdInSourceSql = (source: string, alias: string) => `IN (${evidenceHead("id", ` FROM (${source}) ${alias}`, "")})`;
