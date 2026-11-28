"use client";
import dynamic from "next/dynamic";
export const LeaveControls = dynamic(() => import("@erpx/people-payroll-ui/components/leave-controls").then(module => module.LeaveControls));
