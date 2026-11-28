"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ProjectMilestoneConsole = dynamic(() => import("@erpx/projects-ui/components/project-milestones").then(module => module.ProjectMilestoneConsole));
