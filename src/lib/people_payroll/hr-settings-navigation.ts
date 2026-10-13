import type { PeopleHrTopic } from "@/server/modules/people_payroll/public/people-hr-settings";

export const hrSettingsTopics: { id: PeopleHrTopic; title: string; description: string }[] = [
  { id: "levels", title: "Staff levels", description: "Levels and leave defaults" },
  {
    id: "leave",
    title: "Leave & overtime",
    description: "Work policies, entitlements and eligibility",
  },
  { id: "calendars", title: "Holiday calendars", description: "Calendars and dated holidays" },
  {
    id: "pay-groups",
    title: "Pay groups",
    description: "Pay frequency, currency and calculation policy",
  },
  {
    id: "templates",
    title: "Organization templates",
    description: "Reusable positions and Holding HR templates",
  },
  {
    id: "lifecycle",
    title: "Lifecycle handoff",
    description: "Onboarding, offboarding and IT task ownership",
  },
];
