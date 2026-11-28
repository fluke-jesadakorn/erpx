"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ClearanceWorkspace = dynamic(() => import("@erpx/people-payroll-ui/components/clearance-workspace").then(module => module.ClearanceWorkspace));
