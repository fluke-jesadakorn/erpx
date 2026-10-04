export type DeliveryStatus = "implemented" | "flow_accepted" | "sandbox_verified" | "live_verified" | "deferred";
export type DeliveryStage = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7;
export type CapabilityDelivery = {
  stage: DeliveryStage; status: DeliveryStatus; ui: string; api: string;
  service: string; schema: string; test: string; gap: string;
};
type ModuleDelivery = CapabilityDelivery & { overrides?: Record<string, Partial<CapabilityDelivery>> };

// Scoped to the retained 2026-10-03 engineering candidate. Provider labels remain
// compatible with the readiness API; they are independent axes in the report.
// Evidence: docs/testing/evidence/2026-10-03-engineering-completion.json.
const delivery: Record<string, ModuleDelivery> = {
  "brand": {
    "stage": 6,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/brand/page.tsx; src/components/modules/brand-identity-console.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/brand/route.ts",
    "service": "src/server/brand-identity.ts",
    "schema": "migrations/postgres/holding-write/0025_workspace_branding.sql; migrations/postgres/holding-write/0964_brand_identity_lifecycle.sql",
    "test": "tests/brand-identity.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create/edit/preview a versioned identity draft; Independently publish the current reviewed version. Company brand/IP approval is separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-category-locks": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Owner pins approved identity or visual category; Reject conflicting drafts and explicitly unlock for reviewed correction. Company brand/IP approval is separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-published-identity": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Publish through an independent reviewer; Inspect immutable versions and roll back with current version and locks. Company brand/IP approval is separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "commercial": {
    "stage": 6,
    "status": "sandbox_verified",
    "ui": "src/app/platform/billing/page.tsx",
    "api": "src/app/api/v1/platform/billing/[[...path]]/route.ts",
    "service": "src/server/billing.ts",
    "schema": "migrations/postgres/control/0009_billing.sql",
    "test": "tests/billing.test.ts; tests/billing-stripe-sandbox.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Review frozen monthly metered costs; Issue/reconcile Sandbox invoice through loss/retry and current provider state. Only Sandbox billing is supported; live collection, tax, automatic refunds and actual access restrictions are outside the verified offering. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "entitlements": {
        "test": "tests/beta-auth-billing.test.ts; tests/social-beta.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts; tests/billing-api.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Reconcile current Sandbox subscription entitlement; Authorize admission within configured resource capacity. Only Sandbox billing is supported; live collection, tax, automatic refunds and actual access restrictions are outside the verified offering. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-catalog-plans": {
        "test": "tests/billing.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Publish dated plan revision; Preserve prior month and apply approved price next period. Only Sandbox billing is supported; live collection, tax, automatic refunds and actual access restrictions are outside the verified offering. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-subscriptions": {
        "test": "tests/billing.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Consent/enroll in approved Sandbox plan; Schedule/withdraw/end month changes and reconcile late costs. Only Sandbox billing is supported; live collection, tax, automatic refunds and actual access restrictions are outside the verified offering. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-grace-recovery": {
        "test": "tests/billing.test.ts; tests/beta-auth-billing.test.ts; tests/billing-stripe-errors.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Recover payment-method/authentication failure from current provider state; Retry durable webhook delivery and ignore older events. Only Sandbox billing is supported; live collection, tax, automatic refunds and actual access restrictions are outside the verified offering. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "identity_access": {
    "stage": 2,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/iam/[[...section]]/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/iam/[[...path]]/route.ts",
    "service": "src/server/account-access.ts",
    "schema": "migrations/postgres/control/0012_account_invitation_delivery.sql; migrations/postgres/control/0017_sandbox_email_deliveries.sql",
    "test": "tests/iam.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Propose/review scope-limited role assignment; Withdraw parent or direct authority and re-read current context. Real Resend delivery, public multi-session activation and Windows/endpoint enforcement require separate provider/Company acceptance; UCS/Samba evidence is an isolated real-service lab. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "access-reviews": {
        "service": "src/server/iam-service.ts",
        "schema": "migrations/postgres/holding-write/0258_iam_access_review_campaigns.sql",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Start independent access review; Keep or revoke each assignment then complete the campaign. Real Resend delivery, public multi-session activation and Windows/endpoint enforcement require separate provider/Company acceptance; UCS/Samba evidence is an isolated real-service lab. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-people-principals": {
        "service": "src/server/account-recovery.ts",
        "schema": "migrations/postgres/control/0014_account_recovery_delivery.sql; migrations/postgres/control/0017_sandbox_email_deliveries.sql",
        "test": "tests/iam.test.ts; tests/organization-access.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Resolve provider identities to People without email merges; Read permitted person profile with current role. Real Resend delivery, public multi-session activation and Windows/endpoint enforcement require separate provider/Company acceptance; UCS/Samba evidence is an isolated real-service lab. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-roles-permissions": {
        "service": "src/server/iam-service.ts",
        "schema": "migrations/postgres/holding-write/0029_people_directory_operations.sql",
        "test": "tests/iam.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Draft/publish versioned role permissions; Approve grants against the exact current revision. Real Resend delivery, public multi-session activation and Windows/endpoint enforcement require separate provider/Company acceptance; UCS/Samba evidence is an isolated real-service lab. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-directory-connectors": {
        "status": "live_verified",
        "service": "src/server/iam-service.ts",
        "schema": "migrations/postgres/holding-write/0029_people_directory_operations.sql",
        "test": "tests/iam.test.ts; tests/iam-providers.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Sync approved directory adapter with complete snapshot; Handle incomplete/outage/expired evidence. Real Resend delivery, public multi-session activation and Windows/endpoint enforcement require separate provider/Company acceptance; UCS/Samba evidence is an isolated real-service lab. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-identity-conflicts": {
        "service": "src/server/iam-service.ts",
        "schema": "migrations/postgres/holding-write/0029_people_directory_operations.sql",
        "test": "tests/iam.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Import same-contact distinct provider identities; Refuse incomplete authoritative sync. Real Resend delivery, public multi-session activation and Windows/endpoint enforcement require separate provider/Company acceptance; UCS/Samba evidence is an isolated real-service lab. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "organization": {
    "stage": 2,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/people/organization/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/people/[...path]/route.ts",
    "service": "src/server/organization.ts; src/server/company-business-acceptance.ts; src/server/beta-onboarding.ts",
    "schema": "migrations/postgres/holding-write/0257_organization_node_changes.sql; migrations/postgres/holding-write/0272_company_business_acceptance.sql; migrations/postgres/holding-write/0273_company_provisioning_receipts.sql; migrations/postgres/holding-write/0274_organization_archive_relocation.sql",
    "test": "tests/organization.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Inspect company, branch, department and reporting hierarchy; Review placements and reject cycles. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "sharing": {
        "test": "tests/social-beta.test.ts; tests/organization-channels.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Choose explicit audiences and current branch/department participants; Transfer placement and re-read shared content. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-node-lifecycle": {
        "test": "tests/organization.test.ts; tests/e2e/demo-organization-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Propose/review activation, suspension, archive and reopen; Retain dependent placement and immutable history. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-business-units": {
        "test": "tests/organization.test.ts; tests/organization-access.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Inspect Company/branch configuration with scoped directory; Review branch/department lifecycle and placement. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "platform_administration": {
    "stage": 6,
    "status": "flow_accepted",
    "ui": "src/app/platform/[destination]/page.tsx; src/components/platform/platform-holding-provisioning.tsx",
    "api": "src/app/api/v1/platform/operations/provision/route.ts",
    "service": "src/server/platform-operations.ts",
    "schema": "migrations/postgres/control/0016_platform_holding_provisioning.sql",
    "test": "tests/platform-operations.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Validate approved Holding slot and provision; Replay activation and inspect durable history. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-holdings": {
        "ui": "src/app/platform/[destination]/page.tsx",
        "api": "src/app/api/v1/platform/operations/route.ts; src/app/api/v1/platform/operations/provision/route.ts; src/app/api/v1/platform/operations/recover/route.ts; src/app/api/v1/platform/operations/migrate/route.ts",
        "schema": "migrations/postgres/control/0015_platform_migration_runs.sql; migrations/postgres/control/0016_platform_holding_provisioning.sql; migrations/postgres/control",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Provision available Holding resource slot; Inspect current binding and runtime operational state. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-connectors": {
        "ui": "src/app/platform/[destination]/page.tsx",
        "api": "src/app/api/v1/platform/operations/route.ts; src/app/api/v1/platform/operations/provision/route.ts; src/app/api/v1/platform/operations/recover/route.ts; src/app/api/v1/platform/operations/migrate/route.ts",
        "schema": "migrations/postgres/control/0015_platform_migration_runs.sql; migrations/postgres/control/0016_platform_holding_provisioning.sql; migrations/postgres/control",
        "test": "tests/engineering-platform-access.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Register and transition platform connectors. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-infrastructure": {
        "ui": "src/app/platform/[destination]/page.tsx",
        "api": "ops/macos/erpxctl.py",
        "service": "ops/macos/erpxctl.py",
        "schema": "migrations/postgres/control",
        "test": "tests/platform-operations.test.ts; tests/engineering-recovery.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Read control and Holding health/queue state; Inspect unavailable Holding and preserve actionable failure. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-backup-recovery": {
        "ui": "src/app/platform/[destination]/page.tsx",
        "api": "ops/macos/erpxctl.py",
        "service": "ops/macos/erpxctl.py",
        "schema": "migrations/postgres/control",
        "test": "tests/backup-restore-operations.test.ts; tests/engineering-recovery.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create isolated database/file/secret backup with encrypted offsite copy; Restore and refuse corrupt or undecryptable artifact. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-projections-events": {
        "ui": "src/app/platform/[destination]/page.tsx",
        "api": "src/app/api/v1/platform/operations/route.ts; src/app/api/v1/platform/operations/provision/route.ts; src/app/api/v1/platform/operations/recover/route.ts; src/app/api/v1/platform/operations/migrate/route.ts",
        "schema": "migrations/postgres/control/0015_platform_migration_runs.sql; migrations/postgres/control/0016_platform_holding_provisioning.sql; migrations/postgres/control",
        "test": "tests/platform-operations.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Inspect durable event projections and dead letters; Review/retry current failed event once. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-migration-runs": {
        "ui": "src/app/platform/[destination]/page.tsx; src/components/platform/platform-migration-run.tsx",
        "api": "src/app/api/v1/platform/operations/migrate/route.ts",
        "service": "src/server/platform-operations.ts; src/server/runtime/migrations.ts",
        "schema": "migrations/postgres/control/0015_platform_migration_runs.sql; migrations/postgres/holding-write/0275_migration_receipts.sql; migrations/postgres/holding-read/0002_migration_receipts.sql",
        "test": "tests/platform-operations.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Run reviewed additive Holding migrations; Reload durable migration receipt and replay. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-temporary-access": {
        "ui": "src/app/security/page.tsx; src/app/security/security-form.tsx",
        "api": "src/app/api/v1/account/iam/[[...path]]/route.ts",
        "service": "src/server/iam-recovery.ts",
        "schema": "migrations/postgres/control/0006_iam.sql; migrations/postgres/holding-write/0020_iam.sql",
        "test": "tests/engineering-platform-access.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Request, approve and revoke audited access. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-platform-audit": {
        "ui": "src/app/platform/[destination]/page.tsx",
        "api": "src/app/api/v1/platform/operations/route.ts; src/app/api/v1/platform/operations/provision/route.ts; src/app/api/v1/platform/operations/recover/route.ts; src/app/api/v1/platform/operations/migrate/route.ts",
        "schema": "migrations/postgres/control/0015_platform_migration_runs.sql; migrations/postgres/control/0016_platform_holding_provisioning.sql; migrations/postgres/control",
        "test": "tests/iam.test.ts; tests/platform-operations.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Search durable scope-specific audit history; Review command recovery audit and original receipt. Off-machine durability, production RPO/RTO and physical provisioning acceptance require separate operational evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "customer_360": {
    "stage": 3,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/customers/page.tsx; src/app/holdings/[holdingSlug]/customers/[id]/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
    "service": "src/server/customer-timeline.ts; src/server/customer-notes.ts",
    "schema": "migrations/postgres/holding-write/0001_baseline.sql",
    "test": "tests/customer-timeline.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create source customer transactions in their owner modules; Read Customer 360 with matching source permissions; Revoke an owner permission and reread the projection. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "service-timeline": {
        "api": "src/app/api/v1/holdings/[holdingSlug]/customers/route.ts",
        "service": "src/server/customer-timeline.ts",
        "schema": "migrations/postgres/holding-write/0215_crm_leads_opportunities.sql, migrations/postgres/holding-write/0059_legal.sql, migrations/postgres/holding-write/0071_legal_native_references.sql",
        "test": "tests/customer-timeline.test.ts; tests/e2e/demo-customer-timeline.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Record CRM activities, notes and follow-up outcomes; Read a bounded customer timeline; Reload immutable owner sources. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-customer-profile": {
        "test": "tests/customer-notes.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Read the authorized canonical customer profile; Attempt notes against stale and out-of-scope customer versions; Reload profile after an authorized contact record. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-customer-notes": {
        "api": "src/app/api/v1/holdings/[holdingSlug]/customer-notes/route.ts",
        "service": "src/server/customer-notes.ts",
        "schema": "migrations/postgres/holding-write/0212_customer_contact_notes.sql",
        "test": "tests/customer-notes.test.ts; tests/e2e/demo-customer-timeline.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Record a scoped customer contact; Correct the note with expected version; Archive and reload original plus correction events. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "customer_authority": {
    "stage": 3,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/customer-duplicates/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/customer-duplicates/route.ts",
    "service": "src/server/customer-duplicates.ts",
    "schema": "migrations/postgres/holding-write/0222_customer_duplicate_reviews.sql, migrations/postgres/holding-write/0264_customer_duplicate_merge.sql, migrations/postgres/holding-write/0265_customer_duplicate_reconciliation.sql",
    "test": "tests/customer-duplicates.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Review an ordered suspected duplicate pair independently; Choose the canonical identity; Merge safe source references and reload lineage/balances. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "access-history": {
        "ui": "src/app/holdings/[holdingSlug]/customer-access/[id]/page.tsx, src/app/customer-portal/[holdingSlug]/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/customer-access/route.ts, src/app/api/customer-portal/[holdingSlug]/route.ts",
        "service": "src/server/customer-portal.ts",
        "schema": "migrations/postgres/holding-write/0221_customer_access.sql, migrations/postgres/holding-write/0267_customer_portal_actions.sql",
        "test": "tests/customer-access.test.ts; tests/e2e/demo-retail-customer-authority.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Independently approve portal authority; Bind the first verified identity; Read allowlisted documents and attempt a second account. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-authority-bindings": {
        "ui": "src/app/holdings/[holdingSlug]/customer-access/page.tsx, src/app/customer-portal/[holdingSlug]/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/customer-access/route.ts, src/app/api/customer-portal/[holdingSlug]/route.ts",
        "service": "src/server/customer-access.ts, src/server/customer-portal.ts",
        "schema": "migrations/postgres/holding-write/0221_customer_access.sql, migrations/postgres/holding-write/0267_customer_portal_actions.sql",
        "test": "tests/customer-access.test.ts; tests/e2e/demo-retail-customer-authority.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare and independently approve authority; Revoke the current grant; Attempt portal reads and stale review decisions. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-identity-changes": {
        "ui": "src/app/holdings/[holdingSlug]/customer-identity/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/customer-identity/route.ts",
        "service": "src/server/customer-identity-changes.ts",
        "schema": "migrations/postgres/holding-write/0223_customer_identity_changes.sql, migrations/postgres/holding-write/0266_customer_contact_identity.sql",
        "test": "tests/customer-identity-changes.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Propose a customer identity change; Assign an independent eligible reviewer; Apply only against the captured unchanged customer version. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "support": {
    "stage": 3,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/support/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/support/route.ts",
    "service": "src/server/support.ts",
    "schema": "migrations/postgres/holding-write/0216_support_tickets.sql",
    "test": "tests/support.test.ts; tests/e2e/demo-support-lifecycle.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Open, triage, reply and resolve customer tickets. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "sla-watch": {
        "ui": "src/app/holdings/[holdingSlug]/support/sla/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/support/sla/route.ts",
        "service": "src/server/support-sla.ts",
        "schema": "migrations/postgres/holding-write/0217_support_sla.sql",
        "test": "tests/support-sla.test.ts; tests/engineering-workplace-workers.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prioritize tickets nearing their response target; Drain native queues under current Company production rules, recheck enrollment and authority before each result, and retain human review. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "incidents": {
        "ui": "src/app/holdings/[holdingSlug]/support/incidents/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/support/incidents/route.ts",
        "service": "src/server/support-incidents.ts",
        "schema": "migrations/postgres/holding-write/0219_support_incidents.sql",
        "test": "tests/support-incidents.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open incidents, add timeline events and broadcast status. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-support-sessions": {
        "ui": "src/app/holdings/[holdingSlug]/support/sessions/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/support/sessions/route.ts",
        "service": "src/server/support-sessions.ts",
        "schema": "migrations/postgres/holding-write/0224_support_sessions.sql",
        "test": "tests/support-sessions.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Request, approve and revoke customer sessions. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-capabilities": {
        "ui": "src/app/holdings/[holdingSlug]/support/sessions/[id]/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/support/sessions/route.ts",
        "service": "src/server/support-sessions.ts",
        "schema": "migrations/postgres/holding-write/0224_support_sessions.sql",
        "test": "tests/support-sessions.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Issue and revoke scoped support capabilities. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-attachments": {
        "ui": "src/components/modules/support-evidence-console.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/uploads/route.ts",
        "service": "src/server/support-evidence.ts",
        "schema": "migrations/postgres/holding-write/0220_support_evidence.sql",
        "test": "tests/support-evidence.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Share customer-safe evidence or private internal files. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "finance": {
    "stage": 1,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/finance/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/finance/[...path]/route.ts",
    "service": "src/server/finance.ts",
    "schema": "migrations/postgres/holding-write/0262_finance_accrual_schedules.sql; migrations/postgres/holding-write/0279_finance_consolidation_workflow.sql; migrations/postgres/holding-write/0280_finance_consolidation_eliminations.sql",
    "test": "tests/thai-accounting-foundation.test.ts; tests/finance-generic-payments.test.ts; tests/finance-invoice-lines.test.ts; tests/finance-withholding.test.ts; tests/e2e/demo-engineering-money-people.spec.ts; tests/finance-journal-route-encoding.test.ts; tests/commerce-foreign-goods.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create, balance, post and inspect multi-currency journals. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "payables": {
        "test": "tests/finance-generic-payments.test.ts; tests/finance-invoice-lines.test.ts; tests/finance-withholding.test.ts; tests/e2e/demo-engineering-money-people.spec.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create or revise the complete native supplier invoice lines and immutable source snapshots; Independently post the bill and review actual supplier income withholding; Use distinct Treasury approval and exact net bank evidence; retain immediate50tawi copies; Prepare, independently review and remit an ordinary monthly source workpaper; record actual recovery before source cancellation. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-chart-of-accounts": {
        "ui": "src/app/holdings/[holdingSlug]/finance/setup/page.tsx",
        "schema": "migrations/postgres/holding-write/0001_baseline.sql",
        "test": "tests/thai-accounting-foundation.test.ts; tests/finance-opening-balances.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Import a chart, add accounts and change status. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-payments": {
        "ui": "src/app/holdings/[holdingSlug]/finance/page.tsx, src/app/holdings/[holdingSlug]/finance/payments/page.tsx",
        "schema": "migrations/postgres/holding-write/0255_finance_multi_currency.sql",
        "test": "tests/finance-generic-payments.test.ts; tests/finance-withholding.test.ts; tests/e2e/demo-engineering-money-people.spec.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create, approve and release payments. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-receivables-aging": {
        "ui": "src/app/holdings/[holdingSlug]/finance/invoices/page.tsx",
        "schema": "migrations/postgres/holding-write/0256_finance_fx_revaluation.sql",
        "test": "tests/thai-accounting-foundation.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Inspect customer balances and aging. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-bank-reconciliation": {
        "schema": "migrations/postgres/holding-write/0255_finance_multi_currency.sql",
        "test": "tests/finance-generic-payments.test.ts; tests/finance-withholding.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Import CSV, review matches and reconcile. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-reports": {
        "ui": "src/app/holdings/[holdingSlug]/finance/reports/page.tsx",
        "service": "src/server/ledger.ts",
        "schema": "migrations/postgres/holding-write/0001_baseline.sql",
        "test": "tests/finance-reporting.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Select the independently approved Company NPAEs profile and explicitly classify ledger accounts; Prepare current and comparative position, income, equity changes and note disclosures from posted records; A different Finance approver issues a versioned retained export; Amend a later correction into a new edition while retaining the earlier export and source history. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-period-close": {
        "ui": "src/app/holdings/[holdingSlug]/finance/close/page.tsx",
        "schema": "migrations/postgres/holding-write/0256_finance_fx_revaluation.sql",
        "test": "tests/finance-close-reconciliation.test.ts; tests/finance-reporting.test.ts; tests/engineering-finance-close-fixture.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Reconcile actual source GL and dated AR/AP/bank evidence; Prepare and independently approve two consecutive monthly closes; Prepare and independently post the reviewed complete fiscal result into retained earnings; Close the final accounting period and require another Finance approver for reopening. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-opening-balances": {
        "ui": "src/app/holdings/[holdingSlug]/finance/setup/page.tsx",
        "schema": "migrations/postgres/holding-write/0253_finance_control_account_kinds.sql",
        "test": "tests/finance-opening-balances.test.ts; tests/engineering-finance-close-fixture.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Import and verify the opening trial balance. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "pos": {
    "stage": 5,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/pos/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/pos/[...path]/route.ts",
    "service": "src/server/pos.ts",
    "schema": "migrations/postgres/holding-write/0030_pos.sql",
    "test": "tests/pos.test.ts; tests/e2e/demo-retail-customer-authority.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Request a partial refund on a paid sale; Independently approve and record provider/bank payout result; Reload original cost, remaining refundable quantities and journals. Stripe test-mode journeys do not certify live payments, physical devices, registered e-Tax or offline selling. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-register-session": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open an authorized register session; Record split tender against a sale; Count, independently review and close the drawer. Stripe test-mode journeys do not certify live payments, physical devices, registered e-Tax or offline selling. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-sales-baskets": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create a source-priced basket; Submit duplicate line commands concurrently; Reload the basket and retained command receipt. Stripe test-mode journeys do not certify live payments, physical devices, registered e-Tax or offline selling. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-payments-receipts": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Reserve actual available stock and freeze sale prices; Receive cash and post invoice/payment/allocation/stock journal atomically; Retry the lost response and reload sale/source GL. Stripe test-mode journeys do not certify live payments, physical devices, registered e-Tax or offline selling. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-voids": {
        "test": "tests/pos.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Stage clean void evidence for the current sale; Void using the same source version; Attempt a stale attachment set and reload the sale. Stripe test-mode journeys do not certify live payments, physical devices, registered e-Tax or offline selling. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "sales": {
    "stage": 1,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/commerce/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
    "service": "src/server/commerce.ts",
    "schema": "migrations/postgres/holding-write/0001_baseline.sql",
    "test": "tests/sales-pricing.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Independently activate a dated price list; Prepare a quotation with captured prices and discount authority; Request independent manager approval for excess discount. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "orders": {
        "test": "tests/flow-routing.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Approve a versioned credit limit independently; Prepare and approve Sales demand; Attempt excess or stale credit exposure. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "fulfillment": {
        "test": "tests/stock-reservations.test.ts; tests/sales-delivery-evidence.test.ts; tests/e2e/demo-engineering-business.spec.ts; tests/business-scheduled-production.test.ts; tests/commerce-delivery-file-authority.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Approve and confirm Sales demand; Partially fulfill from its own reservation; Finish remaining delivery and reload quantities/costs. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-cancellations": {
        "test": "tests/flow-routing.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare and independently approve an order; Request independent cancellation before stock movement; Attempt cancellation after delivery. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-revenue-handoff": {
        "schema": "migrations/postgres/holding-write/0201_commerce_returns.sql",
        "test": "tests/ecommerce.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Fulfill an approved Sales order and independently post its actual stock journal; Prepare the source receivable and independently post Finance recognition; Collect the exact balance and replay the lost-response command. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "assets": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/assets/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/assets/route.ts",
    "service": "src/server/asset-maintenance.ts",
    "schema": "migrations/postgres/holding-write/0225_asset_maintenance.sql",
    "test": "tests/asset-maintenance.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Recognize a Finance acquisition independently; Register the physical asset with cost/currency and Finance version; Reload its physical event history. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "condition": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Register and place the asset; Record inspection evidence and degraded condition; Reload condition and immutable event sequence. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-placement": {
        "test": "tests/asset-maintenance.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Register an asset linked to recognized Finance cost; Place the asset into an eligible staff member custody; Reload placement and later clear custody on reviewed retirement. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-maintenance-lifecycle": {
        "test": "tests/asset-maintenance.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open corrective work against the current asset revision; Start, block, resume and complete an evidenced repair; Reload physical and maintenance records. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-retirement": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Request physical retirement after all work closes; Prepare and independently post the linked Finance disposal; Reject requester self review and independently approve physical retirement. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "inventory": {
    "stage": 1,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/commerce/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
    "service": "src/server/commerce.ts",
    "schema": "migrations/postgres/holding-write/0239_inventory_lot_serial.sql",
    "test": "tests/stock-reservations.test.ts; tests/commerce-foreign-goods.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create competing approved Sales demand for the last unit; Reserve concurrently using source versions; Reload shared physical and available quantities. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "reservations": {
        "ui": "src/app/holdings/[holdingSlug]/commerce/orders/[id]/page.tsx",
        "service": "src/server/inventory-reservation-alerts.ts",
        "schema": "migrations/postgres/holding-write/0233_planning_manufacturing.sql",
        "test": "tests/stock-reservations.test.ts; tests/business-scheduled-production.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Reserve approved Sales demand; Deliver part of the order from its own reservation; Deliver the remainder and reload quantity/value. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-stock-register": {
        "test": "tests/inventory-lot-serial.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Receive actual controlled lots; Quarantine only the affected lot; Independently inspect/release and compare lot totals to the stock register. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-receipts": {
        "test": "tests/purchase-to-payment.test.ts; tests/e2e/demo-inventory-lot-serial.spec.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Approve sourced procurement demand; Receive six units then the remaining four; Reload receipt sources, inventory and consolidated invoice. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-issues-movements": {
        "ui": "src/app/holdings/[holdingSlug]/stock-transit/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/stock-transit/[[...path]]/route.ts",
        "service": "src/server/inventory-transit.ts",
        "schema": "migrations/postgres/holding-write/0207_inventory_transit.sql",
        "test": "tests/inventory-transit.test.ts; tests/inventory-lot-serial.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Independently approve transfer; Dispatch stock into in-transit custody; Receive partial quantities and final remainder. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "maintenance": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/maintenance/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/maintenance/route.ts; src/app/api/v1/holdings/[holdingSlug]/maintenance/plans/route.ts",
    "service": "src/server/asset-maintenance.ts; src/server/maintenance-preventive.ts",
    "schema": "migrations/postgres/holding-write/0225_asset_maintenance.sql",
    "test": "tests/asset-maintenance.test.ts; tests/maintenance-preventive.test.ts; tests/e2e/demo-engineering-business.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Open an inspection work order; Attempt overlapping work and retirement; Revoke the owner and independently reassign the open work. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "downtime": {
        "test": "tests/asset-maintenance.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open and start maintenance; Record an explicit blocker then resume; Complete with condition/evidence and reload the asset and work order. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-blocked-work": {
        "test": "tests/asset-maintenance.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open and start maintenance; Record an explicit blocker then resume; Complete with condition/evidence and reload the asset and work order. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-completion": {
        "test": "tests/asset-maintenance.test.ts; tests/e2e/demo-engineering-business.spec.ts; tests/finance-journal-route-encoding.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open and start maintenance; Record an explicit blocker then resume; Complete with condition/evidence and reload the asset and work order. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-maintenance-history": {
        "test": "tests/asset-maintenance.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open and start maintenance; Record an explicit blocker then resume; Complete with condition/evidence and reload the asset and work order. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "operations": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/operations/page.tsx; src/components/modules/operations-console.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/operations/route.ts",
    "service": "src/server/operations-exceptions.ts",
    "schema": "migrations/postgres/holding-write/0263_staff_task_delegations.sql; migrations/postgres/holding-write/0271_staff_task_delegation_history.sql",
    "test": "tests/operations-exceptions.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Acknowledge, assign and prioritize exceptions. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-escalations": {
        "test": "tests/operations-exceptions.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Escalate exceptions to an operations lead. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-approval-decisions": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Record reviewed operational decisions. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-resolution": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Resolve and close exceptions. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-queue-policies": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Publish and inspect work-queue policies. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-operations-summary": {
        "test": "tests/operations-exceptions.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts; tests/engineering-workplace-workers.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Inspect area health and handovers; Drain native queues under current Company production rules, recheck enrollment and authority before each result, and retain human review. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "procurement": {
    "stage": 1,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/requests/[id]/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/requests/[[...path]]/route.ts",
    "service": "src/server/employee-requests.ts",
    "schema": "migrations/postgres/holding-write/0238_split_requisition_uom.sql",
    "test": "tests/purchase-to-payment.test.ts; tests/e2e/demo-purchase-request-rfq.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Independently approve ten-unit demand; Compare supplier offers and release an approved purchase order; Receive six and four units, match one AP invoice, post and pay against imported bank evidence. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "purchase-orders": {
        "ui": "src/app/holdings/[holdingSlug]/commerce/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
        "service": "src/server/commerce.ts",
        "schema": "migrations/postgres/holding-write/0001_baseline.sql",
        "test": "tests/purchase-to-payment.test.ts; tests/e2e/demo-purchase-request-rfq.spec.ts; tests/commerce-foreign-goods.test.ts"
      },
      "goods-receipts": {
        "ui": "src/app/holdings/[holdingSlug]/commerce/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
        "service": "src/server/commerce.ts",
        "schema": "migrations/postgres/holding-write/0001_baseline.sql",
        "test": "tests/purchase-to-payment.test.ts; tests/e2e/demo-purchase-request-rfq.spec.ts; tests/commerce-foreign-goods.test.ts"
      },
      "feature-invoice-match": {
        "ui": "src/app/holdings/[holdingSlug]/commerce/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts",
        "service": "src/server/commerce.ts",
        "schema": "migrations/postgres/holding-write/0001_baseline.sql",
        "test": "tests/purchase-to-payment.test.ts; tests/e2e/demo-purchase-request-rfq.spec.ts; tests/commerce-foreign-goods.test.ts"
      },
      "feature-supplier-history": {
        "ui": "src/app/holdings/[holdingSlug]/commerce/orders/[id]/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/supplier-promises/[id]/route.ts",
        "service": "src/server/supplier-promises.ts",
        "schema": "migrations/postgres/holding-write/0208_supplier_promises.sql",
        "test": "tests/procurement-rfqs.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Invite scoped supplier offers; Record immutable price/UOM/expiry quotation evidence; Independently award and replay the PO handoff. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "quality": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/quality/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/quality/[[...path]]/route.ts",
    "service": "src/server/quality.ts; src/server/quality-alerts.ts",
    "schema": "migrations/postgres/holding-write/0244_quality_scrap_disposition.sql; migrations/postgres/holding-write/0281_quality_partial_disposition.sql; migrations/postgres/holding-write/0282_quality_partial_stock_guard.sql; migrations/postgres/holding-write/0283_quality_whole_hold_error.sql; migrations/postgres/holding-write/0284_quality_hold_alerts.sql",
    "test": "tests/quality.test.ts; tests/business-scheduled-production.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare and independently activate measurement thresholds; Inspect the held stock under that source revision; Reload derived results and captured specification. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "results": {
        "test": "tests/quality.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Record a failed inspection without removing the hold; Reinspect and record passing evidence; Have a third person authorize release. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "release-gates": {
        "test": "tests/quality.test.ts; tests/commerce-foreign-goods.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Reserve available Sales stock; Create overlapping quality holds; Attempt fulfillment before and after independent release. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-cancellation": {
        "test": "tests/quality.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create an incorrect hold alongside a valid hold; Request cancellation with reason; Independently review and reload all holds. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "legal": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/legal/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/legal/[[...path]]/route.ts",
    "service": "src/server/legal/service.ts",
    "schema": "migrations/postgres/holding-write",
    "test": "tests/legal.test.ts; tests/legal-g2.test.ts; tests/e2e/legal-uat.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Draft/version/review exact contract and signer route; Independently import/review signed original and activate. External legal acceptance, real portal/provider delivery and production operation require independent review. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-documents": {
        "test": "tests/legal.test.ts; tests/legal-law-corpus.test.ts; tests/legal-g2.test.ts; tests/legal-body.test.ts; tests/legal-download-route.test.ts; tests/legal-citation-page.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Import scanned immutable legal originals; Extract/independently review current source text and export; Upload a canonical private original through its native Legal owner; Reject oversized declared body before proxy truncation can hide its true length; Independently review retained document and law text; Open the exact source passage through installed Next Page params. External legal acceptance, real portal/provider delivery and production operation require independent review. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-legal-holds": {
        "test": "tests/legal.test.ts; tests/attachments-v2.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Apply overlapping holds through typed linked evidence; Independently release one and retain the other. External legal acceptance, real portal/provider delivery and production operation require independent review. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-document-access": {
        "test": "tests/legal.test.ts; tests/legal-law-corpus.test.ts; tests/legal-download-route.test.ts; tests/legal-citation-page.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Share only selected immutable document with authenticated recipient; Withdraw membership/share and re-read current evidence/citations; Authorize a confidential native member or immutable scoped external share; Download through the actual owner and portal route boundaries; Withdraw native member or exact share authority; Open a reviewed citation before and after current authority changes. External legal acceptance, real portal/provider delivery and production operation require independent review. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-archive": {
        "test": "tests/legal.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Close/archive under independently reviewed retention policy; Prevent held disposal and authorize exact manifest independently. External legal acceptance, real portal/provider delivery and production operation require independent review. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "people_payroll": {
    "stage": 2,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/people/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/people/[...path]/route.ts",
    "service": "src/server/people-payroll.ts",
    "schema": "migrations/postgres/holding-write/0026_people_lifecycle.sql",
    "test": "tests/talent.test.ts; tests/payroll-statutory.test.ts; tests/e2e/demo-engineering-money-people.spec.ts; tests/e2e/demo-engineering-thai-payroll.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Hire and transition employee status; Employeeownnormal-role reads exactrecoveryproposal; Employee signs ordeclines current specificagreement; Reload immutable agreement and history. Thai rule-pack/employee evidence readiness remains required; external statutory/business certification is separate. Fixtures do not certify bank delivery. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "payroll-summaries": {
        "schema": "migrations/postgres/holding-write/0026_people_lifecycle.sql, migrations/postgres/holding-write/0202_payroll_core_configuration.sql, migrations/postgres/holding-write/0276_payroll_gl_posting.sql",
        "test": "tests/payroll-statutory.test.ts; tests/e2e/demo-engineering-thai-payroll.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Calculate source-dated Thai statutory payroll and independently review and post its journal; Read restricted Company aggregates by actual pay month; Deny Treasury/employee and scoped department assignments, and isolate other Companies; Post originalstatutoryperiod and pay it; Post and pay separatelatepay correction; Calculate nextperiod using retained postedprioritems; Review recovery as cashreceivable without automatic taxrefund. Thai rule-pack/employee evidence readiness remains required; external statutory/business certification is separate. Fixtures do not certify bank delivery. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-leave-requests": {
        "test": "tests/self-service-journeys.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create and decide leave requests. Thai rule-pack/employee evidence readiness remains required; external statutory/business certification is separate. Fixtures do not certify bank delivery. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-expenses": {
        "test": "tests/expense-v2.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create and decide expense claims. Thai rule-pack/employee evidence readiness remains required; external statutory/business certification is separate. Fixtures do not certify bank delivery. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-payroll-runs": {
        "ui": "src/app/holdings/[holdingSlug]/people/page.tsx, src/app/holdings/[holdingSlug]/people/payroll/[id]/page.tsx",
        "schema": "migrations/postgres/holding-write/0261_payroll_advances.sql, migrations/postgres/holding-write/0202_payroll_core_configuration.sql, migrations/postgres/holding-write/0276_payroll_gl_posting.sql",
        "test": "tests/payroll-statutory.test.ts; tests/payroll-core.test.ts; tests/e2e/demo-engineering-thai-payroll.spec.ts; tests/payroll-cancellation.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare source-dated monthly/day/hour/final-pay payroll from reviewed compensation, time, OT, bonus, service charge and actualYTD; Calculate, independently approve and post the retained run revision and balanced GL; Issue an immutable employee-own payslip in the posting transaction; export reviewed payment instructions and match exact bank delivery; Preserve original reviewed and issued bytes through later correction revisions and deny other employee/Company/revoked reads; Post and settle original source run; Prepare/source-review separately paid adjustment or employee-owned recovery agreement; Independently review and Finance post; Independently approve/export/bank-settle positive cash or collect reviewed recovery in later statutory payroll; Reload source history and reject stale/concurrent/reopened source use. Thai rule-pack/employee evidence readiness remains required; external statutory/business certification is separate. Fixtures do not certify bank delivery. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "crm": {
    "stage": 3,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/crm/page.tsx; src/app/holdings/[holdingSlug]/crm/[id]/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/crm/route.ts; src/app/api/v1/holdings/[holdingSlug]/crm/activities/route.ts",
    "service": "src/server/crm.ts; src/server/crm-activities.ts",
    "schema": "migrations/postgres/holding-write/0001_baseline.sql",
    "test": "tests/crm.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Capture a prospect against an authorized customer identity; Qualify once into a customer opportunity; Reload canonical customer and source opportunity. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "activities": {
        "schema": "migrations/postgres/holding-write/0214_customer_followups.sql; migrations/postgres/holding-write/0286_crm_lead_activities.sql",
        "test": "tests/crm-activities.test.ts; tests/e2e/demo-crm-activities.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Schedule an activity on the current lead version; Deliver activity to the named eligible owner; Record outcome and reload activity/Work/history. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-leads": {
        "schema": "migrations/postgres/holding-write/0215_crm_leads_opportunities.sql",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Capture and qualify a lead under competing commands; Record a lost decision; Reopen without overwriting the original evidence. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-opportunities": {
        "schema": "migrations/postgres/holding-write/0215_crm_leads_opportunities.sql",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Qualify a source lead into an opportunity; Prepare and independently approve its quote; Confirm the Sales order before recording the win. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-sales-handoff": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare the opportunity quote; Use the Sales owner approval and confirmation workflow; Finish the CRM win with the current confirmed source order. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "marketing": {
    "stage": 5,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/marketing/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/marketing/route.ts",
    "service": "src/server/marketing.ts",
    "schema": "migrations/postgres/holding-write/0232_marketing.sql",
    "test": "tests/marketing.test.ts; tests/public-marketing-form.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create typed form, content, domain and provider assets; Submit each for independent review; Reload approved snapshots and explicit readiness. Declared provider adapters/fixtures do not certify live public delivery or production campaigns. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-domains": {
        "test": "tests/marketing.test.ts"
      },
      "feature-provider-readiness": {
        "test": "tests/marketing.test.ts"
      },
      "feature-lead-candidates": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Use an active consented form revision; Capture an attributed lead candidate; Reconcile into the CRM owner command and replay. Declared provider adapters/fixtures do not certify live public delivery or production campaigns. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-engagement": {
        "test": "tests/crm-marketing-analytics.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Capture multiple actual attributed contacts; Query first-touch and last-touch source cohorts; Calculate ROI only from linked posted cost. Declared provider adapters/fixtures do not certify live public delivery or production campaigns. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "manufacturing": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/manufacturing/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/production/route.ts",
    "service": "src/server/production.ts",
    "schema": "migrations/postgres/holding-write/0233_planning_manufacturing.sql",
    "test": "tests/production.test.ts; tests/e2e/demo-engineering-business.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Activate a BOM through an independent reviewer; Calculate and independently release a plan into a manufacturing order; Reserve, issue, report and independently release accepted output. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "material-readiness": {
        "test": "tests/production.test.ts; tests/business-scheduled-production.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Release two orders against the same last available material; Reserve both concurrently; Reload material availability and protected quantities. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "quality-disposition": {
        "test": "tests/production.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Issue protected material; Report output and reject self quality review; Fail, rework, accumulate new conversion costs and independently scrap the unusable batch. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-specifications": {
        "test": "tests/production.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Draft a versioned multi-component BOM; Independently activate the specification; Freeze the approved specification revision in a plan and order. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-labor-output": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Configure effective resource capacity and routing; Independently release an allocated plan; Complete ordered routing operations before reporting labor, overhead and output. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-costs-genealogy": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Independently approve inventory/WIP/conversion Finance account mappings; Issue material and report actual conversion costs; Independently release accepted output and reload stock, cost and GL. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "planning": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/planning/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/production/route.ts",
    "service": "src/server/production.ts",
    "schema": "migrations/postgres/holding-write/0233_planning_manufacturing.sql",
    "test": "tests/production.test.ts; tests/e2e/demo-engineering-business.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Select an approved specification and demand quantity; Calculate a scenario without moving stock; Retain the versioned inputs and resulting recommendation. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "shortages": {
        "test": "tests/production.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Calculate component need from approved BOM and demand; Read available unreserved stock; Reload the persisted shortage result. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "recommendations": {
        "test": "tests/production.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Calculate two plans against the same resource dates; Independently approve competing recommendations; Cancel an unused scenario and replay cancellation. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-capacity": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Configure weekday resource minutes and a dated closure; Calculate sequential setup/run routing; Release capacity and attempt to reduce already reserved date capacity. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-plan-comparison": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Persist a calendar-based plan; Calculate a second plan using the first immutable plan as comparison; Reload both calculation snapshots. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "projects": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/projects/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/projects/[[...path]]/route.ts; src/app/api/v1/holdings/[holdingSlug]/projects/[id]/structure/route.ts",
    "service": "src/server/staff-tasks.ts; src/server/project-structure.ts",
    "schema": "migrations/postgres/holding-write/0001_baseline.sql",
    "test": "tests/project-actuals.test.ts; tests/e2e/demo-engineering-business.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create versioned milestones under the project owner; Connect same-project prerequisite edges; Complete prerequisite and reread successor blockers. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-projects-wbs": {
        "test": "tests/project-structure.test.ts; tests/e2e/demo-client-form-hydration.spec.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create hierarchical owner work packages with sibling order; Execute assigned normal staff tasks; Close child packages before their parent and reload derived WBS. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-handoffs": {
        "test": "tests/project-structure.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Close and classify all actual work and reviews; Prepare immutable project evidence for another current member; Reject, prepare again, independently accept and review reopening. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-time-costs": {
        "test": "tests/project-structure.test.ts; tests/project-actuals.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Submit project time and review through the independent owner; Create a posted Finance expense journal and independently approve its allocation; Link current approved source versions to the completed work package. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-attachments": {
        "test": "tests/project-actuals.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Stage scanned documents for the exact project; Bind through the current project owner; Attempt outsider read and member write. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-project-graph": {
        "test": "tests/project-structure.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Read the WBS hierarchy, prerequisites and actual assigned tasks; Resolve current reviewed time/posted cost sources; Attempt hierarchy/dependency cycles and competing source edits. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "ecommerce": {
    "stage": 5,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/ecommerce/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/ecommerce/route.ts",
    "service": "src/server/ecommerce.ts",
    "schema": "migrations/postgres/holding-write/0234_ecommerce.sql",
    "test": "tests/ecommerce.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create and independently review channel and product listing; Build a versioned cart; Checkout once through Sales approval/confirmation and reload sources. Declared channel/payment adapters do not certify live merchant/provider operation. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "returns": {
        "test": "tests/ecommerce.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Checkout and deliver a Sales-backed order; Independently post invoice, collect exact balance and request partial customer return; Independently release original-cost/credit journals, approve refund and match exact bank evidence. Declared channel/payment adapters do not certify live merchant/provider operation. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-carts": {
        "test": "tests/ecommerce.test.ts; tests/e2e/demo-engineering-business.spec.ts"
      },
      "feature-checkout": {
        "test": "tests/ecommerce.test.ts; tests/e2e/demo-engineering-business.spec.ts"
      },
      "feature-fulfillment": {
        "test": "tests/ecommerce.test.ts; tests/e2e/demo-engineering-business.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Checkout and reserve stock through Sales; Deliver from the owning reservation; Synchronize delivery evidence into the Ecommerce source. Declared channel/payment adapters do not certify live merchant/provider operation. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "benefits": {
    "stage": 6,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/benefits/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/benefits/route.ts",
    "service": "src/server/benefits.ts",
    "schema": "migrations/postgres/holding-write/0229_benefits.sql",
    "test": "tests/benefits.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Add and validate dependents. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "coverage": {
        "test": "tests/benefits.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Elect and validate coverage. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-plans": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Publish benefit plans. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-enrollment-windows": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open and inspect enrollment periods. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-provider-payroll": {
        "test": "tests/benefits.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Submit, acknowledge and release payroll changes. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "learning": {
    "stage": 6,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/learning/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/learning/route.ts",
    "service": "src/server/learning.ts",
    "schema": "migrations/postgres/holding-write/0228_learning.sql",
    "test": "tests/learning.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Publish and retire courses. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-assignments": {
        "test": "tests/learning.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Assign courses to permitted people. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-attendance-results": {
        "test": "tests/learning.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Record attendance and provider results. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-completion": {
        "test": "tests/learning.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Complete or waive enrollments. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-eligibility-handoff": {
        "test": "tests/benefits.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Open related benefits eligibility. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "talent": {
    "stage": 6,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/talent/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/talent/route.ts",
    "service": "src/server/talent.ts",
    "schema": "migrations/postgres/holding-write/0230_talent.sql",
    "test": "tests/talent.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create, approve and transition requisitions. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "applications": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Submit, interview, select or withdraw applications. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "offers": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create, approve, present and decide offers. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-candidates-privacy": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create profiles, holds and erasure requests. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-hire-handoffs": {
        "test": "tests/talent.test.ts; tests/e2e/demo-engineering-money-people.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Request and reconcile employee onboarding. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-proposals": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create, submit, approve and reconcile proposals. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "field_service": {
    "stage": 4,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/field-service/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/field-service/route.ts",
    "service": "src/server/field-service.ts",
    "schema": "migrations/postgres/holding-write/0226_field_service.sql, migrations/postgres/holding-write/0268_field_service_warranty_pricing.sql, migrations/postgres/holding-write/0269_field_service_warranty_snapshots.sql",
    "test": "tests/field-service.test.ts; tests/e2e/demo-engineering-business.spec.ts; tests/business-scheduled-production.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Independently activate a customer service agreement; Dispatch an eligible technician and reserve actual parts; Start visit, resolve exception, record customer confirmation and independently release billing. Customer/provider production acceptance remains separate from the isolated workflow. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "feature-agreements": {
        "test": "tests/field-service.test.ts"
      },
      "feature-dispatch": {
        "test": "tests/field-service.test.ts; tests/e2e/demo-engineering-business.spec.ts"
      },
      "feature-visits": {
        "test": "tests/field-service.test.ts; tests/e2e/demo-engineering-business.spec.ts"
      },
      "feature-exceptions": {
        "test": "tests/field-service.test.ts"
      },
      "feature-billing-release": {
        "test": "tests/field-service.test.ts; tests/e2e/demo-engineering-business.spec.ts"
      }
    }
  },
  "agent": {
    "stage": 7,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/agents/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/agents/route.ts",
    "service": "src/server/agent-governance.ts",
    "schema": "migrations/postgres/holding-write/0231_agent_governance.sql",
    "test": "tests/agent-governance.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Register, suspend and retire agent definitions. Human review and source access remain mandatory; provider operation and autonomous execution are not implied. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "versions": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Add and approve agent versions. Human review and source access remain mandatory; provider operation and autonomous execution are not implied. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "assignments": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Start, delegate, retry or cancel assignments. Human review and source access remain mandatory; provider operation and autonomous execution are not implied. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-pools-routing": {
        "test": "tests/agent-governance.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Register pools, replace members and inspect routing. Human review and source access remain mandatory; provider operation and autonomous execution are not implied. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-private-sessions": {
        "test": "tests/agent-governance.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts; tests/guided-intake.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Run private, held or expiring sessions. Human review and source access remain mandatory; provider operation and autonomous execution are not implied. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-field-suggestions": {
        "test": "tests/ai-providers.test.ts; tests/expense-ai-draft.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts; tests/guided-intake.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Generate cited owner-authorized field proposal; Review/apply explicit fields and invalidate changed source. Human review and source access remain mandatory; provider operation and autonomous execution are not implied. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-invocation-evidence": {
        "test": "tests/agent-governance.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts; tests/guided-intake.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Inspect attributable agent activity. Human review and source access remain mandatory; provider operation and autonomous execution are not implied. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "collaboration": {
    "stage": 2,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/home/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/feed/route.ts",
    "service": "src/server/social.ts",
    "schema": "migrations/postgres/holding-write/0001_baseline.sql",
    "test": "tests/messaging.test.ts; tests/social-beta.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Start direct/team thread with eligible company participants; Reply/read/reload current authorized history. Verification covers the named owner/source adapters; it does not promise arbitrary field linking or universal relation-graph coverage. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "mentions": {
        "test": "tests/social-beta.test.ts; tests/engineering-workplace-workers.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Select eligible scoped mention target; Publish and deliver only to current readable recipients; Drain native queues under current Company production rules, recheck enrollment and authority before each result, and retain human review. Verification covers the named owner/source adapters; it does not promise arbitrary field linking or universal relation-graph coverage. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-comments-moderation": {
        "ui": "src/app/holdings/[holdingSlug]/feed/page.tsx",
        "test": "tests/social-beta.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Reply, edit and tombstone comments. Verification covers the named owner/source adapters; it does not promise arbitrary field linking or universal relation-graph coverage. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-following-reactions": {
        "ui": "src/app/holdings/[holdingSlug]/feed/page.tsx",
        "test": "tests/social-beta.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Follow work and add non-decision signals. Verification covers the named owner/source adapters; it does not promise arbitrary field linking or universal relation-graph coverage. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-topics-feed": {
        "ui": "src/app/holdings/[holdingSlug]/feed/page.tsx",
        "test": "tests/social-beta.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Publish posts and follow governed hashtags. Verification covers the named owner/source adapters; it does not promise arbitrary field linking or universal relation-graph coverage. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-attachments": {
        "ui": "src/app/holdings/[holdingSlug]/inbox/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/messaging/[[...path]]/route.ts",
        "service": "src/server/messaging-files.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Quarantine owned draft files and bind once into authorized content; Revoke membership and recheck original download. Verification covers the named owner/source adapters; it does not promise arbitrary field linking or universal relation-graph coverage. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-action-cards": {
        "ui": "src/app/holdings/[holdingSlug]/collaboration/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/collaboration/route.ts",
        "service": "src/server/collaboration-work.ts",
        "schema": "migrations/postgres/holding-write/0227_collaboration_work.sql",
        "test": "tests/collaboration-work.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Propose and confirm reviewed actions. Verification covers the named owner/source adapters; it does not promise arbitrary field linking or universal relation-graph coverage. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "feature-record-relations": {
        "ui": "src/app/holdings/[holdingSlug]/collaboration/page.tsx",
        "api": "src/app/api/v1/holdings/[holdingSlug]/collaboration/route.ts",
        "service": "src/server/collaboration-work.ts",
        "schema": "migrations/postgres/holding-write/0227_collaboration_work.sql",
        "test": "tests/collaboration-work.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Add, order, inspect and revoke typed links. Verification covers the named owner/source adapters; it does not promise arbitrary field linking or universal relation-graph coverage. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "pms": {
    "stage": 5,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/pms/[[...section]]/page.tsx",
    "api": "src/app/api/v1/holdings/[holdingSlug]/pms/route.ts",
    "service": "src/server/pms.ts",
    "schema": "migrations/postgres/holding-write/0031_pms.sql",
    "test": "tests/pms-read-model.test.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Create actual property/reservation/room sources; Read the bounded dashboard; Compare totals to the full source projection. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "reservations": {
        "test": "tests/pms-booking.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Publish authorized rates and policy; Quote and confirm a multi-night guest reservation; Reload reservation and durable notice queue. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "front-desk": {
        "test": "tests/pms.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Check in an inspected room; Move the in-house guest to another available room; Extend stay under current availability and reload history. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "rooms": {
        "test": "tests/pms.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare a dirty room through housekeeping; Require independent inspection before check-in; Attempt maintenance on occupied inventory. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "housekeeping": {
        "test": "tests/pms.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Create a multi-room booking and check in one room; Cancel only an unarrived room; Attempt stale housekeeping actions against an old occupancy cycle. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "folios": {
        "test": "tests/pms.test.ts; tests/e2e/demo-hospitality-setup.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Post hotel service to a guest folio; Split and merge charge ownership; Reverse the recipient charge with expected version. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "cashier": {
        "test": "tests/pms.test.ts; tests/e2e/demo-hospitality-setup.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare a guest transfer payment without assuming receipt; Match exact imported evidence through a different Finance user; Reload payment allocation and posted journal. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "inventory": {
        "test": "tests/pms.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Charge actual stocked minibar items; Consume source warehouse quantity and cost atomically; Attempt insufficient stock and reload folio/inventory. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "guests": {
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Read property guest sources under current scope; Change company/branch/permission context; Reload every bounded query model. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "reports": {
        "test": "tests/pms-read-model.test.ts; tests/pms.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Populate actual property sources; Read dashboard and source report projection; Compare all authorized totals. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "night-audit": {
        "test": "tests/pms.test.ts; tests/e2e/demo-hospitality-setup.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare an auditable open hotel day; Run competing durable night workers; Restart from committed source chunks and reload the closed day. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "settings": {
        "test": "tests/pms.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Prepare property Finance and operating mappings; Require independent unchanged review; Activate and reload retained readiness evidence. SiteMinder/OTA remains outside this delivery; real source-data cutover, payment providers and hotel acceptance remain separate. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  },
  "manage_erpx": {
    "stage": 6,
    "status": "flow_accepted",
    "ui": "src/app/holdings/[holdingSlug]/manage-erpx/page.tsx",
    "api": "src/app/api/v1/platform/operations/route.ts; src/app/api/v1/platform/operations/provision/route.ts; src/app/api/v1/platform/operations/recover/route.ts; src/app/api/v1/platform/operations/migrate/route.ts",
    "service": "src/server/platform-operations.ts",
    "schema": "migrations/postgres/control",
    "test": "tests/platform-operations.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
    "gap": "Recorded 2026-10-03 engineering acceptance covers: Read source-backed Platform and Holding runtime overview. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision.",
    "overrides": {
      "billing": {
        "ui": "src/app/holdings/[holdingSlug]/manage-erpx/billing/page.tsx; src/components/billing/billing-console.tsx",
        "api": "src/app/api/v1/platform/billing/[[...path]]/route.ts",
        "service": "src/server/billing-api.ts; src/server/billing.ts; src/server/billing-stripe.ts",
        "test": "tests/billing.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts; tests/billing-api.test.ts; tests/billing-stripe-errors.test.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Review metered monthly costs and reconcile Sandbox invoice recovery. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "iam": {
        "ui": "src/app/holdings/[holdingSlug]/manage-erpx/iam/[[...section]]/page.tsx; src/components/iam/iam-console.tsx",
        "api": "src/app/api/v1/platform/iam/[[...path]]/route.ts",
        "service": "src/server/iam-api.ts; src/server/iam-service.ts; src/server/authorization.ts",
        "test": "tests/iam.test.ts; tests/e2e/demo-workplace-platform-lifecycle.spec.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Review current IAM matrix and durable scope-specific evidence. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      },
      "executive": {
        "ui": "src/app/holdings/[holdingSlug]/manage-erpx/executive/page.tsx",
        "api": "src/app/api/v1/platform/executive/route.ts",
        "service": "src/server/platform-executive.ts",
        "gap": "Recorded 2026-10-03 engineering acceptance covers: Read executive operational aggregates from control-plane projections. External provider and production operation require separately scoped evidence. Independent Company business/release acceptance remains separate; this result belongs to the recorded candidate, not every later source revision."
      }
    }
  }
};

export function capabilityDelivery(moduleId: string, capabilityId: string): CapabilityDelivery | undefined {
  const base = delivery[moduleId];
  if (!base) return undefined;
  const { overrides, ...defaults } = base;
  return { ...defaults, ...overrides?.[capabilityId] };
}

export function deliveryModules() { return Object.keys(delivery); }
