"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ProjectActualsConsole = dynamic(() => import("@/components/projects/project-actuals").then(module => module.ProjectActualsConsole));
