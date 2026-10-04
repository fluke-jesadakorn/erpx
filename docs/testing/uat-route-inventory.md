# ERPX UAT route inventory

Generated from `src/app` page files on 2026-10-03. The route source fingerprint is `2bb7b41398a63dbd67c9bd63674c5e5247f466985267564bf4ed523713c60d06`. This inventories implemented page files and is not a capability-catalog acceptance claim.

- 205 page routes in source (current source count)
- Desktop Chrome and Pixel 7 emulation
- Anonymous, intended-role and explicit-denial access checks are recorded separately in Playwright results.
- Holding routes are exercised at holding, branch, department, and branch-plus-department scopes.
- Dynamic detail routes use a simulation fixture ID when one exists and also test an invalid ID.

| Route template | Area | Signed-out expectation | Primary role | Workspace scopes | Data case |
| --- | --- | --- | --- | --- | --- |
| `/access` | Public and account | public | holding_owner | — | page route |
| `/activate` | Public and account | public | holding_owner | — | page route |
| `/book/[holdingSlug]/[propertyId]` | Public booking | public | holding_owner | — | sample record + invalid ID |
| `/book/[holdingSlug]/manage` | Public booking | public | holding_owner | — | page route |
| `/book/[holdingSlug]` | Public booking | public | holding_owner | — | page route |
| `/customer-portal/[holdingSlug]` | Customer portal | login-or-explicit-denial | holding_owner | — | page route |
| `/dev/ui` | Public and account | public | holding_owner | — | page route |
| `/forgot-password` | Public and account | public | holding_owner | — | page route |
| `/forms/[holdingSlug]/[formId]` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/front-access/forgot-password` | Public and account | public | holding_owner | — | page route |
| `/front-access/login` | Public and account | public | holding_owner | — | page route |
| `/front-access` | Public and account | public | holding_owner | — | page route |
| `/front-access/reset-password` | Public and account | public | holding_owner | — | page route |
| `/front-access/verify-email` | Public and account | public | holding_owner | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/[[...path]]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/holdings/[holdingSlug]/agents` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/assets/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/assets` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/benefits/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/benefits` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/brand` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/calendar` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/collaboration` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/commerce/orders/[id]/notes` | Holding workspace | login-or-explicit-denial | purchasing_operator or sales_operator | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/commerce/orders/[id]` | Holding workspace | login-or-explicit-denial | purchasing_operator or sales_operator | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/commerce` | Holding workspace | login-or-explicit-denial | purchasing_operator or sales_operator | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/crm/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/crm` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customer-access/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customer-access` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customer-duplicates/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customer-duplicates` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customer-followups/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customer-followups` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customer-identity/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customer-identity` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/ecommerce/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/ecommerce` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/executive` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/executive/reports/[reportId]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/expenses/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/expenses/new` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/expenses` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/feed` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/field-service/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/field-service` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/assets/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/assets` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/bank` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/close` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/engines` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/group` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/hotels` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/invoices/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/invoices` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/journals/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/ledger` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/payments/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/payments` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/refunds/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/refunds` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/reports` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/statement-packages/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/supplier-batches/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/supplier-batches` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/tax/corrections/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/tax/entries/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/tax/packages/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/tax` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/thai-documents/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/thai-tax` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/future-features` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/handbook/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/handbook` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/home` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/hospitality/setup` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/iam/[[...section]]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inbox` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/landed-costs/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/learn` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/learning/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/learning` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/leave/new` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/leave` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/legal/citations/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/legal` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/maintenance/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/maintenance` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/maintenance/plans/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/manage-erpx/billing` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/manage-erpx/billing/usage` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/manage-erpx/executive` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/manage-erpx/iam/[[...section]]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/manage-erpx` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/manufacturing/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/manufacturing` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/marketing/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/marketing` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/operations/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/operations` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/overtime/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/overtime` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/payroll/advances` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/payroll/history` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/payroll` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/payroll/runs/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/payroll/settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people/access` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/clearance/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people/hire-handoffs` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/move/batch` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/move` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/move/review` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/organization/drafts/[id]/review` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people/organization/drafts` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/organization` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/payroll/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people/payroll/advances` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/payroll/history` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/payroll` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/payroll/recoveries` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/payroll/runs/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people/payroll/settings` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/payslips/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people/payslips` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/roster` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/settings/leave-policy/new` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people/settings` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/planning` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/pms/[[...section]]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/documents/[id]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/import` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pos` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | page route |
| `/holdings/[holdingSlug]/posts/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/procurement-rfqs/[id]` | Holding workspace | login-or-explicit-denial | purchasing_operator or sales_operator | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/procurement-rfqs` | Holding workspace | login-or-explicit-denial | purchasing_operator or sales_operator | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/profiles/[userId]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/quality/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/quality` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/requests/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/requests/new` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/requests` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/routine-templates/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/routine-templates` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/settings/business-readiness` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/settings/notifications` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/settings/pilot-features` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/settings/stripe` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/staff-settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/staff-tasks/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/staff-tasks/new` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/staff-tasks` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/stock-counts/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/stock-counts` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/stock-transfers/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/stock-transfers` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/stock-transit/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/stock-transit` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/supplier-bills/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/supplier-bills` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/support/[id]` | Holding workspace | login-or-explicit-denial | support_agent | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/support/incidents/[id]` | Holding workspace | login-or-explicit-denial | support_agent | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/support/incidents` | Holding workspace | login-or-explicit-denial | support_agent | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/support` | Holding workspace | login-or-explicit-denial | support_agent | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/support/sessions/[id]` | Holding workspace | login-or-explicit-denial | support_agent | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/support/sessions` | Holding workspace | login-or-explicit-denial | support_agent | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/support/sla` | Holding workspace | login-or-explicit-denial | support_agent | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/talent/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/talent` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/tasks/people_payroll/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/technology` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/usage` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/department-approvals/[kind]/[id]` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/work` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/routing/[kind]/[id]` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/work/routing` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/join` | Public and account | public | holding_owner | — | page route |
| `/legal-portal/[holdingSlug]` | Legal portal | login-or-explicit-denial | holding_owner | — | page route |
| `/legal-portal/enroll` | Legal portal | public | holding_owner | — | page route |
| `/login` | Public and account | public | holding_owner | — | page route |
| `/marketing` | Public and account | public | holding_owner | — | page route |
| `/my-access` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/onboarding` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/platform/[destination]` | Platform | login-or-explicit-denial | platform administrator (external identity required) | — | page route |
| `/platform/billing` | Platform | login-or-explicit-denial | platform administrator (external identity required) | — | page route |
| `/platform/executive` | Platform | login-or-explicit-denial | platform administrator (external identity required) | — | page route |
| `/platform/iam/[[...section]]` | Platform | login-or-explicit-denial | platform administrator (external identity required) | — | page route |
| `/platform/preview/[...path]` | Platform | login-or-explicit-denial | platform administrator (external identity required) | — | page route |
| `/platform/tasks/[...path]` | Platform | login-or-explicit-denial | platform administrator (external identity required) | — | page route |
| `/reset-password` | Public and account | public | holding_owner | — | page route |
| `/security` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/signup` | Public and account | public | holding_owner | — | page route |
| `/verify-email` | Public and account | public | holding_owner | — | page route |
| `/work-role` | Public and account | login-or-explicit-denial | holding_owner | — | page route |

## Acceptance boundary

This inventory covers the page files present in source at the fingerprint above. A route passes only when the expected login/permission/Coming Soon/not-found state is visible, the page has no server error, and mobile content does not overflow horizontally. Existing workflow specs remain the interaction coverage for create, review, validation, reload, and recovery behavior.
