"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ExpenseActions = dynamic(() => import("@/components/modules/expense-controls").then(module => module.ExpenseActions));
export const ExpenseEditor = dynamic(() => import("@/components/modules/expense-controls").then(module => module.ExpenseEditor));
export const ExpenseReceipts = dynamic(() => import("@/components/modules/expense-controls").then(module => module.ExpenseReceipts));
