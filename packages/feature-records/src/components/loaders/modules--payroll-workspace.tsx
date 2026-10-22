"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollDetailOverview = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-workspace").then(module => module.PayrollDetailOverview));
export const PayrollDisclosure = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-workspace").then(module => module.PayrollDisclosure));
export const PayrollHeading = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-workspace").then(module => module.PayrollHeading));
export const PayrollItemList = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-workspace").then(module => module.PayrollItemList));
export const PayrollNote = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-workspace").then(module => module.PayrollNote));
export const PayrollPolicySummary = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-workspace").then(module => module.PayrollPolicySummary));
export const PayrollReadinessPanel = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-workspace").then(module => module.PayrollReadinessPanel));
