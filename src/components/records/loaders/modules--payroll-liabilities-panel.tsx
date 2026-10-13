"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollLiabilitiesPanel = dynamic(() => import("@/components/people_payroll/payroll-liabilities-panel").then(module => module.PayrollLiabilitiesPanel));
