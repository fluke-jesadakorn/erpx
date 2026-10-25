"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollPaymentPanel = dynamic(() => import("@/components/modules/payroll-payment-panel").then(module => module.PayrollPaymentPanel));
