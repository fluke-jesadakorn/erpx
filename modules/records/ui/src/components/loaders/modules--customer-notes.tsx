"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const CustomerNotes = dynamic(() => import("@erpx/customer-360-ui/components/customer-notes").then(module => module.CustomerNotes));
