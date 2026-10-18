"use client";
import type {ReactNode} from "react";
import type {PayrollPurchaseControls} from "@/lib/people_payroll/payroll-purchases";
import {PayrollPurchaseProvider} from "./payroll-purchase-context";
import {PayrollPurchaseNotice} from "./payroll-purchase-notice";
import {PayrollDraftBoundary} from "./payroll-draft-boundary";
export function PayrollEnvironment({purchase, children}: {purchase?: PayrollPurchaseControls; children: ReactNode}) {
  return <PayrollPurchaseProvider controls={purchase}><PayrollDraftBoundary><PayrollPurchaseNotice/>{children}</PayrollDraftBoundary></PayrollPurchaseProvider>;
}
