"use client";
import dynamic from "next/dynamic";
export const RequestEditor = dynamic(() => import("./request-controls-content").then(module => module.RequestEditor), { loading: () => <p role="status">Loading details…</p> });
export const RequestActions = dynamic(() => import("./request-controls-content").then(module => module.RequestActions), { loading: () => <p role="status">Loading details…</p> });
export const RequestFiles = dynamic(() => import("./request-controls-content").then(module => module.RequestFiles), { loading: () => <p role="status">Loading details…</p> });
export const RequestHistory = dynamic(() => import("./request-controls-content").then(module => module.RequestHistory), { loading: () => <p role="status">Loading details…</p> });
export const RequestState = dynamic(() => import("./request-controls-content").then(module => module.RequestState), { loading: () => <p role="status">Loading details…</p> });
export type { RequestOptions } from "./request-controls-content";
