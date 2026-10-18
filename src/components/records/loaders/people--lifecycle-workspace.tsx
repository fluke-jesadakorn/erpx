"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LifecyclePanel = dynamic(() => import("@/components/people_payroll/directory-lifecycle-panel").then(module => module.DirectoryLifecyclePanel));
