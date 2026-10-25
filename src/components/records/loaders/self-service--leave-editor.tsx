"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LeaveDecisions = dynamic(() => import("@/components/self-service/leave-editor").then(module => module.LeaveDecisions));
export const LeaveEditor = dynamic(() => import("@/components/self-service/leave-editor").then(module => module.LeaveEditor));
