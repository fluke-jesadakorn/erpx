"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const AccessEditor = dynamic(() => import("@/components/identity_access/access-editor").then(module => module.AccessEditor));
