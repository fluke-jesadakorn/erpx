"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ExpenseActions = dynamic(() => import("@erpx/feature-people_payroll/components/expense-controls").then(module => module.ExpenseActions));
export const ExpenseEditor = dynamic(() => import("@erpx/feature-people_payroll/components/expense-controls").then(module => module.ExpenseEditor));
export const ExpenseReceipts = dynamic(() => import("@erpx/feature-people_payroll/components/expense-controls").then(module => module.ExpenseReceipts));
