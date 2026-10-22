"use client";
import dynamic from "next/dynamic";

// Keep the server-rendered detail, but load its client code only when rendered.
export const AgentGovernanceConsole = dynamic(() => import("@erpx/feature-agent/components/agent-governance-console").then(module => module.AgentGovernanceConsole));
