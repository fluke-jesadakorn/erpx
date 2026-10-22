"use client";
import dynamic from "next/dynamic";
export const LeaveEditor = dynamic(() => import("@erpx/feature-people_payroll/components/self-service/leave-editor-content").then(module => module.LeaveEditor), { loading: () => <p role="status">Loading details…</p> });
export const LeaveDecisions = dynamic(() => import("@erpx/feature-people_payroll/components/self-service/leave-editor-content").then(module => module.LeaveDecisions), { loading: () => <p role="status">Loading details…</p> });
