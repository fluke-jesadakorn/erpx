"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LifecyclePanel = dynamic(() => import("@erpx/feature-people_payroll/components/directory-lifecycle-panel").then(module => module.DirectoryLifecyclePanel));
