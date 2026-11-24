import "server-only";
/** Canonical byte-identical SELECT-only owner read shared across Commercial
 * adapters (G4 read-tail r1). The constant owns one literal; every consumer
 * imports it and keeps no local copy. Each call site still prepares and executes
 * its own independent statement, and no export changes. Pure read: no INSERT,
 * UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. */
export const commercialChangeSetReceiptSql = "SELECT 1 FROM company_package_change_sets WHERE id=?";
