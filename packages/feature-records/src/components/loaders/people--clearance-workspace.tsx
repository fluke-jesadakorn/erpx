"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ClearanceWorkspace = dynamic(() => import("@erpx/feature-people_payroll/components/clearance-workspace").then(module => module.ClearanceWorkspace));
