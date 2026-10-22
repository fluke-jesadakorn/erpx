"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LeaveEvidencePanel = dynamic(() => import("@erpx/feature-people_payroll/components/self-service/leave-evidence-panel").then(module => module.LeaveEvidencePanel));
