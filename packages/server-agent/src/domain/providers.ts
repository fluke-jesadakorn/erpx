import {z} from 'zod';
import type {AiProtocol,AiPreset} from "@erpx/feature-shared/lib/ai/providers";
export const aiConnectionSchema = z.object({
  expectedVersion: z.number().int().nonnegative().default(0),
  name: z.string().trim().min(2).max(100),
  baseUrl: z.string().trim().max(500),
  authMode: z.enum(["bearer", "none"]).default("bearer"),
  apiKey: z.string().max(8000).optional(),
  removeKey: z.boolean().default(false),
  providerPreset: z.enum(["custom", "opencode-zen", "opencode-go"]).default("custom"),
  textModel: z.string().trim().min(1).max(200),
  imageModel: z.string().trim().max(200).default(""),
});
export const aiVersionSchema = z.object({
  expectedVersion: z.number().int().positive(),
});
export const aiSelectionSchema = z.object({
  expectedVersion: z.number().int().nonnegative(),
  mode: z.enum(["deployment", "custom", "disabled"]),
  textConnectionId: z.string().nullable().default(null),
  imageMode: z.enum(["default", "custom", "disabled"]).default("default"),
  imageConnectionId: z.string().nullable().default(null),
  homeAiName: z.string().trim().min(1).max(40).refine(value => !/[@\r\n]/.test(value)).optional(),
});
export type Connection = {
  id: string;
  company_id: string;
  name: string;
  version: number;
  draft_revision: number;
  active_revision: number | null;
  disabled: number;
  deleted_at: number | null;
};
export type Revision = {
  connection_id: string;
  revision: number;
  base_url: string;
  auth_mode: "bearer" | "none";
  credential_json: string | null;
  provider_preset: AiPreset;
  text_protocol: AiProtocol;
  image_protocol: AiProtocol;
  text_model: string;
  image_model: string | null;
  text_verified: number;
  image_verified: number;
  tested_at: number | null;
  test_error: string | null;
};
export type Selection = {
  company_id: string;
  version: number;
  mode: "deployment" | "custom" | "disabled";
  text_connection_id: string | null;
  image_connection_id: string | null;
  image_mode: "default" | "custom" | "disabled";
  home_ai_name: string;
};
export type AiConnectionView = Connection &
  Omit<Revision, "credential_json"> & {
    hasKey: boolean;
    active: Pick<Revision, "base_url" | "text_model" | "image_model" | "text_protocol" | "image_protocol"> | null;
  };
export type AiProviderOverview = {
  connections: AiConnectionView[];
  selection: Selection;
  deploymentAvailable: boolean;
  keyStorageReady: boolean;
  usage: {
    connection_id: string | null;
    model: string;
    requests: number;
    input_tokens: number;
    output_tokens: number;
    failures: number;
  }[];
};
export type AiProviderSnapshot = {
  companyId: string;
  connectionId: string | null;
  revision: number;
  settingsVersion: number;
  model: string;
  kind: "text" | "image";
  deploymentDigest?: string;
};
export const aiDiscoverySchema = z.object({
  baseUrl: z.string().trim().max(500),
  authMode: z.enum(["bearer", "none"]).default("bearer"),
  apiKey: z.string().max(8000).optional(),
  connectionId: z.string().max(180).optional(),
  removeKey: z.boolean().default(false),
});
