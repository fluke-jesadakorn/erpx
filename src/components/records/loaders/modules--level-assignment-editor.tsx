"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LevelAssignmentEditor = dynamic(() => import("@/components/people_payroll/level-assignment-editor").then(module => module.LevelAssignmentEditor));
