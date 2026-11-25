/** Byte-identical PMS read-tail r49 settlement source heads shared across
 * owner-local source templates. Text only: every consumer keeps its own
 * FROM/JOIN/WHERE tail, binds, locks and result type; nothing is cached or
 * executed here. Variants come from a closed union owned by this module,
 * never caller SQL. Locking reads, the native registry and the r17-r48
 * fragments stay untouched. */

type SourceKey = 'charge' | 'reverse' | 'settlementReceipt' | 'refund';
const SOURCE_KEY: Record<SourceKey, string> = {
  charge: "':charge' source_key",
  reverse: "':reverse'",
  settlementReceipt: "CASE WHEN r.data->>'description'='Receive OTA / company remittance' THEN ':settlement' ELSE ':receipt' END",
  refund: "':refund'",
};
export const pmsSettlementSourceKeySql = (kind: SourceKey): string =>
  `SELECT e.package_id,e.company_id,e.property_id,r.id||${SOURCE_KEY[kind]}`;
