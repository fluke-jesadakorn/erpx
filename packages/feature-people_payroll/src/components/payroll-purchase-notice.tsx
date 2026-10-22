"use client";
import {usePayrollPurchase} from "@erpx/feature-people_payroll/components/payroll-purchase-context";

/** The visual notice belongs to the ordinary Payroll surface. The purchase
 * provider remains presentation-free for specialized shared consumers. */
export function PayrollPurchaseNotice() {
  const controls = usePayrollPurchase();
  return controls && !controls.newWork ? <p className="notice" role="status">{controls.settleExisting
    ? "Payroll preparation is paused for this package. Existing payroll obligations and history remain available."
    : "Payroll is available for retained history."}</p> : null;
}
