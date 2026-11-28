"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const AccessEditor = dynamic(() => import("@erpx/identity-access-ui/components/access-editor").then(module => module.AccessEditor));
