"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const FieldServiceConsole = dynamic(() => import("@/components/modules/field-service-console").then(module => module.FieldServiceConsole));
