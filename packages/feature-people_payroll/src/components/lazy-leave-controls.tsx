"use client";
import dynamic from "next/dynamic";
export const LeaveControls = dynamic(() => import("@erpx/feature-people_payroll/components/leave-controls").then(module => module.LeaveControls));
