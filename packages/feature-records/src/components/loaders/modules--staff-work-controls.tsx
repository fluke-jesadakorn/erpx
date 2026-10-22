"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const TaskActions = dynamic(() => import("@erpx/feature-work_projection/components/staff-work-controls").then(module => module.TaskActions));
export const TaskEditor = dynamic(() => import("@erpx/feature-work_projection/components/staff-work-controls").then(module => module.TaskEditor));
export const TaskDelegationControls = dynamic(() => import("@erpx/feature-work_projection/components/staff-work-controls").then(module => module.TaskDelegationControls));
export const ProjectEditor = dynamic(() => import("@erpx/feature-work_projection/components/staff-work-controls").then(module => module.ProjectEditor));
export const RoutineScheduleControls = dynamic(() => import("@erpx/feature-work_projection/components/staff-work-controls").then(module => module.RoutineScheduleControls));
