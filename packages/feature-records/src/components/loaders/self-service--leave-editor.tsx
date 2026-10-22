"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LeaveDecisions = dynamic(() => import("@erpx/feature-people_payroll/components/self-service/leave-editor").then(module => module.LeaveDecisions));
export const LeaveEditor = dynamic(() => import("@erpx/feature-people_payroll/components/self-service/leave-editor").then(module => module.LeaveEditor));
