"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LifecyclePanel = dynamic(() => import("@/components/people_payroll/lifecycle-workspace").then(module => module.LifecyclePanel));
