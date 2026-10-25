"use client";
import dynamic from "next/dynamic";
export const ExpenseEditor = dynamic(() => import("./expense-editor-content").then(module => module.ExpenseEditor), { loading: () => <p role="status">Loading details…</p> });
