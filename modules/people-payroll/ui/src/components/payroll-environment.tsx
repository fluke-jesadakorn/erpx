"use client";
import type {ReactNode} from "react";
import type {PayrollPurchaseControls} from "@erpx/people-payroll-ui/lib/payroll-purchases";
import {PayrollPurchaseProvider} from "@erpx/people-payroll-ui/components/payroll-purchase-context";
import {PayrollPurchaseNotice} from "@erpx/people-payroll-ui/components/payroll-purchase-notice";
import {PayrollDraftBoundary} from "@erpx/people-payroll-ui/components/payroll-draft-boundary";
export function PayrollEnvironment({purchase, children}: {purchase?: PayrollPurchaseControls; children: ReactNode}) {
  return <PayrollPurchaseProvider controls={purchase}><PayrollDraftBoundary><PayrollPurchaseNotice/>{children}</PayrollDraftBoundary></PayrollPurchaseProvider>;
}
