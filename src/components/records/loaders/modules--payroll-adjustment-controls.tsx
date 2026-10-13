"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollAdjustmentControls = dynamic(() => import("@/components/people_payroll/payroll-adjustment-controls").then(module => module.PayrollAdjustmentControls));
