"use client";
import dynamic from "next/dynamic";

// Preserve server-rendered detail; load controls only when this detail is rendered.
export const ExpenseAiPanel = dynamic(() => import("@/components/people_payroll/expense-ai-panel").then(module => module.ExpenseAiPanel));
