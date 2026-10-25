"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollAccountingPanel = dynamic(() => import("@/components/modules/payroll-accounting-panel").then(module => module.PayrollAccountingPanel));
