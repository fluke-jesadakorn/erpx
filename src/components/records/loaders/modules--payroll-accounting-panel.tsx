"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollAccountingPanel = dynamic(() => import("@/components/people_payroll/payroll-accounting-panel").then(module => module.PayrollAccountingPanel));
