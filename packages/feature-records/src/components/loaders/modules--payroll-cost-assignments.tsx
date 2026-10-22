"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const PayrollCostAssignmentReview = dynamic(() => import("@erpx/feature-people_payroll/components/payroll-cost-assignments").then(module => module.PayrollCostAssignmentReview));
