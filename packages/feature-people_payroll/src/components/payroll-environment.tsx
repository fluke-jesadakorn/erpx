"use client";
import type {ReactNode} from "react";
import type {PayrollPurchaseControls} from "@erpx/feature-people_payroll/lib/payroll-purchases";
import {PayrollPurchaseProvider} from "@erpx/feature-people_payroll/components/payroll-purchase-context";
import {PayrollPurchaseNotice} from "@erpx/feature-people_payroll/components/payroll-purchase-notice";
import {PayrollDraftBoundary} from "@erpx/feature-people_payroll/components/payroll-draft-boundary";
export function PayrollEnvironment({purchase, children}: {purchase?: PayrollPurchaseControls; children: ReactNode}) {
  return <PayrollPurchaseProvider controls={purchase}><PayrollDraftBoundary><PayrollPurchaseNotice/>{children}</PayrollDraftBoundary></PayrollPurchaseProvider>;
}
