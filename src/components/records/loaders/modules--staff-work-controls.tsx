"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const TaskActions = dynamic(() => import("@/components/work_projection/staff-work-controls").then(module => module.TaskActions));
export const TaskEditor = dynamic(() => import("@/components/work_projection/staff-work-controls").then(module => module.TaskEditor));
export const TaskDelegationControls = dynamic(() => import("@/components/work_projection/staff-work-controls").then(module => module.TaskDelegationControls));
export const ProjectEditor = dynamic(() => import("@/components/work_projection/staff-work-controls").then(module => module.ProjectEditor));
export const RoutineScheduleControls = dynamic(() => import("@/components/work_projection/staff-work-controls").then(module => module.RoutineScheduleControls));
