"use client";
import dynamic from "next/dynamic";
export const ExpenseEditor = dynamic(() => import("@erpx/people-payroll-ui/components/expense-controls").then(module => module.ExpenseEditor));
