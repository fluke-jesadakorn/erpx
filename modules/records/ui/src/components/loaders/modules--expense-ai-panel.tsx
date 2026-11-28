"use client";
import dynamic from "next/dynamic";

// Preserve server-rendered detail; load controls only when this detail is rendered.
export const ExpenseAiPanel = dynamic(() => import("@erpx/people-payroll-ui/components/expense-ai-panel").then(module => module.ExpenseAiPanel));
