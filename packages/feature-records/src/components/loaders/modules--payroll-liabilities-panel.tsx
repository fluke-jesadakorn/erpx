"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollLiabilitiesPanel = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-liabilities-panel").then(module => module.PayrollLiabilitiesPanel));
