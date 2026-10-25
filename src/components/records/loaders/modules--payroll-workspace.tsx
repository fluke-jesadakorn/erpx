"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollDetailOverview = dynamic(() => import("@/components/modules/payroll-workspace").then(module => module.PayrollDetailOverview));
export const PayrollDisclosure = dynamic(() => import("@/components/modules/payroll-workspace").then(module => module.PayrollDisclosure));
export const PayrollHeading = dynamic(() => import("@/components/modules/payroll-workspace").then(module => module.PayrollHeading));
export const PayrollItemList = dynamic(() => import("@/components/modules/payroll-workspace").then(module => module.PayrollItemList));
export const PayrollNote = dynamic(() => import("@/components/modules/payroll-workspace").then(module => module.PayrollNote));
export const PayrollPolicySummary = dynamic(() => import("@/components/modules/payroll-workspace").then(module => module.PayrollPolicySummary));
export const PayrollReadinessPanel = dynamic(() => import("@/components/modules/payroll-workspace").then(module => module.PayrollReadinessPanel));
