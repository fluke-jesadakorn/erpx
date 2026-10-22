"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const RecordSections = dynamic(() => import("@erpx/workplace/components/record-sections").then(module => module.RecordSections));
