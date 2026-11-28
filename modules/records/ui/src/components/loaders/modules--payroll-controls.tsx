"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CompensationEditor = dynamic(() => import("@erpx/people-payroll-ui/components/payroll-controls").then(module => module.CompensationEditor));
export const TimeEntryEditor = dynamic(() => import("@erpx/people-payroll-ui/components/self-service/time-entry-editor").then(module => module.TimeEntryEditor));
export const PayrollCorrectionControls = dynamic(() => import("@erpx/people-payroll-ui/components/payroll-controls").then(module => module.PayrollCorrectionControls));
