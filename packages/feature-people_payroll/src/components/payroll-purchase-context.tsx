"use client";
import { createContext, useContext, type ReactNode } from "react";
import { payrollActionAllowed, type PayrollPurchaseControls } from "@erpx/feature-people_payroll/lib/payroll-purchases";
const Purchase = createContext<PayrollPurchaseControls | undefined>(undefined);
export function PayrollPurchaseProvider({controls, children}: {controls?: PayrollPurchaseControls; children?: ReactNode}) {
  return <Purchase.Provider value={controls}>{children}</Purchase.Provider>;
}
export const usePayrollPurchase = () => useContext(Purchase);
export const usePayrollActionAllowed = (purpose: "new-work" | "settle-existing" | "history") => payrollActionAllowed(usePayrollPurchase(), purpose);
