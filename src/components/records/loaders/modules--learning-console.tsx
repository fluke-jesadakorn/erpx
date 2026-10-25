"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const LearningConsole = dynamic(() => import("@/components/modules/learning-console").then(module => module.LearningConsole));
