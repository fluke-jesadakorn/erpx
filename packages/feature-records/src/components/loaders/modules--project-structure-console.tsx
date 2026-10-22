"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const ProjectStructureConsole = dynamic(() => import("@erpx/feature-projects/components/project-structure-console").then(module => module.ProjectStructureConsole));
