"use client";
import dynamic from "next/dynamic";
export const ExpenseEditor = dynamic(() => import("@erpx/feature-people_payroll/components/expense-controls").then(module => module.ExpenseEditor));
