"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const DepartmentOverrideDecision = dynamic(() => import("@/components/organization/department-override-decision").then(module => module.DepartmentOverrideDecision));
