/** G4 / retire.crm.read-tail-r25 byte-identical read family.
 *
 * The renderer below reproduces the pre-dedupe purchase-row projection prefix
 * exactly; each call site keeps its own witness and join tail verbatim.
 * All transaction lock and row-lock statements stay inline in the calling
 * adapters. r8-r24 families are untouched.
 */

/** One parameterized renderer for the fixed id/company/branch/department
 * projection prefix shared by the purchase source row reads. Callers pass only
 * the row alias and the branch/department expressions; rendered text stays
 * byte-identical to the retired inline literals. */
export const crmPurchaseRowProjection = (
  alias: string,
  branch: string,
  department: string,
) => `SELECT ${alias}.id,${alias}.company_id,${branch},${department},`;
