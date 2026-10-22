export type AppearancePreference = "system" | "light" | "dark";

export function appearancePreference(value: string | undefined): AppearancePreference {
  return value === "light" || value === "dark" ? value : "system";
}
