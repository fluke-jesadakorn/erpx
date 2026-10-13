"use client";
import dynamic from "next/dynamic";
export const ExpenseEditor = dynamic(() => import("@/components/people_payroll/expense-editor-content").then(module => module.ExpenseEditor), { loading: () => <p role="status">Loading details…</p> });
