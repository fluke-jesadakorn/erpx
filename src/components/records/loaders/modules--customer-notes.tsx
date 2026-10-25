"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CustomerNotes = dynamic(() => import("@/components/modules/customer-notes").then(module => module.CustomerNotes));
