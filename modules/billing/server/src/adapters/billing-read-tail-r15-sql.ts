/** Byte-identical Billing read-tail SQL shared across owner-local point reads.
 * Text only: every call site still executes with its own binds and result type. */
import {billingPointReadSql} from './billing-read-tail-r29-sql';

export const BILLING_INVOICE_BY_ID_SQL = billingPointReadSql('*','billing_invoices','id=$1');
