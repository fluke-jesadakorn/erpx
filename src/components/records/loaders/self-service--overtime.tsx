"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const OvertimeForm = dynamic(() => import("@/components/people_payroll/self-service/overtime").then(module => module.OvertimeForm));
