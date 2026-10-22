"use client";
import dynamic from "next/dynamic";

// Preserve server-rendered detail; load controls only when this detail is rendered.
export const ExpenseAccounting = dynamic(() => import("@erpx/feature-people_payroll/components/expense-workflow-controls").then(module => module.ExpenseAccounting));
export const ExpensePayments = dynamic(() => import("@erpx/feature-people_payroll/components/expense-workflow-controls").then(module => module.ExpensePayments));
export const ExpenseStageActions = dynamic(() => import("@erpx/feature-people_payroll/components/expense-workflow-controls").then(module => module.ExpenseStageActions));
