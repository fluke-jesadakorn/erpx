"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollAccountingPanel = dynamic(() => import("@erpx/people-payroll-ui/components/payroll-accounting-panel").then(module => module.PayrollAccountingPanel));
