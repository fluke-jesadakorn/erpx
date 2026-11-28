/**
 * Test/harness-only oracle seeding for supplier bills.
 *
 * These three writers were removed from the production contract
 * (`public/supplier-bill-matching`) because the HTTP routes are native-only.
 * They survive here so integration suites and the browser simulation can keep
 * building fixtures; no production caller may import this subpath.
 *
 * Removal condition: migrate these seeding call sites onto native seeding, then
 * delete this file together with the adapter implementations.
 */
export {
  captureSupplierBill,
  decideSupplierBill,
  saveSupplierInvoicePolicy,
} from '../adapters/supplier-bills';
