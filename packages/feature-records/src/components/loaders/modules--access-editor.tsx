"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const AccessEditor = dynamic(() => import("@erpx/feature-identity_access/components/access-editor").then(module => module.AccessEditor));
