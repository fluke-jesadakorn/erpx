# ERPX UAT route inventory

Generated from `src/app` page files on 2026-10-18. The route source fingerprint is `7a198337979a0b63cde7890337d023c5489a0075c3b1636debc034d104adcc0a`. This inventories implemented page files and is not a capability-catalog acceptance claim.

- 941 page routes in source (current source count)
- Desktop Chrome and Pixel 7 emulation
- Anonymous, intended-role and explicit-denial access checks are recorded separately in Playwright results.
- Holding routes are exercised at holding, branch, department, and branch-plus-department scopes.
- Dynamic detail routes use a simulation fixture ID when one exists and also test an invalid ID.

| Route template | Area | Signed-out expectation | Primary role | Workspace scopes | Data case |
| --- | --- | --- | --- | --- | --- |
| `/access` | Public and account | public | holding_owner | — | page route |
| `/activate` | Public and account | public | holding_owner | — | page route |
| `/admin/choose-work-role` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/book/[holdingSlug]/[propertyId]` | Public booking | public | holding_owner | — | sample record + invalid ID |
| `/book/[holdingSlug]/manage` | Public booking | public | holding_owner | — | page route |
| `/book/[holdingSlug]` | Public booking | public | holding_owner | — | page route |
| `/choose-work-role` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/customer-portal/[holdingSlug]` | Customer portal | login-or-explicit-denial | holding_owner | — | page route |
| `/customer-portal/[holdingSlug]/tickets` | Customer portal | login-or-explicit-denial | holding_owner | — | page route |
| `/dev/ui` | Public and account | public | holding_owner | — | page route |
| `/forgot-password` | Public and account | public | holding_owner | — | page route |
| `/forms/[holdingSlug]/[formId]` | Public forms | public | public form visitor | — | sample record + invalid ID |
| `/front-access/forgot-password` | Public and account | public | holding_owner | — | page route |
| `/front-access/login` | Public and account | public | holding_owner | — | page route |
| `/front-access` | Public and account | public | holding_owner | — | page route |
| `/front-access/reset-password` | Public and account | public | holding_owner | — | page route |
| `/front-access/verify-email` | Public and account | public | holding_owner | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/cashier` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/cashier/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/cashier` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/cashier/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/front-desk` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/front-desk/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/guest-accounts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/guest-accounts/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/history` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/history/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/housekeeping` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/housekeeping/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/kitchen` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/night-audit` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/night-audit/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/notifications` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/reservations` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/reservations/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/rooms` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/rooms/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/sale` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/sale/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/shifts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/shifts/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/today` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/today/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/front-desk` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/front-desk/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/guest-accounts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/guest-accounts/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/history` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/history/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/housekeeping` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/housekeeping/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/kitchen` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/night-audit` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/night-audit/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/notifications` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/reservations` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/reservations/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/rooms` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/rooms/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/sale` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/sale/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/shifts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/shifts/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/today` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/today/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/cashier` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/cashier` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/cashier/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/front-desk` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/front-desk/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/guest-accounts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/guest-accounts/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/history` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/history/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/housekeeping` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/housekeeping/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/kitchen` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/night-audit` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/night-audit/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/notifications` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/reservations` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/reservations/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/rooms` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/rooms/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/sale` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/sale/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/shifts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/shifts/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/today` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/today/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/front-desk` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/guest-accounts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/history` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/cashier` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/cashier/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/front-desk` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/front-desk/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/guest-accounts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/guest-accounts/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/history` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/history/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/housekeeping` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/housekeeping/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/kitchen` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/night-audit` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/night-audit/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/notifications` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/reservations` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/reservations/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/rooms` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/rooms/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/sale` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/sale/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/shifts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/shifts/receipts/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/today` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/holding/today/records/[recordKind]/[id]/[recordSection]` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | sample record + invalid ID |
| `/front/[holdingSlug]/[companySlug]/[app]/housekeeping` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/kitchen` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/night-audit` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/notifications` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]` | Front PMS/POS | login-or-explicit-denial | holding_owner | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/reservations` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/rooms` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/sale` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/shifts` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/front/[holdingSlug]/[companySlug]/[app]/today` | Front PMS/POS | login-or-explicit-denial | pms_frontdesk or pos_cashier | — | page route |
| `/holdings/[holdingSlug]/administration/agents` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/agents/records/agent_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_assignment/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_invocation/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_invocation/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_pool/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_pool/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_session/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_session/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_version/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/agents/records/agent_version/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/ai` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/approvals/history` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/approvals/issues` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/approvals` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/approvals/templates` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/brand/history` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/brand/identity` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/brand/media` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/brand` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/brand/records/brand_draft/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/brand/records/brand_draft/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/brand/records/brand_version/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/brand/records/brand_version/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/business-readiness` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/notifications` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/pilot-features` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/stripe` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/directory/connections` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/directory/connections/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/directory/connections/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/directory/mappings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/directory/objects` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/directory` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/directory/work` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization/branch/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/branch/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/branch` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization/company/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/company/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/company` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization/department/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/department/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/department` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization/level/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/level/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/level` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization/person/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/person/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/person` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization/position/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/position/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/position` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization/team/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/team/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/team` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/organization/unassigned/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/unassigned/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/organization/unassigned` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/resources/accounts` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/resources/groups` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/resources/groups/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/resources/groups/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/resources/packages` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/resources` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/resources/policies` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/resources/tasks` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/settings/attributes` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/settings/groups` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/settings/groups/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/settings/groups/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/settings/matrix` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/settings/matrix/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/settings/matrix/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/settings/reviews` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/settings/reviews/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/settings/reviews/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/settings/roles` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/administration/technology/settings/roles/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/technology/settings/roles/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/administration/usage` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/calendar/coverage` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/calendar/mine` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/calendar` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/calendar/team` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/collaboration` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/contacts` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/directory/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/directory/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/directory` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/prospects` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/records/party/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/records/party/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/access/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/access/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/access` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/duplicates/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/duplicates/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/duplicates` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/identity/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/identity/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews/identity` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/reviews` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/service/cases/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/service/cases/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/service/cases` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/service/incidents/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/service/incidents/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/service/incidents` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/service` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/service/portal/[bindingId]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/service/portal` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/service/sessions/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/service/sessions/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/customers/service/sessions` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/customers/service/sla` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/ecommerce/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/ecommerce/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/ecommerce` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/ecommerce/records/ecommerce_cart/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/ecommerce/records/ecommerce_cart/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/ecommerce/records/ecommerce_channel/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/ecommerce/records/ecommerce_channel/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/ecommerce/records/ecommerce_listing/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/ecommerce/records/ecommerce_listing/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/marketing/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/marketing/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/marketing` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/marketing/records/marketing_campaign_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/marketing/records/marketing_campaign_sources/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/documents` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/documents/quote/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/documents/quote/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/documents/quote` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/documents/sales/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/documents/sales/[id]/notes` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/documents/sales/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/documents/sales` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/followups/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/followups/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/followups` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/pipeline/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/pipeline/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/pipeline` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/pricing` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/pricing/records/price_list/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/pricing/records/price_list/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/reports` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/settings/initial-stock/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/customers-sales/sales/settings/initial-stock` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/settings/posting` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/settings/services` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/customers-sales/sales/today` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/billing/history` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/billing/history/records/billing_cost/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/history/records/billing_line/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/history/records/billing_source/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/history/records/billing_statement/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/invoices` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/billing/invoices/records/billing_cost/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/invoices/records/billing_line/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/invoices/records/billing_source/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/invoices/records/billing_statement/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/overview` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/billing/overview/records/billing_cost/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/overview/records/billing_line/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/overview/records/billing_source/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/overview/records/billing_statement/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/billing/payment` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/billing/payment/records/billing_cost/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/payment/records/billing_line/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/payment/records/billing_source/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/payment/records/billing_statement/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/plans` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/billing/plans/records/billing_cost/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/plans/records/billing_line/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/plans/records/billing_source/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/plans/records/billing_statement/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/target` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/billing/target/records/billing_cost/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/target/records/billing_line/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/target/records/billing_source/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/billing/target/records/billing_statement/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/executive` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/connections` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/connections/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/connections/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/groups` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/groups/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/groups/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/matrix` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/matrix/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/matrix/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/my-access` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/my-access/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/my-access/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/overview` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/overview/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/overview/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/reviews` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/reviews/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/reviews/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/roles` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/roles/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/roles/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/users` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/iam/users/records/iam_assignment/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/iam/users/records/iam_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/records/platform_provision/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/usage` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/erpx-management/usage/records/usage_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/erpx-management/usage/records/usage_item/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/assets/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/assets/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/assets` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/book-adjustments` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/close/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/close/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/close` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/engines` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/hotels/accounts/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/hotels/accounts` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/hotels/documents/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/hotels/documents` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/hotels` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/instruments` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/journals/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/journals/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/journals` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/records/accrual_run/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/accrual_run/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/accrual_schedule/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/accrual_schedule/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/aging_bucket/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/aging_bucket/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/calculation_run/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/calculation_run/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/consolidation/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/consolidation/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/consolidation_elimination/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/consolidation_elimination/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_account/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_account/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_asset_policy/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_asset_policy/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_asset_policy_run/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_asset_policy_run/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_book/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_book/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_budget_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_budget_sources/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_business_bill/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_business_bill/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_business_contract/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_business_contract/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_acquisition/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_acquisition/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_edition/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_edition/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_ownership/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_ownership/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_pair/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_pair/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_source/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_source/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_group_sources/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_instrument/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_instrument/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_instrument_run/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_instrument_run/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_obligation/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_obligation/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_plan/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_plan/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_recognition/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_recognition/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_report/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_report/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_report_row/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_report_row/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_report_snapshot/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/finance_report_snapshot/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/intercompany/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/intercompany/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/price_list/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/price_list/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/statement_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/statement_sources/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/trial_balance/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/records/trial_balance/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/restaurants` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/tax/corrections/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/tax/corrections/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/tax/corrections` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/tax/entries/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/tax/entries/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/tax/entries` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/tax/packages/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/tax/packages/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/accounting/tax/packages` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/accounting/tax` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/cash/bank` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/cash/collections` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/cash` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/cash/payments/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/payments/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/payments` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/cash/records/bank_match_group/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/bank_match_group/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/bank_profile/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/bank_profile/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/bank_statement/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/bank_statement/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/bank_transaction/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/bank_transaction/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/cash_forecast_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/cash_forecast_sources/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/collection_case/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/records/collection_case/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/refunds/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/refunds/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/refunds` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/cash/supplier-batches/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/supplier-batches/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/cash/supplier-batches` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/contracts` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/due-drafts` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/expenses/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/documents/expenses/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/documents/expenses/new` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/expenses` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/invoices/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/documents/invoices/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/documents/invoices` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/payable` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/receivable` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/sales/invoices` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/sales/new` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/sales` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/sales/quote` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/sales/receipts/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/documents/sales/receipts` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/sales/sales` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/documents/sales/settings` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/reports/company` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/reports/group-editions/[id]/source/[journalId]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/reports/group-editions` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/reports/group` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/reports` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/reports/statement-packages/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/reports/statement-packages/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/reports/statement-packages` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/ai` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/asset-policies` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/books` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/chart` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/checklist` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/expenses` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/hospitality-settings` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/hotel-import` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/opening` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/policy` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/profile` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/thai-documents/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/setup/thai-documents/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/finance/setup/thai-documents` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/thai-tax` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/setup/tools` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/today/actions` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/today` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/today/team` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/finance/today/waiting` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/home` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/hospitality` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inbox/messages` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inbox/notifications` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inbox` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inbox/teams` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/insights/executive` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/insights/executive/records/executive_source/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/insights/executive/records/executive_source/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/insights/executive/records/executive_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/insights/executive/records/executive_sources/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/insights/executive/reports/[reportId]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/insights/executive/reports` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/insights` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inventory-quality/counts/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/counts/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/counts` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inventory-quality` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inventory-quality/products` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inventory-quality/quality/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/quality/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/quality` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inventory-quality/records/inventory_lot/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/inventory_lot/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/inventory_movement/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/inventory_movement/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/product/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/product/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/reorder_policy/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/reorder_policy/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/stock_position/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/stock_position/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/warehouse/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/records/warehouse/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/stock` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inventory-quality/transfers/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/transfers/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/transfers` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inventory-quality/transit/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/transit/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/inventory-quality/transit` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/inventory-quality/warehouses` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/guides` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/handbook/drafts` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/handbook/editors` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/handbook` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/handbook/published/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/knowledge/handbook/published/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/knowledge/handbook/published` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/help` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/learning/assignments/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/knowledge/learning/assignments/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/knowledge/learning/assignments` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/learning/courses` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/learning/mine` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/learning` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/learning/records/learning_course/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/knowledge/learning/records/learning_course/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/knowledge/learning/settings/learners/[personId]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/learning/settings/learners` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/learning/settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/knowledge/roadmap` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/leave/new` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/leave` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/overtime/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/overtime/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/overtime` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/benefits/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/benefits/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/benefits/enrollments` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/benefits/mine` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/benefits` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/benefits/plans` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/benefits/records/benefit_plan/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/benefits/records/benefit_plan/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/clearance/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/clearance/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/clearance` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/archived` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/branch/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/branch/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/branch` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/company/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/company/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/company` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/current` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/department/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/department/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/department` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/level/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/level/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/level` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/person/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/person/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/person` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/position/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/position/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/position` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/team/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/team/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/team` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/directory/unassigned/[id]/[explorerSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/unassigned/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/directory/unassigned` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/evaluation/criteria` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/evaluation/cycles` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/evaluation/graph` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/evaluation/individual` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/evaluation` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/evaluation/table` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/hire-handoffs/[id]/overview` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/hire-handoffs/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/hire-handoffs` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/hr-settings/calendars` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/hr-settings/leave` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/hr-settings/levels` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/hr-settings/lifecycle` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/hr-settings` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/hr-settings/pay-groups` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/hr-settings/templates` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/move/batch` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/move` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/move/review` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/organization/drafts/[id]/review` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/organization/drafts` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/organization` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payroll/advances` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payroll/dashboard` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payroll/history` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payroll` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payroll/records/payroll_month/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/payroll/records/payroll_month/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/payroll/recoveries` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payroll/runs/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/payroll/runs/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/payroll/runs` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payroll/settings` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payroll/settings/posting` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/payslips/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/payslips/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/people/payslips` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/roster` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/users-access/organizations` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/users-access` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/users-access/roles` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/users-access/teams` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/people/users-access/users` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/talent/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/talent/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/talent/candidates/[id]/privacy/overview` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/talent/candidates/[id]/privacy` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/talent` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/people-organization/talent/records/talent_candidate/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/talent/records/talent_candidate/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/talent/records/talent_requisition/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/people-organization/talent/records/talent_requisition/[id]` | Holding workspace | login-or-explicit-denial | people_manager | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/approvals` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/approvals/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/approvals/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/approvals/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/approvals/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/cashier` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/cashier/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/cashier/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/cashier/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/cashier/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/dashboard` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/dashboard/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/dashboard/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/dashboard/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/dashboard/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/documents/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/documents/[id]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/documents` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/front-desk` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/front-desk/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/front-desk/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/front-desk/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/front-desk/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/guest-accounts` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/guest-accounts/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/guest-accounts/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/guest-accounts/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/guest-accounts/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/guests` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/guests/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/guests/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/guests/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/guests/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/housekeeping` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/housekeeping/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/housekeeping/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/housekeeping/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/housekeeping/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/import` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/import/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/import/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/import/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/import/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/minibar` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/minibar/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/minibar/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/minibar/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/minibar/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/night-audit` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/night-audit/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/night-audit/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/night-audit/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/night-audit/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/records/pms_booking_refund/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/records/pms_booking_refund/[id]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/records/pms_control_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/records/pms_control_sources/[id]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/records/pms_counter_refund/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/records/pms_counter_refund/[id]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/records/pms_snapshot_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/records/pms_snapshot_entry/[id]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/reports` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/reports/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/reports/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/reports/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/reports/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/reservations` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/reservations/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/reservations/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/reservations/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/reservations/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/rooms` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/rooms/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/rooms/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/rooms/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/rooms/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/settings` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/settings/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/settings/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/settings/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/settings/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/setup` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | page route |
| `/holdings/[holdingSlug]/pms/setup/records/pms_entry/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/setup/records/pms_hotel_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/setup/records/pms_hotel_sources/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pms/setup/records/pms_record/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pms_frontdesk | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pos/approvals` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | page route |
| `/holdings/[holdingSlug]/pos` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | page route |
| `/holdings/[holdingSlug]/pos/receipts/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pos/receipts/[id]` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pos/records/pos_report/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pos/records/pos_report/[id]` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pos/reports` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | page route |
| `/holdings/[holdingSlug]/pos/settings/initial-stock/[id]` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | sample record + invalid ID |
| `/holdings/[holdingSlug]/pos/settings/initial-stock` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | page route |
| `/holdings/[holdingSlug]/pos/settings` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | page route |
| `/holdings/[holdingSlug]/pos/settings/startup` | Holding workspace | login-or-explicit-denial | pos_cashier | holding | page route |
| `/holdings/[holdingSlug]/posts/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/posts` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/production/manufacturing/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/production/manufacturing/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/production/manufacturing` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/production/manufacturing/records/manufacturing_spec/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/production/manufacturing/records/manufacturing_spec/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/production/manufacturing/settings/initial-stock/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/production/manufacturing/settings/initial-stock` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/production/manufacturing/settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/production/manufacturing/settings/posting` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/production` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/production/planning` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/production/planning/records/planning_scenario/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/production/planning/records/planning_scenario/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/profiles/[userId]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/profiles` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines/operations/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/operations/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/operations` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines/projects/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/projects/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/projects/[id]/tasks/new` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/projects` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines/projects/settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines/projects/settings/workers/[personId]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines/projects/settings/workers` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines/routines/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/routines/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/routines` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines/tasks/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/tasks/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/projects-routines/tasks/new` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/projects-routines/tasks` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/purchasing/landed-costs/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/purchasing/landed-costs/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/purchasing/landed-costs` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/purchasing/orders/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/purchasing/orders/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/purchasing/orders` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/purchasing` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/purchasing/rfqs/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/purchasing/rfqs/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/purchasing/rfqs` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/purchasing/setup` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/purchasing/setup/posting` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/purchasing/supplier-bills/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/purchasing/supplier-bills/[id]` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/purchasing/supplier-bills` | Holding workspace | login-or-explicit-denial | finance_approver | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/purchasing/suppliers` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/record-details/[kind]/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/record-details` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/assets/accounting/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/assets/accounting` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/assets/maintenance/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/assets/maintenance/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/assets/maintenance` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/assets` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/assets/plans/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/assets/plans/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/assets/plans` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/assets/register/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/assets/register/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/assets/register` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/desk/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/desk/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/desk/new` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/desk` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/field-service/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/field-service/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/field-service` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/field-service/settings/initial-stock/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/field-service/settings/initial-stock` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/field-service/settings` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/field-service/settings/parts` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/citations/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/citations` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/compliance/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/compliance/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/compliance` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/contract/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/contract/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/contract` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/corporate/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/corporate/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/corporate` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/counsel/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/counsel/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/counsel` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/deadlines` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/ip/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/ip/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/ip` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/knowledge` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/laws` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/license/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/license/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/license` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/matter/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/matter/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/matter` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/mine` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/obligation/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/obligation/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/obligation` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/privacy/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/privacy/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/privacy` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/release` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/reports` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/request/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/request/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/request` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/rule/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/rule/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/rule` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/template/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/template/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/services/legal/template` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services/legal/thailand` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/services` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/tasks` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/tasks/people_payroll/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/tasks/people_payroll/[id]` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/tasks/people_payroll` | Holding workspace | login-or-explicit-denial | holding_owner | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/actions` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/all` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/approvals` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/completed` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/department-approvals/[kind]/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/work/department-approvals/[kind]/[id]` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/work/department-approvals` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/for-me` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/reports` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/requests` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/routing/[kind]/[id]/[recordSection]` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/work/routing/[kind]/[id]` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | sample record + invalid ID |
| `/holdings/[holdingSlug]/work/routing` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/team` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/tracking` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/holdings/[holdingSlug]/work/waiting` | Holding workspace | login-or-explicit-denial | employee | holding, branch, department, branch-department | page route |
| `/join` | Public and account | public | holding_owner | — | page route |
| `/legal-portal/[holdingSlug]` | Legal portal | login-or-explicit-denial | holding_owner | — | page route |
| `/legal-portal/enroll` | Legal portal | public | holding_owner | — | page route |
| `/login` | Public and account | public | holding_owner | — | page route |
| `/marketing` | Public and account | public | holding_owner | — | page route |
| `/my-access` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/my-profile` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/onboarding` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/product-entry` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/provider/commercial/packages` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/provider/commercial` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/provider` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/reset-password` | Public and account | public | holding_owner | — | page route |
| `/security` | Public and account | login-or-explicit-denial | holding_owner | — | page route |
| `/signup` | Public and account | public | holding_owner | — | page route |
| `/verify-email` | Public and account | public | holding_owner | — | page route |
| `/work-role` | Public and account | login-or-explicit-denial | holding_owner | — | page route |

## Acceptance boundary

This inventory covers the page files present in source at the fingerprint above. A route passes only when the expected login/permission/Coming Soon/not-found state is visible, the page has no server error, and mobile content does not overflow horizontally. Existing workflow specs remain the interaction coverage for create, review, validation, reload, and recovery behavior.
