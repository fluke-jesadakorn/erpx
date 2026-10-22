"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const RecordDocumentPanel = dynamic(() => import("@erpx/feature-files/components/record-document-panel").then(module => module.RecordDocumentPanel));
