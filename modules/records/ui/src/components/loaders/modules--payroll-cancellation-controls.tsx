"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollCancellationControls = dynamic(() => import("@erpx/people-payroll-ui/components/payroll-cancellation-controls").then(module => module.PayrollCancellationControls));
