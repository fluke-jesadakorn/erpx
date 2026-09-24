import { visibility as legalVisibility } from "../legal/access";
import "server-only";
import { hasPermission } from "@/lib/permissions";
import { hasModuleAccess } from "@/data/module-access";
import { metricIds, type Breakdown, type Filters, type MetricData, type MetricId, type MetricInfo } from "@/lib/executive/canvas";
import type { WorkContext } from "../authorization";
import { sha256 } from "../crypto";
import { ApiError } from "../http";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import type { SqlDatabase } from "../runtime/types";
import { runtimeEnv } from "../runtime";
import { customerTimelineEvents } from "../customer-timeline";
import { socialActivityEvents } from "../social-activity";
import { projectDependencyBlockerCohort, projectMilestoneOverdueCohort } from "./project-milestone-access";
import { participantSql } from "../approvals";
import { operationsMetrics, operationsTaskCohort, operationsTaskVisibility, type OperationsMetricId } from "./operations-access";
import { companyAuthority } from "../staff-policy";

type MetricAdapter = {
  moduleId: string;
  label: string;
  description: string;
  formula: string;
  unit: "money" | "count" | "percent" | "duration" | "quantity";
  dateBasis: string;
  permission: string;
  sourceTables: string[];
  sourceFields: string[];
  joins: string[];
  queryCost: "low" | "medium";
  freshness: "transactional";
  companyOnly?: boolean;
  departmentScopeUnsupported?: boolean;
  moduleGate?: string;
  requiredPermissions?: string[];
  holdingOwnerOnly?: boolean;
  acceptancePending?: boolean;
  betaScopeUnsupported?: boolean;
};

/** This catalog describes fixed, audited queries; models can select an ID but cannot supply SQL. */
const specs: Record<MetricId, MetricAdapter> = {
  "legal-records": {
    moduleId: "legal", label: "Legal records", description: "Records visible to the current reader, grouped by state.",
    formula: "Count visible legal items updated in the selected period, grouped by state.", unit: "count", dateBasis: "Last update", permission: "legal.read",
    sourceTables: ["legal_items"], sourceFields: ["id", "state", "updated_at"], joins: [], queryCost: "medium", freshness: "transactional",
  },
  "legal-contract-value": {
    moduleId: "legal", label: "Legal contract value", description: "Contract face values in the selected currency for records visible to the current reader. Not recognized revenue.",
    formula: "Sum recorded contract face values for visible legal items in the selected currency and update interval; not recognized revenue.", unit: "money", dateBasis: "Last update", permission: "legal.read",
    sourceTables: ["legal_items", "legal_contracts"], sourceFields: ["state", "updated_at", "data.value", "currency"], joins: ["legal_contracts.item_id = legal_items.id"], queryCost: "medium", freshness: "transactional",
  },
  "legal-exposure": {
    moduleId: "legal", label: "Legal case exposure", description: "Recorded case exposure in the selected currency for matters visible to the current reader. Not a booked provision.",
    formula: "Sum recorded matter exposure for visible legal items in the selected currency and update interval; not a booked provision.", unit: "money", dateBasis: "Last update", permission: "legal.read",
    sourceTables: ["legal_items", "legal_matters"], sourceFields: ["state", "updated_at", "data.exposure", "currency"], joins: ["legal_matters.item_id = legal_items.id"], queryCost: "medium", freshness: "transactional",
  },
  "active-legal-obligations": {
    moduleId: "legal", label: "Active legal obligations", description: "Nonterminal commitment schedule rows attached to visible, active Legal records.",
    formula: "Count visible legal_obligation_schedule rows whose schedule state is not completed or waived and whose parent legal record is not terminal. A due date is a deadline, not an effective interval.", unit: "count", dateBasis: "Current schedule and parent record state", permission: "legal.read",
    sourceTables: ["legal_items", "legal_obligation_schedule"], sourceFields: ["legal_items.company_id", "legal_items.branch_id", "legal_items.department_id", "legal_items.classification", "legal_items.kind", "legal_items.state", "legal_obligation_schedule.category", "legal_obligation_schedule.title", "legal_obligation_schedule.owner_id", "legal_obligation_schedule.due_date", "legal_obligation_schedule.state", "legal_obligation_schedule.version"], joins: ["legal_obligation_schedule.item_id = legal_items.id"], queryCost: "medium", freshness: "transactional", acceptancePending: true,
  },
  "active-legal-holds": {
    moduleId: "legal", label: "Active legal holds", description: "Active and release-requested legal holds attached to source records visible to the current reader.",
    formula: "Count distinct legal hold records in active or release_requested state whose source legal item passes the current Legal visibility rule; related target links are not counted as additional holds.", unit: "count", dateBasis: "Current hold state", permission: "legal.read",
    sourceTables: ["legal_holds", "legal_items"], sourceFields: ["legal_holds.id", "legal_holds.item_id", "legal_holds.state", "legal_holds.created_at", "legal_items.company_id", "legal_items.branch_id", "legal_items.department_id", "legal_items.classification", "legal_items.kind", "legal_items.version"], joins: ["legal_holds.item_id = legal_items.id"], queryCost: "medium", freshness: "transactional", acceptancePending: true,
  },
  "active-subscriptions": {
    moduleId: "commercial", label: "Active subscriptions", description: "Current Holding billing enrollments and active legacy Stripe subscription snapshots.",
    formula: "Count current Holding billing_enrollments started by the reference time and without ended_at (a scheduled ending remains active only until effective_at), plus beta_billing snapshots with a subscription ID, active or trialing status, and no expired current period.", unit: "count", dateBasis: "Current subscription state and effective interval (UTC)", permission: "company.owner",
    sourceTables: ["billing_accounts", "billing_enrollments", "billing_plans", "beta_billing"], sourceFields: ["holding_id", "version", "plan_id", "started_at", "ended_at", "pending_end", "effective_at", "revision", "name", "subscription_id", "status", "current_period_end", "last_event_at", "updated_at"], joins: ["billing_enrollments.holding_id = billing_accounts.holding_id", "billing_enrollments.plan_id = billing_plans.id", "beta_billing.holding_id = billing_accounts.holding_id"], queryCost: "low", freshness: "transactional", holdingOwnerOnly: true, acceptancePending: true,
  },
  "overdue-invoices": {
    moduleId: "commercial", label: "Overdue subscription invoices", description: "Unpaid issued Holding invoices whose due date has passed. The local billing source records invoice state, not a partial remaining balance.",
    formula: "Count current Holding billing_invoices with status open or uncollectible, due_at before the reference time, and due_at within the selected UTC interval; draft, paid and void invoices are excluded.", unit: "count", dateBasis: "Due date and current invoice state (UTC)", permission: "company.owner",
    sourceTables: ["billing_accounts", "billing_invoices"], sourceFields: ["holding_id", "status", "due_at", "updated_at", "kind"], joins: ["billing_invoices.holding_id = billing_accounts.holding_id"], queryCost: "low", freshness: "transactional", holdingOwnerOnly: true, acceptancePending: true,
  },
  receivables: {
    moduleId: "finance", label: "Open receivables", description: "Current outstanding amount on posted and partially paid receivable invoices, grouped by status. No draft or void invoices.",
    formula: "Sum outstanding_minor for posted and partially paid receivable invoices in the selected currency and invoice-date range.", unit: "money", dateBasis: "Invoice date; current balance", permission: "finance.read",
    sourceTables: ["finance_invoices"], sourceFields: ["kind", "currency", "invoice_date", "status", "outstanding_minor"], joins: [], queryCost: "low", freshness: "transactional", companyOnly: true,
  },
  payables: {
    moduleId: "finance", label: "Open payables", description: "Current outstanding amount on posted and partially paid payable invoices, grouped by status.",
    formula: "Sum outstanding_minor for posted and partially paid payable invoices in the selected currency and invoice-date range.", unit: "money", dateBasis: "Invoice date; current balance", permission: "finance.read",
    sourceTables: ["finance_invoices"], sourceFields: ["kind", "currency", "invoice_date", "status", "outstanding_minor"], joins: [], queryCost: "low", freshness: "transactional", companyOnly: true,
  },
  "invoiced-sales": {
    moduleId: "finance", label: "Invoiced sales", description: "Posted, partially paid and paid receivable invoice totals including tax, grouped by invoice month. This is invoiced value, not recognized revenue or cash collected.",
    formula: "Sum total_minor including tax for posted, partially paid and paid receivable invoices in the selected currency and invoice-date range.", unit: "money", dateBasis: "Invoice date", permission: "finance.read",
    sourceTables: ["finance_invoices"], sourceFields: ["kind", "currency", "invoice_date", "status", "total_minor"], joins: [], queryCost: "low", freshness: "transactional", companyOnly: true,
  },
  "cash-receipts": {
    moduleId: "finance", label: "Posted cash receipts", description: "Posted customer receipts in the selected currency. Voided, unposted and reversed receipts are excluded.",
    formula: "Sum finance_payments.amount_minor for receipt payments linked to a posted journal whose posted_at is in the selected UTC interval, excluding payment IDs with an immutable reversal.", unit: "money", dateBasis: "Journal posting time (UTC)", permission: "finance.read",
    sourceTables: ["finance_payments", "finance_journals", "finance_payment_reversals"], sourceFields: ["finance_payments.kind", "finance_payments.amount_minor", "finance_payments.currency", "finance_payments.journal_id", "finance_journals.status", "finance_journals.posted_at", "finance_payment_reversals.payment_id"], joins: ["finance_payments.journal_id = finance_journals.id"], queryCost: "medium", freshness: "transactional", companyOnly: true, acceptancePending: true,
  },
  "close-blockers": {
    moduleId: "finance", label: "Open close blockers", description: "Latest unresolved blocking finance checks for periods in the selected interval.",
    formula: "Count the latest finance_close_checks row per accounting period and check key when its status is block, it is unresolved, and its accounting period is still open or closing.", unit: "count", dateBasis: "Accounting period end date; latest check", permission: "finance.read",
    sourceTables: ["finance_close_checks", "finance_periods"], sourceFields: ["finance_close_checks.period_id", "finance_close_checks.check_key", "finance_close_checks.status", "finance_close_checks.resolved_at", "finance_close_checks.checked_at", "finance_periods.end_date", "finance_periods.state"], joins: ["finance_close_checks.period_id = finance_periods.id"], queryCost: "medium", freshness: "transactional", companyOnly: true, acceptancePending: true,
  },
  "sales-orders": {
    moduleId: "sales", label: "Confirmed sales orders", description: "Confirmed, partially fulfilled and fulfilled sales order value including tax, grouped by order month.",
    formula: "Sum subtotal_minor plus tax_minor for confirmed, partially fulfilled and fulfilled sales orders in the selected currency and order-date range.", unit: "money", dateBasis: "Order date", permission: "commerce.read",
    sourceTables: ["commerce_orders", "organization_nodes"], sourceFields: ["kind", "currency", "order_date", "state", "subtotal_minor", "tax_minor", "branch_id", "department_id"], joins: ["commerce_orders.branch_id = organization_nodes.id"], queryCost: "medium", freshness: "transactional",
  },
  "sales-fulfillment-rate": {
    moduleId: "sales", label: "Sales order fulfillment rate", description: "Share of eligible confirmed sales orders that are currently fully fulfilled. Each order counts once regardless of product unit.",
    formula: "Count eligible confirmed sales orders currently in fulfilled state divided by eligible confirmed, partially fulfilled and fulfilled sales orders; include only orders with at least one line. Each order counts once regardless of product unit.", unit: "percent", dateBasis: "Order date (eligible order cohort)", permission: "commerce.read",
    sourceTables: ["commerce_orders", "commerce_order_lines", "organization_nodes"], sourceFields: ["commerce_orders.company_id", "commerce_orders.branch_id", "commerce_orders.department_id", "commerce_orders.kind", "commerce_orders.order_date", "commerce_orders.state", "commerce_orders.version", "commerce_order_lines.order_id"], joins: ["commerce_order_lines.order_id = commerce_orders.id", "commerce_orders.branch_id = organization_nodes.id"], queryCost: "medium", freshness: "transactional", moduleGate: "sales", acceptancePending: true,
  },
  "sales-cancellations": {
    moduleId: "sales", label: "Sales order cancellations", description: "Sales orders in the terminal cancelled state, dated by the state transition's updated_at value.",
    formula: "Count sales orders whose current state is cancelled and whose terminal updated_at timestamp falls in the selected UTC date interval. Cancellation is terminal in the commerce workflow.", unit: "count", dateBasis: "Cancellation time (UTC; terminal cancelled row updated_at)", permission: "commerce.read",
    sourceTables: ["commerce_orders", "organization_nodes"], sourceFields: ["company_id", "kind", "state", "updated_at", "branch_id", "department_id"], joins: ["commerce_orders.branch_id = organization_nodes.id"], queryCost: "medium", freshness: "transactional", moduleGate: "sales", acceptancePending: true,
  },
  "sales-cancellation-value": {
    moduleId: "sales", label: "Confirmed sales cancellation value", description: "Value including tax for previously approved sales orders that are now cancelled, in the selected currency.",
    formula: "Sum subtotal_minor plus tax_minor for cancelled sales orders with approved_by set, in the selected currency and UTC cancellation interval. Draft cancellations are excluded; this is order value, not lost revenue.", unit: "money", dateBasis: "Cancellation time (UTC; terminal cancelled row updated_at)", permission: "commerce.read",
    sourceTables: ["commerce_orders", "organization_nodes"], sourceFields: ["company_id", "kind", "state", "approved_by", "updated_at", "currency", "subtotal_minor", "tax_minor", "branch_id", "department_id"], joins: ["commerce_orders.branch_id = organization_nodes.id"], queryCost: "medium", freshness: "transactional", moduleGate: "sales", acceptancePending: true,
  },
  "purchase-orders": {
    moduleId: "procurement", label: "Confirmed purchase orders", description: "Confirmed, partially fulfilled and fulfilled purchase order value including tax, grouped by order month.",
    formula: "Sum subtotal_minor plus tax_minor for confirmed, partially fulfilled and fulfilled purchase orders in the selected currency and order-date range.", unit: "money", dateBasis: "Order date", permission: "commerce.read",
    sourceTables: ["commerce_orders", "organization_nodes"], sourceFields: ["kind", "currency", "order_date", "state", "subtotal_minor", "tax_minor", "branch_id", "department_id"], joins: ["commerce_orders.branch_id = organization_nodes.id"], queryCost: "medium", freshness: "transactional",
  },
  "open-po-receipts": {
    moduleId: "procurement", label: "Purchase orders awaiting receipt", description: "Confirmed purchase orders with at least one ordered quantity that has not yet been received, grouped by due month, state or Branch.",
    formula: "Count distinct confirmed or partially fulfilled purchase orders due in the selected interval that have at least one order line where fulfilled_milli is less than quantity_milli. Received quantities are taken from completed goods receipts.", unit: "count", dateBasis: "Order due date; current received quantity", permission: "commerce.read",
    sourceTables: ["commerce_orders", "commerce_order_lines"], sourceFields: ["commerce_orders.company_id", "commerce_orders.kind", "commerce_orders.state", "commerce_orders.due_date", "commerce_orders.branch_id", "commerce_orders.department_id", "commerce_order_lines.quantity_milli", "commerce_order_lines.fulfilled_milli"], joins: ["commerce_order_lines.order_id = commerce_orders.id"], queryCost: "medium", freshness: "transactional", moduleGate: "procurement", acceptancePending: true,
  },
  "invoice-match-exceptions": {
    moduleId: "procurement", label: "Supplier invoice match exceptions", description: "Returned supplier invoice matches without a replacement active match, and unapproved quantity or subtotal variances within the reader's authorized Company scope.",
    formula: "Count current supplier invoice matches whose status is returned and whose receipt links have no replacement active match, or whose status is review with a quantity mismatch or subtotal variance. Count each match row once, grouped by invoice month, exception type or Branch.", unit: "count", dateBasis: "Supplier invoice date; current match state", permission: "finance.read",
    sourceTables: ["procurement_bill_matches", "procurement_bill_match_receipts"], sourceFields: ["company_id", "branch_id", "department_id", "invoice_date", "status", "quantity_matches", "expected_subtotal_minor", "subtotal_minor", "match_id", "fulfillment_id", "active"], joins: ["procurement_bill_matches.id = procurement_bill_match_receipts.match_id", "procurement_bill_match_receipts.fulfillment_id = replacement receipt links"], queryCost: "medium", freshness: "transactional", moduleGate: "procurement", acceptancePending: true,
  },
  headcount: {
    moduleId: "people_payroll", label: "Active headcount", description: "Distinct active people with a currently effective active engagement, counted once across engagements in the selected scope.",
    formula: "Count distinct active people with an effective active engagement today; latest start date then engagement ID selects one breakdown group.", unit: "count", dateBasis: "Current effective date; date filter does not apply", permission: "people.read",
    sourceTables: ["hr_people", "hr_engagements", "organization_nodes", "staff_levels"], sourceFields: ["status", "merged_into_id", "person_id", "start_date", "end_date", "branch_id", "department_id", "level_id"], joins: ["hr_engagements.person_id = hr_people.id", "hr_engagements.branch_id = organization_nodes.id", "hr_engagements.level_id = staff_levels.id"], queryCost: "medium", freshness: "transactional",
  },
  "approved-leave-duration": {
    moduleId: "people_payroll", label: "Approved leave duration", description: "Saved leave-day units for approved requests the current reader is permitted to open. Employee identity, leave type and handover notes are excluded.",
    formula: "Sum calculated_units (or the recorded legacy days when calculated_units is absent) for approved leave requests visible to the current reader, grouped by request start month, Branch or Department. Requests are dated by their start date; units remain leave days.", unit: "quantity", dateBasis: "Request start date", permission: "people.read",
    sourceTables: ["leave_requests", "approval_instances", "approval_steps", "organization_nodes"], sourceFields: ["leave_requests.id", "leave_requests.number", "leave_requests.company_id", "leave_requests.branch_id", "leave_requests.department_id", "leave_requests.start_date", "leave_requests.end_date", "leave_requests.calculated_units", "leave_requests.days", "leave_requests.state", "leave_requests.version", "leave_requests.created_by", "leave_requests.decided_by", "approval_instances.record_type", "approval_steps.approver_user_id"], joins: ["record-level visibility via leave requester, decision maker or current leave/cancellation approver"], queryCost: "medium", freshness: "transactional", requiredPermissions: ["module.people_payroll.read"], acceptancePending: true,
  },
  "authorized-overtime-duration": {
    moduleId: "people_payroll", label: "Authorized overtime duration", description: "Final, immutable overtime minutes handed to Payroll for approved requests visible to the current reader. Reasons and employee-level pay are excluded.",
    formula: "Sum overtime_payroll_handoffs.minutes converted to exact milliseconds by local work date, for approved overtime requests visible to the current reader. Planned, unapproved and withdrawn requests are excluded.", unit: "duration", dateBasis: "Local work date on the approved Payroll handoff", permission: "people.read",
    sourceTables: ["overtime_payroll_handoffs", "overtime_requests", "approval_instances", "approval_steps", "organization_nodes"], sourceFields: ["overtime_payroll_handoffs.request_id", "overtime_payroll_handoffs.company_id", "overtime_payroll_handoffs.work_date", "overtime_payroll_handoffs.minutes", "overtime_payroll_handoffs.request_version", "overtime_requests.branch_id", "overtime_requests.department_id", "overtime_requests.created_by", "overtime_requests.state", "overtime_requests.version", "approval_instances.record_type", "approval_steps.approver_user_id"], joins: ["overtime_payroll_handoffs.request_id = overtime_requests.id", "record-level visibility via overtime requester or current overtime approver"], queryCost: "medium", freshness: "transactional", requiredPermissions: ["module.people_payroll.read"], acceptancePending: true,
  },
  "payroll-summary": {
    moduleId: "people_payroll", label: "Posted net payroll", description: "Net pay totals from posted payroll runs with a posted Finance journal, limited to the selected Company and currency. Employee-level rows are not aggregated into the report.",
    formula: "Sum net_minor from posted payroll items for payroll runs whose linked Finance journal is posted, grouped by run period end month or as one Company total. Currency is selected explicitly; Branch and Department scopes are unsupported.", unit: "money", dateBasis: "Payroll period end date", permission: "payroll.read",
    sourceTables: ["hr_payroll_runs", "hr_payroll_items", "hr_payroll_postings", "finance_journals"], sourceFields: ["hr_payroll_runs.company_id", "hr_payroll_runs.period_start", "hr_payroll_runs.period_end", "hr_payroll_runs.pay_date", "hr_payroll_runs.currency", "hr_payroll_runs.status", "hr_payroll_runs.version", "hr_payroll_items.net_minor", "hr_payroll_items.status", "finance_journals.status"], joins: ["hr_payroll_items.run_id = hr_payroll_runs.id", "hr_payroll_postings.run_id = hr_payroll_runs.id", "hr_payroll_postings.journal_id = finance_journals.id"], queryCost: "medium", freshness: "transactional", companyOnly: true, requiredPermissions: ["module.people_payroll.read"], acceptancePending: true,
  },
  "my-tasks": {
    moduleId: "collaboration", label: "My open tasks", description: "Tasks assigned to the current reader, excluding drafts, completed and cancelled tasks, grouped by state.",
    formula: "Count tasks assigned to the current reader, due in the selected interval, excluding drafts, completed and cancelled tasks.", unit: "count", dateBasis: "Due date (UTC)", permission: "self.tasks",
    sourceTables: ["staff_tasks"], sourceFields: ["assignee_user_id", "state", "due_at", "branch_id", "department_id"], joins: [], queryCost: "low", freshness: "transactional",
  },
  "open-operations-exceptions": {
    moduleId: "operations", label: "Open operational exceptions", description: "Overdue, unfinished tasks visible through the current reader's staff-task relationships.",
    formula: "Count staff tasks visible to the current reader that are past due and not completed or cancelled at the reference time. A task enters this exception cohort when its due_at passes; date filters do not change this current-state count.", unit: "count", dateBasis: "Current task state and due time (UTC)", permission: "records.read",
    sourceTables: ["staff_tasks", "staff_task_delegations", "staff_projects", "staff_project_members", "approval_instances", "approval_steps", "social_task_origins", "social_posts"], sourceFields: ["staff_tasks.title", "staff_tasks.state", "staff_tasks.due_at", "staff_tasks.branch_id", "staff_tasks.department_id", "staff_tasks.version", "staff_task_delegations.status", "staff_projects.owner_user_id", "staff_project_members.user_id", "approval_steps.approver_user_id", "social_posts.state"], joins: ["staff task relationship authorization", "conversation task source-post visibility"], queryCost: "medium", freshness: "transactional", moduleGate: "operations", acceptancePending: true,
  },
  "exception-resolution-time": {
    moduleId: "operations", label: "Overdue task resolution time", description: "Median time past the task due time before a visible task is completed.",
    formula: "Median milliseconds from due_at to the terminal completed-row updated_at among staff tasks visible to the current reader, completed after their due time, with completion time inside the selected UTC interval. This measures overdue time, not total task lifetime.", unit: "duration", dateBasis: "Completion time (UTC); interval begins at task due time", permission: "records.read",
    sourceTables: ["staff_tasks", "staff_task_delegations", "staff_projects", "staff_project_members", "approval_instances", "approval_steps", "social_task_origins", "social_posts"], sourceFields: ["staff_tasks.title", "staff_tasks.state", "staff_tasks.due_at", "staff_tasks.updated_at", "staff_tasks.branch_id", "staff_tasks.department_id", "staff_tasks.version", "staff_task_delegations.status", "staff_projects.owner_user_id", "staff_project_members.user_id", "approval_steps.approver_user_id", "social_posts.state"], joins: ["staff task relationship authorization", "conversation task source-post visibility"], queryCost: "medium", freshness: "transactional", moduleGate: "operations", acceptancePending: true,
  },
  "operations-escalations": {
    moduleId: "operations", label: "Overdue task escalations", description: "Current overdue tasks with a durable overdue alert for the current reader.",
    formula: "Count staff tasks visible to the current reader that remain past due and unfinished and have a durable staff_task_alert notification for that reader on the task's current version. The alert is the scheduled Operations escalation; age is measured from due_at.", unit: "count", dateBasis: "Current task due time, state, version and alert state (UTC)", permission: "records.read",
    sourceTables: ["staff_tasks", "staff_notifications", "staff_task_delegations", "staff_projects", "staff_project_members", "approval_instances", "approval_steps", "social_task_origins", "social_posts"], sourceFields: ["staff_tasks.title", "staff_tasks.state", "staff_tasks.due_at", "staff_tasks.version", "staff_tasks.branch_id", "staff_tasks.department_id", "staff_notifications.user_id", "staff_notifications.record_type", "staff_notifications.record_version", "staff_task_delegations.status", "staff_projects.owner_user_id", "staff_project_members.user_id", "approval_steps.approver_user_id", "social_posts.state"], joins: ["staff task relationship authorization", "current-version overdue alert", "conversation task source-post visibility"], queryCost: "medium", freshness: "transactional", moduleGate: "operations", acceptancePending: true,
  },
  "collaboration-thread-activity": {
    moduleId: "collaboration", label: "Conversation replies and mentions", description: "Published, role-visible social thread replies and mentions. Message bodies and mention targets are never included.",
    formula: "Count published comments and person/record mention rows attached to a post that is visible in the current social feed scope, by UTC creation month or activity type.", unit: "count", dateBasis: "Reply or mention creation time (UTC)", permission: "records.read",
    sourceTables: ["social_posts", "social_comments", "social_mentions", "assignments"], sourceFields: ["social_posts.company_id", "social_posts.branch_id", "social_posts.department_id", "social_posts.state", "social_comments.state", "social_comments.created_at", "social_mentions.owner_type", "social_mentions.owner_id", "social_mentions.target_kind", "social_mentions.created_at"], joins: ["social_comments.post_id = social_posts.id", "social_mentions.owner_id = visible social_posts or comments"], queryCost: "medium", freshness: "transactional", moduleGate: "collaboration", acceptancePending: true,
  },
  inventory: {
    moduleId: "inventory", label: "Stocked SKU locations", description: "Product and warehouse pairs with positive on-hand stock, grouped by warehouse. Counts locations, not mixed-unit quantities or inventory value.",
    formula: "Count product and warehouse pairs with positive on-hand stock; do not add quantities with different units.", unit: "count", dateBasis: "Current stock; date filter does not apply", permission: "commerce.read",
    sourceTables: ["commerce_stock", "commerce_warehouses", "commerce_products"], sourceFields: ["commerce_stock.quantity_milli", "commerce_stock.warehouse_id", "commerce_stock.product_id", "commerce_stock.version", "commerce_warehouses.company_id", "commerce_warehouses.name", "commerce_products.sku", "commerce_products.name", "commerce_products.unit"], joins: ["commerce_stock.warehouse_id = commerce_warehouses.id", "commerce_stock.product_id = commerce_products.id"], queryCost: "medium", freshness: "transactional", companyOnly: true, moduleGate: "inventory", acceptancePending: true,
  },
  "available-stock": {
    moduleId: "inventory", label: "Available stock by product unit", description: "Current available quantity by product and warehouse. Product units remain separate and the result has no cross-unit total.",
    formula: "For each active product and warehouse, subtract the larger of active reservations or quarantined quantity from on-hand stock, with a zero floor. Quantities are expressed in thousandths of each product's recorded unit and are never added across units.", unit: "quantity", dateBasis: "Current stock, active reservations and quality quarantine; date filter does not apply", permission: "commerce.read",
    sourceTables: ["commerce_stock", "commerce_products", "commerce_warehouses", "pos_reservations", "inventory_order_reservations", "field_service_part_reservations", "manufacturing_material_reservations", "inventory_quality_holds", "inventory_lots", "inventory_lot_balances"], sourceFields: ["commerce_stock.quantity_milli", "commerce_stock.version", "commerce_products.sku", "commerce_products.name", "commerce_products.unit", "commerce_products.status", "commerce_warehouses.name", "commerce_warehouses.branch_id", "inventory_active_reservations.quantity_milli", "inventory_quality_holds.status", "inventory_quality_holds.lot_id", "inventory_lot_balances.quantity_milli"], joins: ["commerce_stock.warehouse_id = commerce_warehouses.id", "commerce_stock.product_id = commerce_products.id", "inventory_active_reservations grouped by warehouse and product", "inventory_quarantined_quantity(warehouse_id, product_id)"], queryCost: "medium", freshness: "transactional", moduleGate: "inventory", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "inventory-shortages": {
    moduleId: "inventory", label: "Inventory shortages", description: "Active product and warehouse pairs whose current available quantity is below the current independently approved reorder point. Proposed points do not affect this metric.",
    formula: "Count each active product and warehouse pair once when available quantity, computed as the positive floor of on-hand less the larger of active reservations or quality quarantine, is below its current approved reorder point. A proposed point leaves the prior approved value active until approval; a retired point is excluded. Quantities remain in each product's unit and are never summed.", unit: "count", dateBasis: "Current stock, reservations, quarantine and approved reorder policy; date filter does not apply", permission: "commerce.read",
    sourceTables: ["inventory_reorder_policies", "commerce_stock", "commerce_products", "commerce_warehouses", "pos_reservations", "inventory_order_reservations", "field_service_part_reservations", "manufacturing_material_reservations", "inventory_quality_holds", "inventory_lots", "inventory_lot_balances"], sourceFields: ["inventory_reorder_policies.current_reorder_point_milli", "inventory_reorder_policies.proposed_reorder_point_milli", "inventory_reorder_policies.retired_at", "inventory_reorder_policies.version", "commerce_stock.quantity_milli", "commerce_stock.version", "commerce_products.sku", "commerce_products.name", "commerce_products.unit", "commerce_products.status", "commerce_warehouses.name", "commerce_warehouses.branch_id", "inventory_active_reservations.quantity_milli", "inventory_quality_holds.status", "inventory_lot_balances.quantity_milli"], joins: ["inventory_reorder_policies.warehouse_id = commerce_warehouses.id", "inventory_reorder_policies.product_id = commerce_products.id", "inventory_active_reservations grouped by warehouse and product", "inventory_quarantined_quantity(warehouse_id, product_id)"], queryCost: "medium", freshness: "transactional", moduleGate: "inventory", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "inventory-movements": {
    moduleId: "inventory", label: "Inventory movements", description: "Committed, signed inventory ledger movements grouped by date, source type, warehouse or product unit. Quantities remain separate for each product unit.",
    formula: "Sum signed quantity_milli from non-zero inventory_ledger events whose recorded movement_date falls within the selected UTC date range. Group by month by default; optional groups use source type, warehouse or product unit, always preserving product unit. Receipts and issues retain their signs, returns and reversals remain separate source events, and paired transfer legs offset only when grouped across their Company scope.", unit: "quantity", dateBasis: "Movement date recorded by the source event (UTC day)", permission: "commerce.read",
    sourceTables: ["commerce_products", "commerce_warehouses", "commerce_stock_moves", "commerce_fulfillments", "commerce_orders", "inventory_adjustment_moves", "inventory_counts", "inventory_transfer_moves", "inventory_transfers", "pos_stock_moves", "pms_stock_moves", "commerce_return_stock_moves", "commerce_returns", "inventory_transit_events", "inventory_transit", "inventory_landed_cost_moves", "inventory_landed_costs"],
    sourceFields: ["inventory_ledger.id", "inventory_ledger.movement_date", "inventory_ledger.source_type", "inventory_ledger.quantity_milli", "inventory_ledger.source_number", "commerce_products.sku", "commerce_products.name", "commerce_products.unit", "commerce_warehouses.name", "commerce_warehouses.branch_id"],
    joins: ["inventory_ledger.warehouse_id = commerce_warehouses.id", "inventory_ledger.product_id = commerce_products.id"], queryCost: "medium", freshness: "transactional", moduleGate: "inventory", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "pos-sales": {
    moduleId: "pos", label: "Completed POS sales", description: "Completed sale totals including tax and discounts, grouped by business date month. Gross completed sales before subsequent refunds.",
    formula: "Sum total_minor for completed POS sales by business date; subsequent refunds are not netted.", unit: "money", dateBasis: "POS business date", permission: "pos.reports",
    sourceTables: ["pos_sales"], sourceFields: ["id", "number", "branch_id", "currency", "business_date", "state", "total_minor", "version"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "pos", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "net-pos-sales": {
    moduleId: "pos", label: "Net POS sales after refunds", description: "Completed POS sales less completed sale refunds in their accounting interval. Advance refunds and unconfirmed payouts are excluded.",
    formula: "Sum total_minor for completed POS sales whose business date is in range, less amount_minor for completed sale refunds whose accounting date is in range, in the same currency and branch scope. Refund amounts include tax.", unit: "money", dateBasis: "Sale business date and refund accounting date", permission: "pos.reports",
    sourceTables: ["pos_sales", "pos_refunds"], sourceFields: ["pos_sales.total_minor", "pos_sales.business_date", "pos_sales.state", "pos_refunds.amount_minor", "pos_refunds.accounting_date", "pos_refunds.kind", "pos_refunds.state"], joins: ["pos_refunds.sale_id = pos_sales.id"], queryCost: "medium", freshness: "transactional", moduleGate: "pos", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "pos-payment-mix": {
    moduleId: "pos", label: "POS payment mix", description: "Successful tender amounts for completed sales, net of refunds that have actually been paid out.",
    formula: "Sum successful pos_payments.amount_minor less pos_refund_payments.amount_minor in succeeded state, grouped by payment method, POS business month or branch for completed sales in the selected interval.", unit: "money", dateBasis: "POS business date of the original sale", permission: "pos.reports",
    sourceTables: ["pos_sales", "pos_payments", "pos_refunds", "pos_refund_payments"], sourceFields: ["pos_sales.business_date", "pos_sales.state", "pos_sales.currency", "pos_payments.method", "pos_payments.amount_minor", "pos_payments.state", "pos_refunds.kind", "pos_refund_payments.amount_minor", "pos_refund_payments.state"], joins: ["pos_payments.sale_id = pos_sales.id", "pos_refund_payments.payment_id = pos_payments.id", "pos_refund_payments.refund_id = pos_refunds.id"], queryCost: "medium", freshness: "transactional", moduleGate: "pos", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "hotel-rooms": {
    moduleId: "pms", label: "Configured hotel rooms", description: "Room records grouped by property. This is configured room count, not occupancy or sellable inventory.",
    formula: "Count configured room records by property; this is not occupancy, sellable inventory or room revenue.", unit: "count", dateBasis: "Current configuration; date filter does not apply", permission: "pms.read",
    sourceTables: ["pms_records", "pms_properties"], sourceFields: ["kind", "property_id", "name", "company_id", "branch_id"], joins: ["pms_records.property_id = pms_properties.id"], queryCost: "low", freshness: "transactional", betaScopeUnsupported: true,
  },
  "hotel-occupancy": {
    moduleId: "pms", label: "Hotel occupancy", description: "Weighted occupied room-night rate from completed PMS Night Audit snapshots, limited to the selected Company and Branch scope.",
    formula: "Sum sold room nights divided by sum sellable room capacity nights across completed PMS Night Audit snapshots in the selected business-date interval. Night Audit stores the same sold and capacity values used by Daily Performance; open, blocked and malformed snapshots are excluded, with malformed completed snapshots marking the result partial.", unit: "percent", dateBasis: "PMS property business date", permission: "pms.read",
    sourceTables: ["pms_records", "pms_properties"], sourceFields: ["pms_records.kind", "pms_records.data_json.state", "pms_records.data_json.date", "pms_records.data_json.roomMetrics.capacity", "pms_records.data_json.roomMetrics.sold", "pms_records.version", "pms_properties.company_id", "pms_properties.branch_id", "pms_properties.name"], joins: ["pms_records.property_id = pms_properties.id"], queryCost: "medium", freshness: "transactional", requiredPermissions: ["pms.reports"], departmentScopeUnsupported: true, betaScopeUnsupported: true, acceptancePending: true,
  },
  "hotel-adr": {
    moduleId: "pms", label: "Hotel average daily rate (ADR)", description: "Weighted average net room revenue per sold room night from completed Night Audit dates in the selected property currency.",
    formula: "For completed Night Audit dates in the selected currency and business-date interval, sum net room revenue from PMS charge and adjustment entries whose original-entry chain reaches a night room charge, then divide by sold room nights. Group and overall values are weighted by sold room nights. Dates or groups with zero sold room nights have no ADR result; malformed completed room metrics or linked room revenue mark the result partial.", unit: "money", dateBasis: "Completed PMS property business date", permission: "pms.read",
    sourceTables: ["pms_records", "pms_properties"], sourceFields: ["pms_records.kind", "pms_records.data_json.date", "pms_records.data_json.kind", "pms_records.data_json.net", "pms_records.data_json.sourceKey", "pms_records.data_json.originalId", "pms_records.data_json.roomMetrics.sold", "pms_records.data_json.roomMetrics.capacity", "pms_records.version", "pms_properties.company_id", "pms_properties.branch_id", "pms_properties.name", "pms_properties.config_json.currency"], joins: ["PMS entry originalId chain", "pms_records.property_id = pms_properties.id"], queryCost: "medium", freshness: "transactional", requiredPermissions: ["pms.reports"], departmentScopeUnsupported: true, betaScopeUnsupported: true, acceptancePending: true,
  },
  "hotel-revpar": {
    moduleId: "pms", label: "Hotel revenue per available room (RevPAR)", description: "Weighted net room revenue per sellable capacity night from completed Night Audit dates in the selected property currency.",
    formula: "For completed Night Audit dates in the selected currency and business-date interval, sum net room revenue from PMS charge and adjustment entries whose original-entry chain reaches a night room charge, then divide by sellable room capacity nights. Group and overall values are weighted by sellable capacity nights. Malformed completed room metrics or linked room revenue mark the result partial.", unit: "money", dateBasis: "Completed PMS property business date", permission: "pms.read",
    sourceTables: ["pms_records", "pms_properties"], sourceFields: ["pms_records.kind", "pms_records.data_json.date", "pms_records.data_json.kind", "pms_records.data_json.net", "pms_records.data_json.sourceKey", "pms_records.data_json.originalId", "pms_records.data_json.roomMetrics.sold", "pms_records.data_json.roomMetrics.capacity", "pms_records.version", "pms_properties.company_id", "pms_properties.branch_id", "pms_properties.name", "pms_properties.config_json.currency"], joins: ["PMS entry originalId chain", "pms_records.property_id = pms_properties.id"], queryCost: "medium", freshness: "transactional", requiredPermissions: ["pms.reports"], departmentScopeUnsupported: true, betaScopeUnsupported: true, acceptancePending: true,
  },
  "hotel-folio-balance": {
    moduleId: "pms", label: "Open folio balances", description: "Current posted PMS receivable balances for non-closed folios, grouped by property or branch. The date range does not apply and currencies are never combined.",
    formula: "Sum posted finance journal debit less credit lines on each property's configured receivable account, linked through PMS postings to folio entry records for non-closed folios. Draft, in-review, reversed and other non-posted journals are excluded. Apply the selected currency; the date filter does not apply to this current balance.", unit: "money", dateBasis: "Current posted PMS receivable balance", permission: "pms.read",
    sourceTables: ["pms_properties", "pms_records", "pms_postings", "finance_journals", "finance_journal_lines"], sourceFields: ["pms_properties.company_id", "pms_properties.branch_id", "pms_properties.config_json.currency", "pms_properties.config_json.accounts.receivable", "pms_records.kind", "pms_records.data_json.folioId", "pms_records.data_json.state", "pms_records.version", "pms_postings.record_id", "finance_journals.status", "finance_journal_lines.account_id", "finance_journal_lines.debit_minor", "finance_journal_lines.credit_minor"], joins: ["pms_records property_id to pms_properties", "pms_postings record_id to posted PMS entry journal", "finance_journal_lines.account_id to the property's configured receivable account"], queryCost: "medium", freshness: "transactional", requiredPermissions: ["pms.reports"], departmentScopeUnsupported: true, betaScopeUnsupported: true, acceptancePending: true,
  },
  "hotel-night-audit-blockers": {
    moduleId: "pms", label: "Night audit blockers", description: "Blocked PMS Night Audit jobs by property business date and stored system error code.",
    formula: "Count pms_jobs rows in blocked state for the selected Company, Branch and business-date interval. This covers persisted blocked audit jobs; pre-start readiness checks are not stored as records and remain visible in PMS Daily Controls.", unit: "count", dateBasis: "PMS property business date; current job state", permission: "pms.read",
    sourceTables: ["pms_jobs", "pms_properties", "pms_records"], sourceFields: ["pms_jobs.property_id", "pms_jobs.business_date", "pms_jobs.state", "pms_jobs.error_code", "pms_jobs.updated_at", "pms_properties.company_id", "pms_properties.branch_id", "pms_properties.name", "pms_records.kind", "pms_records.data_json.date", "pms_records.data_json.state", "pms_records.version"], joins: ["pms_jobs.property_id = pms_properties.id", "pms_jobs.id = pms_records.id; persisted Night Audit state and version are authoritative for refresh"], queryCost: "low", freshness: "transactional", requiredPermissions: ["pms.reports"], departmentScopeUnsupported: true, betaScopeUnsupported: true, acceptancePending: true,
  },
  projects: {
    moduleId: "projects", label: "My projects", description: "Projects owned by or explicitly shared with the current reader, grouped by creation month.",
    formula: "Count projects owned by or explicitly shared with the current reader and created in the selected interval.", unit: "count", dateBasis: "Created date (UTC)", permission: "records.read",
    sourceTables: ["staff_projects", "staff_project_members"], sourceFields: ["owner_user_id", "created_at", "branch_id", "department_id", "project_id", "user_id"], joins: ["staff_project_members.project_id = staff_projects.id"], queryCost: "medium", freshness: "transactional",
  },
  "overdue-project-milestones": {
    moduleId: "projects", label: "Overdue project milestones", description: "Incomplete milestones past their UTC due time in projects owned by or shared with the current reader.",
    formula: "Count planned or in-progress project milestones whose due time has passed and falls in the selected due-time interval. Completed and cancelled milestones are excluded.", unit: "count", dateBasis: "Milestone due time (UTC)", permission: "records.read",
    sourceTables: ["staff_project_milestones", "staff_projects", "staff_project_members", "hr_people"], sourceFields: ["staff_project_milestones.title", "staff_project_milestones.due_at", "staff_project_milestones.owner_user_id", "staff_project_milestones.state", "staff_project_milestones.version", "staff_projects.company_id", "staff_projects.branch_id", "staff_projects.department_id", "staff_projects.owner_user_id", "staff_project_members.user_id", "hr_people.legal_name"], joins: ["staff_project_milestones.project_id = staff_projects.id", "staff_project_members.project_id = staff_projects.id", "hr_people.user_id = staff_project_milestones.owner_user_id"], queryCost: "medium", freshness: "transactional", moduleGate: "projects", acceptancePending: true,
  },
  "project-dependency-blockers": {
    moduleId: "projects", label: "Blocked project dependencies", description: "Incomplete project milestones whose prerequisite milestone is not complete, for projects owned by or shared with the current reader.",
    formula: "Count each planned or in-progress successor milestone once when at least one prerequisite is not completed. Filter by successor due date, and use current prerequisite state. Projects are limited to current owner or member access.", unit: "count", dateBasis: "Successor due date (UTC); current prerequisite state", permission: "records.read",
    sourceTables: ["staff_project_milestone_dependencies", "staff_project_milestones", "staff_projects", "staff_project_members"], sourceFields: ["staff_project_milestone_dependencies.project_id", "staff_project_milestone_dependencies.predecessor_milestone_id", "staff_project_milestone_dependencies.successor_milestone_id", "staff_project_milestones.title", "staff_project_milestones.due_at", "staff_project_milestones.state", "staff_project_milestones.version", "staff_projects.company_id", "staff_projects.branch_id", "staff_projects.department_id", "staff_projects.owner_user_id", "staff_project_members.user_id"], joins: ["dependency.predecessor_milestone_id = prerequisite milestone", "dependency.successor_milestone_id = blocked milestone", "current project owner or member relationship"], queryCost: "medium", freshness: "transactional", moduleGate: "projects", acceptancePending: true,
  },
  "project-actual-hours": {
    moduleId: "projects", label: "Tracked project hours", description: "Approved project work log minutes, converted to exact milliseconds, for projects the current reader owns or belongs to.",
    formula: "Sum minutes from approved, owner-reviewed project work logs and convert to exact milliseconds. Project membership is checked at read time; date filters apply to the work date.", unit: "duration", dateBasis: "Work date (UTC)", permission: "records.read",
    sourceTables: ["staff_project_work_logs", "staff_projects", "staff_project_members"], sourceFields: ["staff_project_work_logs.project_id", "staff_project_work_logs.user_id", "staff_project_work_logs.work_date", "staff_project_work_logs.minutes", "staff_project_work_logs.state", "staff_project_work_logs.version", "staff_projects.company_id", "staff_projects.branch_id", "staff_projects.department_id", "staff_projects.owner_user_id", "staff_project_members.user_id"], joins: ["staff_project_work_logs.project_id = staff_projects.id", "current project owner or member relationship"], queryCost: "medium", freshness: "transactional", moduleGate: "projects", acceptancePending: true,
  },
  "project-posted-cost": {
    moduleId: "projects", label: "Posted project cost", description: "Finance-approved project allocations from posted expense debit lines, in the selected currency.",
    formula: "Sum approved project allocations linked to debit lines on currently posted expense or other-expense journals. Reversed and non-posted journals are excluded. The selected currency remains explicit and currencies are never combined.", unit: "money", dateBasis: "Finance journal posting date", permission: "finance.read",
    sourceTables: ["staff_project_cost_allocations", "staff_projects", "staff_project_members", "finance_journal_lines", "finance_journals", "finance_accounts"], sourceFields: ["staff_project_cost_allocations.amount_minor", "staff_project_cost_allocations.state", "staff_project_cost_allocations.version", "staff_projects.company_id", "staff_projects.branch_id", "staff_projects.department_id", "staff_projects.owner_user_id", "staff_project_members.user_id", "finance_journal_lines.debit_minor", "finance_journal_lines.dimension_json", "finance_journals.currency", "finance_journals.entry_date", "finance_journals.status", "finance_accounts.category"], joins: ["allocation.journal_line_id = finance_journal_lines.id", "finance_journal_lines.journal_id = finance_journals.id", "finance_journal_lines.account_id = finance_accounts.id", "current project owner or member relationship"], queryCost: "medium", freshness: "transactional", moduleGate: "projects", acceptancePending: true,
  },
  routines: {
    moduleId: "collaboration", label: "My published routines", description: "Published routine templates assigned to the current reader, grouped by publication status.",
    formula: "Count published routine templates assigned to the current reader.", unit: "count", dateBasis: "Current template; date filter does not apply", permission: "self.tasks",
    sourceTables: ["routine_templates"], sourceFields: ["assignee_user_id", "published", "branch_id", "department_id"], joins: [], queryCost: "low", freshness: "transactional",
  },
  "active-customers": {
    moduleId: "customer_360", label: "Active customers", description: "Current active canonical customer profiles. Company-wide source records do not carry branch scope.",
    formula: "Count finance parties where kind is customer, status is active and merged_into_id is null.", unit: "count", dateBasis: "Current profile status", permission: "commerce.read",
    sourceTables: ["finance_parties"], sourceFields: ["kind", "status", "merged_into_id"], joins: [], queryCost: "low", freshness: "transactional", companyOnly: true, acceptancePending: true,
  },
  "recent-customer-activity": {
    moduleId: "customer_360", label: "Recent customer activity", description: "Visible entries from the Customer 360 timeline. Source-record snapshots use their latest update time; immutable events use their event time.",
    formula: "Count Customer 360 timeline entries in the selected UTC interval, grouped by event month or source type. Includes only the source categories exposed by the reader's current permissions and effective Company, Branch and Department scope. Some entries represent the latest source state rather than a complete transition history.", unit: "count", dateBasis: "Timeline event time or current source update time (UTC)", permission: "commerce.read",
    sourceTables: ["commerce_orders", "commerce_fulfillments", "commerce_returns", "customer_contact_notes", "customer_contact_note_history", "customer_followups", "customer_followup_events", "support_tickets", "support_ticket_events", "field_service_agreements", "field_service_agreement_events", "field_service_orders", "field_service_order_events", "legal_items", "legal_native_references", "legal_members", "legal_events", "finance_invoices", "crm_leads", "crm_opportunities", "crm_case_events", "crm_lead_activities"], sourceFields: ["company_id", "branch_id", "department_id", "party_id", "customer_id", "source_id", "kind", "number", "status", "action", "version", "created_at", "updated_at"], joins: ["Customer 360 timeline's authorized sales, CRM, support, Field Service, Finance and Legal source relationships"], queryCost: "medium", freshness: "transactional", acceptancePending: true,
  },
  "duplicate-reviews-pending": {
    moduleId: "customer_authority", label: "Duplicate reviews pending", description: "Company-wide customer duplicate decisions awaiting an independent eligible reviewer.",
    formula: "Count customer_duplicate_reviews with pending status in the current Company.", unit: "count", dateBasis: "Current review state", permission: "commerce.sales", requiredPermissions: ["commerce.read"],
    sourceTables: ["customer_duplicate_reviews"], sourceFields: ["company_id", "status", "updated_at"], joins: [], queryCost: "low", freshness: "transactional", companyOnly: true, acceptancePending: true,
  },
  "identity-change-events": {
    moduleId: "customer_authority", label: "Customer identity change events", description: "Immutable workflow events for Company customer identity changes.",
    formula: "Count customer_identity_change_events created in the selected UTC date interval, grouped by month or action.", unit: "count", dateBasis: "Event creation time (UTC)", permission: "commerce.sales", requiredPermissions: ["commerce.read"],
    sourceTables: ["customer_identity_changes", "customer_identity_change_events"], sourceFields: ["company_id", "status", "action", "created_at"], joins: ["customer_identity_change_events.change_id = customer_identity_changes.id"], queryCost: "medium", freshness: "transactional", companyOnly: true, acceptancePending: true,
  },
  "open-support-tickets": {
    moduleId: "support", label: "Open support tickets", description: "Tickets not closed or cancelled in the reader's current Company, Branch and Department scope.",
    formula: "Count support tickets whose status is not closed or cancelled, grouped by status or authorized organization location.", unit: "count", dateBasis: "Current ticket status", permission: "support.read",
    sourceTables: ["support_tickets", "organization_nodes"], sourceFields: ["company_id", "branch_id", "department_id", "status"], joins: ["support_tickets.branch_id = organization_nodes.id", "support_tickets.department_id = organization_nodes.id"], queryCost: "low", freshness: "transactional", acceptancePending: true,
  },
  "overdue-support-sla": {
    moduleId: "support", label: "Overdue support SLAs", description: "Open tickets with a running SLA clock whose approved due time is before the reference time.",
    formula: "Count scoped support tickets not closed or cancelled with sla_clock_json.state equal to running and dueAt before the reference time.", unit: "count", dateBasis: "Current SLA clock", permission: "support.read",
    sourceTables: ["support_tickets"], sourceFields: ["company_id", "branch_id", "department_id", "status", "sla_clock_json.state", "sla_clock_json.dueAt"], joins: [], queryCost: "low", freshness: "transactional", acceptancePending: true,
  },
  "active-support-incidents": {
    moduleId: "support", label: "Active support incidents", description: "Open, investigating or mitigated incidents in the reader's current Company, Branch and Department scope. Incident narrative and linked customer ticket text are not aggregated.",
    formula: "Count support_incidents in open, investigating or mitigated state, grouped by status, severity or authorized organization location.", unit: "count", dateBasis: "Current incident state", permission: "support.read",
    sourceTables: ["support_incidents", "organization_nodes"], sourceFields: ["support_incidents.company_id", "support_incidents.branch_id", "support_incidents.department_id", "support_incidents.status", "support_incidents.severity"], joins: ["support_incidents.branch_id = organization_nodes.id", "support_incidents.department_id = organization_nodes.id"], queryCost: "low", freshness: "transactional", acceptancePending: true,
  },
  "active-assets": {
    moduleId: "assets", label: "Active assets", description: "Assets in the in-service lifecycle state, grouped by condition or authorized organization location.",
    formula: "Count operational_assets whose lifecycle_status is in_service, grouped by condition, branch or department.", unit: "count", dateBasis: "Current lifecycle state", permission: "records.read",
    sourceTables: ["operational_assets", "organization_nodes"], sourceFields: ["company_id", "branch_id", "department_id", "lifecycle_status", "condition"], joins: ["operational_assets.branch_id = organization_nodes.id", "operational_assets.department_id = organization_nodes.id"], queryCost: "low", freshness: "transactional", moduleGate: "assets", acceptancePending: true,
  },
  "asset-condition-location": {
    moduleId: "assets", label: "Asset condition and location", description: "In-service asset counts by current recorded condition or authorized organization location.",
    formula: "Count operational_assets in the in_service lifecycle state, grouped by condition, branch or department.", unit: "count", dateBasis: "Current asset record", permission: "records.read",
    sourceTables: ["operational_assets", "organization_nodes"], sourceFields: ["company_id", "branch_id", "department_id", "lifecycle_status", "condition"], joins: ["operational_assets.branch_id = organization_nodes.id", "operational_assets.department_id = organization_nodes.id"], queryCost: "low", freshness: "transactional", moduleGate: "assets", acceptancePending: true,
  },
  "asset-retirements": {
    moduleId: "assets", label: "Asset retirements", description: "Approved asset retirement events recorded in the selected UTC interval.",
    formula: "Count immutable operational_asset_events with action retire and lifecycle_status retired, in the reader's Company, Branch and Department scope.", unit: "count", dateBasis: "Retirement event time (UTC)", permission: "records.read",
    sourceTables: ["operational_asset_events", "operational_assets"], sourceFields: ["operational_asset_events.action", "operational_asset_events.lifecycle_status", "operational_asset_events.created_at", "operational_assets.company_id", "operational_assets.branch_id", "operational_assets.department_id"], joins: ["operational_asset_events.asset_id = operational_assets.id"], queryCost: "medium", freshness: "transactional", moduleGate: "assets", acceptancePending: true,
  },
  "open-work-orders": {
    moduleId: "maintenance", label: "Open maintenance work orders", description: "Work orders not completed or cancelled, in the reader's current authorized organization scope.",
    formula: "Count maintenance_work_orders whose status is not completed or cancelled, grouped by status or authorized organization location.", unit: "count", dateBasis: "Current work-order state", permission: "records.read",
    sourceTables: ["maintenance_work_orders", "operational_assets", "organization_nodes"], sourceFields: ["company_id", "branch_id", "department_id", "status", "asset_id"], joins: ["maintenance_work_orders.asset_id = operational_assets.id"], queryCost: "medium", freshness: "transactional", moduleGate: "maintenance", acceptancePending: true,
  },
  "blocked-maintenance": {
    moduleId: "maintenance", label: "Blocked maintenance work", description: "Current blocked work orders in the reader's authorized organization scope.",
    formula: "Count maintenance_work_orders with status blocked, grouped by authorized organization location.", unit: "count", dateBasis: "Current blocker state", permission: "records.read",
    sourceTables: ["maintenance_work_orders"], sourceFields: ["company_id", "branch_id", "department_id", "status", "blocked_reason"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "maintenance", acceptancePending: true,
  },
  "asset-downtime": {
    moduleId: "maintenance", label: "Recorded asset downtime", description: "Recorded, ended maintenance downtime intervals for assets in the authorized organization scope. Overlapping intervals on one asset count once.",
    formula: "Sum the union of recorded downtime intervals from completed or cancelled maintenance work orders, clipped to the selected UTC interval. Overlapping intervals for the same asset count once. The source records start and end times but no approval state.", unit: "duration", dateBasis: "Recorded downtime start and end times (UTC)", permission: "records.read",
    sourceTables: ["maintenance_work_orders", "operational_assets", "organization_nodes"], sourceFields: ["maintenance_work_orders.company_id", "maintenance_work_orders.branch_id", "maintenance_work_orders.department_id", "maintenance_work_orders.status", "maintenance_work_orders.downtime_started_at", "maintenance_work_orders.downtime_ended_at", "operational_assets.code", "operational_assets.name", "organization_nodes.display_name"], joins: ["maintenance_work_orders.asset_id = operational_assets.id", "maintenance_work_orders.branch_id = organization_nodes.id", "maintenance_work_orders.department_id = organization_nodes.id"], queryCost: "medium", freshness: "transactional", moduleGate: "maintenance", acceptancePending: true,
  },
  "quality-holds": {
    moduleId: "quality", label: "Open quality holds", description: "Nonterminal quality holds in the reader's Company or Branch scope.",
    formula: "Count inventory_quality_holds whose status is not released, cancelled or disposed.", unit: "count", dateBasis: "Current hold state", permission: "quality.read",
    sourceTables: ["inventory_quality_holds", "organization_nodes"], sourceFields: ["company_id", "branch_id", "status", "organization_nodes.display_name"], joins: ["inventory_quality_holds.branch_id = organization_nodes.id"], queryCost: "low", freshness: "transactional", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "quality-releases": {
    moduleId: "quality", label: "Quality releases", description: "Quality holds released during the selected UTC date interval.",
    formula: "Count inventory_quality_holds with status released and released_at inside the selected UTC date interval.", unit: "count", dateBasis: "Release time (UTC)", permission: "quality.read",
    sourceTables: ["inventory_quality_holds", "organization_nodes"], sourceFields: ["company_id", "branch_id", "status", "released_at", "organization_nodes.display_name"], joins: ["inventory_quality_holds.branch_id = organization_nodes.id"], queryCost: "low", freshness: "transactional", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "inspection-pass-rate": {
    moduleId: "quality", label: "Inspection pass rate", description: "Passed immutable quality inspection events as a share of all finalized pass/fail events.",
    formula: "Passed inventory_quality_inspections rows divided by all immutable pass/fail inventory_quality_inspections rows in the selected UTC inspection-time interval. Each reinspection is a separate finalized event; report a rounded hundredth of a percentage point.", unit: "percent", dateBasis: "Inspection event time (UTC)", permission: "quality.read",
    sourceTables: ["inventory_quality_inspections", "inventory_quality_holds"], sourceFields: ["inventory_quality_inspections.result", "inventory_quality_inspections.created_at", "inventory_quality_holds.company_id", "inventory_quality_holds.branch_id"], joins: ["inventory_quality_inspections.hold_id = inventory_quality_holds.id"], queryCost: "medium", freshness: "transactional", moduleGate: "quality", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "opportunity-pipeline": {
    moduleId: "crm", label: "Opportunity pipeline", description: "Open CRM opportunity expected value in the selected currency and close-date interval.",
    formula: "Sum expected_minor for discovery, proposal and negotiation opportunities in the selected currency and close-date interval; exclude won and lost opportunities.", unit: "money", dateBasis: "Expected close date", permission: "commerce.read",
    sourceTables: ["crm_leads", "crm_opportunities", "organization_nodes"], sourceFields: ["company_id", "branch_id", "department_id", "stage", "expected_minor", "currency", "close_date"], joins: ["crm_opportunities.lead_id = crm_leads.id"], queryCost: "medium", freshness: "transactional", moduleGate: "crm", acceptancePending: true,
  },
  "overdue-followups": {
    moduleId: "crm", label: "Overdue customer follow-ups", description: "Open customer follow-ups due before the current UTC date in the reader's scoped orders.",
    formula: "Count open customer_followups due within the selected interval and before the current UTC date, joined to their scoped sales orders or quotations.", unit: "count", dateBasis: "Follow-up due date (UTC)", permission: "commerce.read",
    sourceTables: ["customer_followups", "commerce_orders"], sourceFields: ["status", "due_date", "order_id", "company_id", "branch_id", "department_id"], joins: ["customer_followups.order_id = commerce_orders.id"], queryCost: "medium", freshness: "transactional", moduleGate: "crm", acceptancePending: true,
  },
  "brand-profiles": {
    moduleId: "brand", label: "Configured brand profiles", description: "Current brand profile rows for active Branches in the Company; excludes Holding-level and inactive Branch records.",
    formula: "Count organization_brand_profiles joined to active Branch nodes in the Company. Holding-level rows and profiles for inactive Branches are excluded.", unit: "count", dateBasis: "Current profile configuration", permission: "records.read",
    sourceTables: ["organization_brand_profiles", "organization_nodes"], sourceFields: ["scope_kind", "scope_id", "company_id", "document_mode", "updated_at", "organization_nodes.display_name", "organization_nodes.status"], joins: ["organization_brand_profiles.scope_id = active organization_nodes branch"], queryCost: "low", freshness: "transactional", moduleGate: "brand", companyOnly: true, departmentScopeUnsupported: true, acceptancePending: true,
  },
  "brand-profile-updates": {
    moduleId: "brand", label: "Brand profile updates", description: "Current active Branch profile rows last updated in the selected UTC date interval; this is not a published version history.",
    formula: "Count current profile rows for active Branches in the Company whose updated_at is within the selected UTC interval. This does not count each edit because the source stores only the current row.", unit: "count", dateBasis: "Current row updated_at (UTC)", permission: "records.read",
    sourceTables: ["organization_brand_profiles", "organization_nodes"], sourceFields: ["scope_kind", "scope_id", "company_id", "updated_at", "organization_nodes.display_name", "organization_nodes.status"], joins: ["organization_brand_profiles.scope_id = active organization_nodes branch"], queryCost: "low", freshness: "transactional", moduleGate: "brand", companyOnly: true, departmentScopeUnsupported: true, acceptancePending: true,
  },
  "active-assignments": {
    moduleId: "identity_access", label: "Active access assignments", description: "Currently effective, non-revoked assignments, grouped by role or assignment scope.",
    formula: "Count assignments for which iam_assignment_active(id, reference time) is true, limited to the selected organization scope. This rechecks validity, revocation, parent grants, role publication, principal status and lifecycle/policy eligibility.", unit: "count", dateBasis: "Current effective IAM authority", permission: "iam.read",
    sourceTables: ["assignments", "iam_roles", "iam_role_revisions", "iam_members", "iam_identities", "iam_connections", "people_access_cutoffs", "people_lifecycle_cases", "people_lifecycle_grants", "organization_nodes"], sourceFields: ["company_id", "valid_from", "valid_until", "revoked_at", "parent_id", "scope_kind", "scope_node_id", "role", "source", "user_id", "identity.status", "identity.enabled", "identity.observed_at", "connection.status"], joins: ["iam_assignment_active() revalidates the assignment chain and principal", "assignments.scope_node_id = organization_nodes.id for branch/department scope"], queryCost: "medium", freshness: "transactional", moduleGate: "identity_access", acceptancePending: true,
  },
  "pending-access-reviews": {
    moduleId: "identity_access", label: "Pending access reviews", description: "Open access review decisions still awaiting an eligible reviewer.",
    formula: "Count pending iam_access_review_items in open campaigns, limited to the selected organization scope.", unit: "count", dateBasis: "Current review state", permission: "iam.review",
    sourceTables: ["iam_access_review_campaigns", "iam_access_review_items", "organization_nodes"], sourceFields: ["company_id", "status", "decision", "scope_kind", "scope_node_id", "branch_id"], joins: ["iam_access_review_items.campaign_id = iam_access_review_campaigns.id"], queryCost: "medium", freshness: "transactional", moduleGate: "identity_access", acceptancePending: true,
  },
  "active-organization-nodes": {
    moduleId: "organization", label: "Active organization nodes", description: "Active branch and department nodes in the current Company scope.",
    formula: "Count active organization_nodes whose kind is branch or department, limited to the selected branch or department.", unit: "count", dateBasis: "Current node status", permission: "organization.manage",
    sourceTables: ["organization_nodes"], sourceFields: ["company_id", "kind", "status", "branch_id", "display_name"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "organization", acceptancePending: true,
  },
  "pending-structure-changes": {
    moduleId: "organization", label: "Pending structure changes", description: "Organization change requests currently submitted for review.",
    formula: "Count organization_node_changes in submitted state; branch selections limit the metric to department changes for that Branch.", unit: "count", dateBasis: "Current workflow state", permission: "organization.manage",
    sourceTables: ["organization_node_changes"], sourceFields: ["company_id", "state", "node_kind", "branch_id", "updated_at"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "organization", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "marketing-lead-candidates": {
    moduleId: "marketing", label: "Marketing lead candidates", description: "Lead candidate counts by current handoff state; contact fields are not read.",
    formula: "Count marketing_lead_candidates by current status in the authorized Company, Branch and Department scope.", unit: "count", dateBasis: "Current candidate state", permission: "records.read",
    sourceTables: ["marketing_lead_candidates"], sourceFields: ["company_id", "branch_id", "department_id", "status", "created_at"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "marketing", acceptancePending: true,
  },
  "marketing-engagement-events": {
    moduleId: "marketing", label: "Marketing engagement events", description: "Recorded engagement event counts by UTC month or channel; evidence and contact details are not read.",
    formula: "Count marketing_engagement_events in the selected UTC interval, joined to candidates only to apply the current organization scope.", unit: "count", dateBasis: "Event creation time (UTC)", permission: "records.read",
    sourceTables: ["marketing_engagement_events", "marketing_lead_candidates"], sourceFields: ["candidate_id", "event_type", "channel", "created_at", "company_id", "branch_id", "department_id"], joins: ["marketing_engagement_events.candidate_id = marketing_lead_candidates.id"], queryCost: "medium", freshness: "transactional", moduleGate: "marketing", acceptancePending: true,
  },
  "completed-manufacturing-orders": {
    moduleId: "manufacturing", label: "Completed manufacturing orders", description: "Completed production order counts by UTC month or Branch; mixed product quantities are not combined.",
    formula: "Count manufacturing_orders with completed status and updated_at within the selected interval.", unit: "count", dateBasis: "Completed order updated_at (UTC)", permission: "records.read",
    sourceTables: ["manufacturing_orders"], sourceFields: ["company_id", "branch_id", "status", "updated_at"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "manufacturing", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "manufacturing-orders-with-scrap": {
    moduleId: "manufacturing", label: "Manufacturing orders with reported scrap", description: "Completed orders with a positive reported scrap quantity; quantities remain separated by product.",
    formula: "Count completed manufacturing_orders with reported_scrap_milli greater than zero and updated_at within the selected interval.", unit: "count", dateBasis: "Completed order updated_at (UTC)", permission: "records.read",
    sourceTables: ["manufacturing_orders"], sourceFields: ["company_id", "branch_id", "status", "reported_scrap_milli", "updated_at"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "manufacturing", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "production-cost": {
    moduleId: "manufacturing", label: "Posted manufacturing material and labor cost", description: "Journal-posted material issue, material return and labor costs for completed production orders, grouped by month, Branch, output product or facility.",
    formula: "Sum amount_minor for material_issue, material_return and labor movements linked to completed manufacturing orders and posted finance journals in the selected company currency and journal posting interval; unposted movements and estimates are excluded.", unit: "money", dateBasis: "Linked journal posted_at (UTC)", permission: "records.read",
    sourceTables: ["manufacturing_cost_movements", "manufacturing_orders", "manufacturing_specifications", "commerce_products", "commerce_warehouses", "organization_nodes", "finance_journals", "commerce_settings"],
    sourceFields: ["manufacturing_cost_movements.company_id", "order_id", "movement_kind", "amount_minor", "journal_id", "occurred_on", "version", "manufacturing_orders.branch_id", "status", "specification_id", "warehouse_id", "manufacturing_specifications.output_product_id", "commerce_products.name", "commerce_warehouses.name", "organization_nodes.display_name", "finance_journals.status", "finance_journals.currency", "finance_journals.posted_at", "commerce_settings.currency"],
    joins: ["manufacturing_cost_movements.order_id = manufacturing_orders.id", "manufacturing_orders.specification_id = manufacturing_specifications.id", "manufacturing_specifications.output_product_id = commerce_products.id", "manufacturing_orders.warehouse_id = commerce_warehouses.id", "manufacturing_orders.branch_id = organization_nodes.id", "manufacturing_cost_movements.journal_id = finance_journals.id"],
    queryCost: "medium", freshness: "transactional", moduleGate: "manufacturing", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "planning-shortage-scenarios": {
    moduleId: "planning", label: "Planning scenarios with material shortages", description: "Planning scenarios whose recorded shortage list includes a positive shortage.",
    formula: "Count planning_scenarios whose shortages_json contains at least one item with shortageMilli greater than zero.", unit: "count", dateBasis: "Current scenario; date filter uses due date", permission: "records.read",
    sourceTables: ["planning_scenarios"], sourceFields: ["company_id", "branch_id", "due_date", "shortages_json", "status"], joins: [], queryCost: "medium", freshness: "transactional", moduleGate: "planning", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "proposed-planning-recommendations": {
    moduleId: "planning", label: "Proposed planning recommendations", description: "Planning recommendations awaiting approval, grouped by recommendation status or Branch.",
    formula: "Count planning_recommendations in proposed state by their scenario's Company and Branch; filter by feasible_date.", unit: "count", dateBasis: "Feasible date", permission: "records.read",
    sourceTables: ["planning_recommendations", "planning_scenarios"], sourceFields: ["planning_recommendations.status", "planning_recommendations.feasible_date", "planning_scenarios.company_id", "planning_scenarios.branch_id"], joins: ["planning_recommendations.scenario_id = planning_scenarios.id"], queryCost: "medium", freshness: "transactional", moduleGate: "planning", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "capacity-feasible-recommendations": {
    moduleId: "planning", label: "Capacity-feasible recommendations", description: "Counts proposed or approved recommendations whose required minutes fit the scenario capacity; this is a count, not a utilization percentage.",
    formula: "Count planning recommendations where required_minutes is less than or equal to the scenario's capacity_minutes, grouped by feasible month or Branch.", unit: "count", dateBasis: "Feasible date", permission: "records.read",
    sourceTables: ["planning_recommendations", "planning_scenarios"], sourceFields: ["planning_recommendations.status", "planning_recommendations.required_minutes", "planning_recommendations.feasible_date", "planning_scenarios.capacity_minutes", "planning_scenarios.company_id", "planning_scenarios.branch_id"], joins: ["planning_recommendations.scenario_id = planning_scenarios.id"], queryCost: "medium", freshness: "transactional", moduleGate: "planning", departmentScopeUnsupported: true, acceptancePending: true,
  },
  "open-ecommerce-carts": {
    moduleId: "ecommerce", label: "Open e-commerce carts", description: "Current open cart counts by channel-independent status or authorized location.",
    formula: "Count ecommerce_carts in open status in the authorized Company, Branch and Department scope.", unit: "count", dateBasis: "Current cart state", permission: "commerce.read",
    sourceTables: ["ecommerce_carts"], sourceFields: ["company_id", "branch_id", "department_id", "status", "created_at"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "ecommerce", acceptancePending: true,
  },
  "completed-ecommerce-checkouts": {
    moduleId: "ecommerce", label: "Completed e-commerce checkouts", description: "Confirmed or fulfilled checkout counts by UTC month or checkout state.",
    formula: "Count ecommerce_checkouts in confirmed, partially_fulfilled or fulfilled state whose created_at falls in the selected interval.", unit: "count", dateBasis: "Checkout creation time (UTC)", permission: "commerce.read",
    sourceTables: ["ecommerce_checkouts", "ecommerce_carts"], sourceFields: ["ecommerce_checkouts.status", "ecommerce_checkouts.created_at", "ecommerce_carts.company_id", "ecommerce_carts.branch_id", "ecommerce_carts.department_id"], joins: ["ecommerce_checkouts.cart_id = ecommerce_carts.id"], queryCost: "medium", freshness: "transactional", moduleGate: "ecommerce", acceptancePending: true,
  },
  "ecommerce-return-links": {
    moduleId: "ecommerce", label: "E-commerce return links", description: "Immutable online return links counted in the selected period and authorized organization scope.",
    formula: "Count ecommerce_return_links created in the selected interval, joined through checkout and cart for current Company and organization scope.", unit: "count", dateBasis: "Return link creation time (UTC)", permission: "commerce.read",
    sourceTables: ["ecommerce_return_links", "ecommerce_checkouts", "ecommerce_carts"], sourceFields: ["ecommerce_return_links.status", "ecommerce_return_links.created_at", "ecommerce_checkouts.cart_id", "ecommerce_carts.company_id", "ecommerce_carts.branch_id", "ecommerce_carts.department_id"], joins: ["ecommerce_return_links.checkout_id = ecommerce_checkouts.id", "ecommerce_checkouts.cart_id = ecommerce_carts.id"], queryCost: "medium", freshness: "transactional", moduleGate: "ecommerce", acceptancePending: true,
  },
  "ecommerce-fulfillment-links": {
    moduleId: "ecommerce", label: "E-commerce fulfillment links", description: "Immutable online fulfillment links counted in the selected period and authorized organization scope.",
    formula: "Count ecommerce_fulfillment_links created in the selected interval, joined through checkout and cart for current Company and organization scope.", unit: "count", dateBasis: "Fulfillment link creation time (UTC)", permission: "commerce.read",
    sourceTables: ["ecommerce_fulfillment_links", "ecommerce_checkouts", "ecommerce_carts"], sourceFields: ["ecommerce_fulfillment_links.status", "ecommerce_fulfillment_links.created_at", "ecommerce_checkouts.cart_id", "ecommerce_carts.company_id", "ecommerce_carts.branch_id", "ecommerce_carts.department_id"], joins: ["ecommerce_fulfillment_links.checkout_id = ecommerce_checkouts.id", "ecommerce_checkouts.cart_id = ecommerce_carts.id"], queryCost: "medium", freshness: "transactional", moduleGate: "ecommerce", acceptancePending: true,
  },
  "active-benefit-elections": {
    moduleId: "benefits", label: "Current benefit elections", description: "Company-level counts of current non-cancelled elections, grouped only by workflow state.",
    formula: "Count benefit_elections effective on the reference date with status other than cancelled; no personal fields are read.", unit: "count", dateBasis: "Current effective interval", permission: "records.read",
    sourceTables: ["benefit_elections"], sourceFields: ["company_id", "effective_from", "effective_to", "status"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "benefits", companyOnly: true, acceptancePending: true,
  },
  "published-benefit-plans": {
    moduleId: "benefits", label: "Published benefit plans", description: "Company-level count of currently effective published plan configurations; no election or person data is read.",
    formula: "Count benefit_plans with published status and an effective interval containing the reference date.", unit: "count", dateBasis: "Current effective interval", permission: "records.read",
    sourceTables: ["benefit_plans"], sourceFields: ["company_id", "status", "effective_from", "effective_to"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "benefits", companyOnly: true, acceptancePending: true,
  },
  "learning-completions": {
    moduleId: "learning", label: "Completed learning assignments", description: "Completion event counts by UTC month or current course; learner identity and result evidence are not read.",
    formula: "Count learning_assignment_events with complete action and completed status in the selected interval for assignments in the authorized scope.", unit: "count", dateBasis: "Completion event time (UTC)", permission: "records.read",
    sourceTables: ["learning_assignment_events", "learning_assignments", "learning_courses"], sourceFields: ["learning_assignment_events.action", "learning_assignment_events.status", "learning_assignment_events.created_at", "learning_assignments.company_id", "learning_assignments.branch_id", "learning_assignments.department_id", "learning_courses.title"], joins: ["learning_assignment_events.assignment_id = learning_assignments.id", "learning_assignments.course_id = learning_courses.id"], queryCost: "medium", freshness: "transactional", moduleGate: "learning", acceptancePending: true,
  },
  "overdue-required-learning": {
    moduleId: "learning", label: "Overdue required learning", description: "Required assignments past due and not completed or waived at the reference time.",
    formula: "Count assignments with a required course, due_date before the current UTC date and status assigned or in_progress.", unit: "count", dateBasis: "Current UTC date and assignment due date", permission: "records.read",
    sourceTables: ["learning_assignments", "learning_courses"], sourceFields: ["learning_assignments.company_id", "learning_assignments.branch_id", "learning_assignments.department_id", "learning_assignments.due_date", "learning_assignments.status", "learning_courses.required_for"], joins: ["learning_assignments.course_id = learning_courses.id"], queryCost: "medium", freshness: "transactional", moduleGate: "learning", acceptancePending: true,
  },
  "open-requisitions": {
    moduleId: "talent", label: "Open talent requisitions", description: "Approved requisitions currently accepting applications in the authorized organization scope.",
    formula: "Count talent_requisitions in open status by Branch or Department.", unit: "count", dateBasis: "Current requisition state", permission: "records.review",
    sourceTables: ["talent_requisitions"], sourceFields: ["company_id", "branch_id", "department_id", "status", "job_title"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "talent", acceptancePending: true,
  },
  "application-stage-counts": {
    moduleId: "talent", label: "Talent application stage counts", description: "Application counts by current stage; candidate identity and application evidence are not read.",
    formula: "Count talent_applications by status for requisitions in the authorized Company, Branch and Department scope, updated in the selected interval.", unit: "count", dateBasis: "Application update time (UTC)", permission: "records.review",
    sourceTables: ["talent_applications", "talent_requisitions"], sourceFields: ["talent_applications.company_id", "talent_applications.status", "talent_applications.updated_at", "talent_requisitions.branch_id", "talent_requisitions.department_id"], joins: ["talent_applications.requisition_id = talent_requisitions.id"], queryCost: "medium", freshness: "transactional", moduleGate: "talent", acceptancePending: true,
  },
  "open-service-orders": {
    moduleId: "field_service", label: "Open field service orders", description: "Nonterminal service orders in the authorized Company, Branch and Department scope.",
    formula: "Count field_service_orders whose status is not completed, cancelled or billed by current organization scope.", unit: "count", dateBasis: "Current service order state", permission: "records.read",
    sourceTables: ["field_service_orders"], sourceFields: ["company_id", "branch_id", "department_id", "status", "scheduled_at"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "field_service", acceptancePending: true,
  },
  "service-billing-releases": {
    moduleId: "field_service", label: "Released service billings", description: "Immutable service billing release counts by UTC month or authorized Branch.",
    formula: "Count field_service_billing_releases created in the selected interval, joined to service orders for Company, Branch and Department scope.", unit: "count", dateBasis: "Billing release time (UTC)", permission: "records.read",
    sourceTables: ["field_service_billing_releases", "field_service_orders"], sourceFields: ["field_service_billing_releases.created_at", "field_service_orders.company_id", "field_service_orders.branch_id", "field_service_orders.department_id"], joins: ["field_service_billing_releases.order_id = field_service_orders.id"], queryCost: "medium", freshness: "transactional", moduleGate: "field_service", acceptancePending: true,
  },
  "field-visit-completion": {
    moduleId: "field_service", label: "Field visit completion", description: "Share of eligible scheduled field service orders with a recorded visit completion. Each service order counts once.",
    formula: "Count eligible scheduled service orders with status completed or billed and visit_completed_at recorded, divided by eligible scheduled, dispatched, in-visit, exception, completed and billed service orders with scheduled_at in the selected interval; canceled orders are excluded.", unit: "percent", dateBasis: "Current scheduled visit time (UTC)", permission: "records.read",
    sourceTables: ["field_service_orders", "organization_nodes", "hr_people"], sourceFields: ["field_service_orders.company_id", "field_service_orders.branch_id", "field_service_orders.department_id", "field_service_orders.technician_id", "field_service_orders.scheduled_at", "field_service_orders.status", "field_service_orders.visit_completed_at", "field_service_orders.version", "hr_people.legal_name"], joins: ["field_service_orders.branch_id = organization_nodes.id", "field_service_orders.technician_id = hr_people.user_id"], queryCost: "medium", freshness: "transactional", moduleGate: "field_service", acceptancePending: true,
  },
  "approved-agent-versions": {
    moduleId: "agent", label: "Approved agent versions", description: "Current approved agent version counts; prompt and policy text are not read.",
    formula: "Count agent_versions in approved status joined to approved Company agent_catalog records.", unit: "count", dateBasis: "Current approval state", permission: "records.read",
    sourceTables: ["agent_catalog", "agent_versions"], sourceFields: ["agent_catalog.company_id", "agent_catalog.status", "agent_versions.status"], joins: ["agent_versions.agent_id = agent_catalog.id"], queryCost: "medium", freshness: "transactional", moduleGate: "agent", companyOnly: true, acceptancePending: true,
  },
  "agent-invocation-outcomes": {
    moduleId: "agent", label: "Agent invocation outcomes", description: "Succeeded and failed invocation counts by UTC month; questions, source text and error bodies are not read.",
    formula: "Count ai_invocation_evidence by outcome and UTC month in the selected interval for the current Company.", unit: "count", dateBasis: "Invocation creation time (UTC)", permission: "records.read",
    sourceTables: ["ai_invocation_evidence"], sourceFields: ["company_id", "outcome", "created_at"], joins: [], queryCost: "low", freshness: "transactional", moduleGate: "agent", companyOnly: true, acceptancePending: true,
  },
};

const periodComparableMetrics = new Set<MetricId>([
  "receivables", "payables", "invoiced-sales", "cash-receipts", "sales-orders", "sales-fulfillment-rate", "sales-cancellations", "sales-cancellation-value", "purchase-orders", "open-po-receipts", "invoice-match-exceptions", "pos-sales", "net-pos-sales", "pos-payment-mix", "payroll-summary", "approved-leave-duration", "authorized-overtime-duration", "hotel-occupancy", "hotel-adr", "hotel-revpar", "hotel-night-audit-blockers", "project-actual-hours", "project-posted-cost",
  "identity-change-events", "quality-releases", "inspection-pass-rate", "opportunity-pipeline", "overdue-followups", "brand-profile-updates", "overdue-invoices",
  "marketing-engagement-events", "completed-manufacturing-orders", "manufacturing-orders-with-scrap", "production-cost",
  "recent-customer-activity", "asset-downtime", "exception-resolution-time",
  "planning-shortage-scenarios", "proposed-planning-recommendations", "capacity-feasible-recommendations",
  "completed-ecommerce-checkouts", "ecommerce-return-links", "ecommerce-fulfillment-links", "learning-completions",
  "application-stage-counts", "service-billing-releases", "field-visit-completion", "agent-invocation-outcomes", "asset-retirements", "collaboration-thread-activity",
]);
export const metricSupportsComparison = (id: MetricId) => periodComparableMetrics.has(id);

export function metricBreakdowns(id: MetricId): Breakdown[] {
  if (id === "hotel-occupancy") return ["default", "month", "branch", "property"];
  if (id === "hotel-adr" || id === "hotel-revpar") return ["default", "month", "branch", "property"];
  if (id === "hotel-folio-balance") return ["default", "branch", "property"];
  if (id === "hotel-night-audit-blockers") return ["default", "month", "status", "property"];
  if (id === "active-subscriptions") return ["default", "status", "plan"];
  if (id === "overdue-invoices") return ["default", "month", "status"];
  if (id === "inventory") return ["default", "warehouse"];
  if (id === "available-stock") return ["default", "product", "warehouse"];
  if (id === "inventory-shortages") return ["default", "product", "warehouse"];
  if (id === "inventory-movements") return ["default", "month", "movement-type", "warehouse", "unit"];
  if (id === "overdue-project-milestones") return ["default", "project", "owner", "age-band"];
  if (id === "project-dependency-blockers") return ["default", "project"];
  if (id === "project-actual-hours" || id === "project-posted-cost") return ["default", "month", "project"];
  if (id === "recent-customer-activity") return ["default", "month", "activity-type"];
  if (id === "collaboration-thread-activity") return ["default", "month", "activity-type"];
  if (id === "active-legal-obligations") return ["default", "status", "month"];
  if (id === "active-legal-holds") return ["default", "status"];
  if (id === "inspection-pass-rate") return ["default", "month", "branch"];
  if (id === "headcount") return ["default", "branch", "department", "level"];
  if (id === "approved-leave-duration" || id === "authorized-overtime-duration") return ["default", "month", "branch", "department"];
  if (id === "payroll-summary") return ["default", "month"];
  if (id === "cash-receipts") return ["default", "month"];
  if (id === "close-blockers") return ["default", "month", "status"];
  if (id === "asset-retirements") return ["default", "month", "branch"];
  if (["open-support-tickets", "overdue-support-sla"].includes(id)) return ["default", "status", "branch", "department"];
  if (id === "active-support-incidents") return ["default", "status", "severity", "branch", "department"];
  if (["active-assets", "asset-condition-location", "open-work-orders", "blocked-maintenance"].includes(id)) return ["default", "status", "branch", "department"];
  if (id === "asset-downtime") return ["default", "month", "asset", "branch", "department"];
  if (id === "open-operations-exceptions") return ["default", "status", "branch", "department", "age-band"];
  if (id === "exception-resolution-time") return ["default", "month"];
  if (id === "operations-escalations") return ["default", "status", "branch", "department", "age-band"];
  if (["quality-holds"].includes(id)) return ["default", "status", "branch"];
  if (["quality-releases", "identity-change-events"].includes(id)) return ["default", "month", "status"];
  if (["opportunity-pipeline"].includes(id)) return ["default", "month", "status", "branch"];
  if (["overdue-followups"].includes(id)) return ["default", "month", "status", "branch", "department"];
  if (["brand-profiles"].includes(id)) return ["default", "branch"];
  if (["brand-profile-updates"].includes(id)) return ["default", "month"];
  if (["active-assignments"].includes(id)) return ["default", "status", "branch", "department"];
  if (["pending-access-reviews"].includes(id)) return ["default", "status", "branch", "department"];
  if (["active-organization-nodes", "pending-structure-changes"].includes(id)) return ["default", "status", "branch"];
  if (["marketing-lead-candidates"].includes(id)) return ["default", "status", "month", "branch", "department"];
  if (["marketing-engagement-events"].includes(id)) return ["default", "month", "status", "branch", "department"];
  if (["completed-manufacturing-orders", "manufacturing-orders-with-scrap"].includes(id)) return ["default", "month", "branch"];
  if (id === "production-cost") return ["default", "month", "branch", "product", "warehouse"];
  if (["planning-shortage-scenarios", "proposed-planning-recommendations", "capacity-feasible-recommendations"].includes(id)) return ["default", "status", "month", "branch"];
  if (["open-ecommerce-carts"].includes(id)) return ["default", "status", "month", "branch", "department"];
  if (["completed-ecommerce-checkouts", "ecommerce-return-links", "ecommerce-fulfillment-links"].includes(id)) return ["default", "month", "status", "branch", "department"];
  if (["active-benefit-elections"].includes(id)) return ["default", "status"];
  if (["published-benefit-plans"].includes(id)) return ["default"];
  if (["learning-completions"].includes(id)) return ["default", "month", "branch", "department"];
  if (["overdue-required-learning"].includes(id)) return ["default", "status", "branch", "department"];
  if (["open-requisitions"].includes(id)) return ["default", "status", "branch", "department"];
  if (["application-stage-counts"].includes(id)) return ["default", "status", "branch", "department"];
  if (["open-service-orders"].includes(id)) return ["default", "status", "branch", "department"];
  if (id === "field-visit-completion") return ["default", "month", "status", "branch", "department", "technician"];
  if (["service-billing-releases"].includes(id)) return ["default", "month", "branch", "department"];
  if (["approved-agent-versions"].includes(id)) return ["default"];
  if (["agent-invocation-outcomes"].includes(id)) return ["default", "month", "status"];
  if (id === "sales-fulfillment-rate") return ["default", "month", "status", "branch"];
  if (["sales-cancellations", "sales-cancellation-value"].includes(id)) return ["default", "month", "branch"];
  if (["sales-orders", "purchase-orders", "pos-sales", "open-po-receipts", "invoice-match-exceptions"].includes(id)) return ["default", "month", "status", "branch"];
  if (id === "net-pos-sales") return ["default", "month", "branch"];
  if (id === "pos-payment-mix") return ["default", "month", "branch"];
  if (["receivables", "payables", "invoiced-sales"].includes(id)) return ["default", "month", "status"];
  return ["default"];
}

export function metricCatalog(a: WorkContext): MetricInfo[] {
  return metricIds.map((id) => {
    const spec = specs[id];
    const accessAvailable = hasPermission(a, spec.permission)
      && (spec.requiredPermissions ?? []).every((permission) => hasPermission(a, permission))
      && (!spec.holdingOwnerOnly || a.role === "holding_owner" && a.assignmentScope === "holding")
      && (!spec.moduleGate || hasModuleAccess(a.modules, spec.moduleGate))
      && !(spec.companyOnly && (a.branchId || a.departmentId))
      && !(spec.betaScopeUnsupported && a.beta)
      && !(a.departmentId && (["pos-sales", "hotel-rooms"].includes(id) || spec.departmentScopeUnsupported));
    const engineeringPreview = process.env.NODE_ENV !== "production";
    const available = accessAvailable && (!spec.acceptancePending || engineeringPreview);
    const reason = !accessAvailable
      ? "The selected work role or scope does not allow this metric."
      : "The adapter is implemented, but source reconciliation and business acceptance are pending.";
    return {
      id,
      label: spec.label,
      description: spec.description,
      unit: spec.unit,
      dateBasis: spec.dateBasis,
      available: !!available,
      ...(available && !spec.acceptancePending ? {} : { reason }),
      acceptancePending: !!spec.acceptancePending,
      breakdowns: metricBreakdowns(id),
      supportsComparison: metricSupportsComparison(id),
    };
  });
}

/** Apply the company pilot gate to metadata as well as reads in production. */
export async function metricCatalogFor(db: SqlDatabase, a: WorkContext): Promise<MetricInfo[]> {
  let catalog = metricCatalog(a);
  if (catalog.some((metric) => (metric.id === "brand-profiles" || metric.id === "brand-profile-updates") && metric.available)
    && !await companyAuthority(db, a)) {
    catalog = catalog.map((metric) => metric.id === "brand-profiles" || metric.id === "brand-profile-updates"
      ? { ...metric, available: false, reason: "Company organization-management authority is required for Brand profile records." }
      : metric);
  }
  if (process.env.NODE_ENV !== "production") return catalog;
  const enabled = new Set((await db.prepare(`SELECT module_id FROM company_feature_rollouts
    WHERE company_id=? AND rollout_state IN ('pilot','available')`).bind(a.companyId).all<{ module_id: string }>()).results.map((row) => row.module_id));
  return catalog.map((metric) => {
    const moduleId = specs[metric.id].moduleId;
    if (metric.acceptancePending) return { ...metric, available: false, reason: "Source reconciliation and business acceptance are still pending." };
    if (enabled.has(moduleId) && ["organization", "identity_access", "people_payroll", "operations", "procurement", "inventory", "finance", "pos", "pms", "collaboration"].includes(moduleId)) return metric;
    return { ...metric, available: false, reason: "This module is outside the approved production pilot or is not enabled for this company." };
  });
}

function exactInteger(value: string | number | bigint): bigint {
  if (typeof value === "bigint") return value;
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) throw new ApiError(500, "METRIC_VALUE_NOT_EXACT", "The source returned a value that cannot be represented exactly.");
    return BigInt(value);
  }
  if (!/^-?\d+$/.test(value)) throw new ApiError(500, "METRIC_VALUE_INVALID", "The source returned an invalid metric value.");
  return BigInt(value);
}

function sumExact(values: string[]): string {
  return String(values.reduce((sum, value) => sum + exactInteger(value), BigInt(0)));
}

function percentHundredths(numerator: string | number | bigint, denominator: string | number | bigint): string {
  const n = exactInteger(numerator), d = exactInteger(denominator);
  if (n < BigInt(0) || d < BigInt(0) || n > d) throw new ApiError(500, "METRIC_RATIO_INVALID", "The source returned an invalid rate numerator or denominator.");
  if (d === BigInt(0)) return "0";
  return String((n * BigInt(10000) + d / BigInt(2)) / d);
}

function moneyRatioMinor(numerator: string | number | bigint, denominator: string | number | bigint): string {
  const n = exactInteger(numerator), d = exactInteger(denominator);
  if (d < BigInt(0)) throw new ApiError(500, "METRIC_RATIO_INVALID", "The source returned an invalid rate denominator.");
  if (d === BigInt(0)) return "0";
  const sign = n < BigInt(0) ? BigInt(-1) : BigInt(1);
  const magnitude = n < BigInt(0) ? -n : n;
  return String(sign * ((magnitude + d / BigInt(2)) / d));
}

/** Read only explicit adapters. Totals and grouped values are cast to text before the shared SQL parser. */
export async function queryMetric(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown = "default"): Promise<MetricData> {
  const spec = specs[id];
  if (!metricBreakdowns(id).includes(breakdown)) throw new ApiError(422, "BREAKDOWN_UNAVAILABLE", "This metric does not support that grouping.");
  if (!(await metricCatalogFor(db, a)).find((metric) => metric.id === id)?.available) throw new ApiError(403, "METRIC_FORBIDDEN", "This metric is unavailable in the current source permissions, acceptance status or scope.");
  await assertProductionModuleEnabled(db, a.companyId, spec.moduleId);
  const sourceDb = spec.holdingOwnerOnly ? (await runtimeEnv()).CONTROL_DB : db;

  const scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`;
  const scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
  let sql = "";
  let values: unknown[] = [];
  let referenceAtOverride: number | null = null;
  let metricTotalOverride: string | null = null;
  let metricPartialOverride = false;
  let metricEmptyOverride = false;
  if (id === "active-subscriptions") {
    const label = breakdown === "status" ? "status" : breakdown === "plan" ? "plan" : "'Active subscriptions'", now = Date.now();
    sql = `WITH subscriptions AS (
        SELECT COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status
        FROM billing_enrollments e
        LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
        WHERE e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL
          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)
        UNION ALL
        SELECT 'Legacy subscription' plan,b.status
        FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
          AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)
      ) SELECT ${label} label,count(*)::text value FROM subscriptions GROUP BY 1 ORDER BY 1`;
    values = [a.holdingId, now, now, a.holdingId, now];
  } else if (id === "overdue-invoices") {
    const label = breakdown === "month" ? "to_char(to_timestamp(i.due_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : breakdown === "status" ? "i.status" : "'Overdue subscription invoices'";
    sql = `SELECT ${label} label,count(*)::text value FROM billing_invoices i
      WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?
      GROUP BY 1 ORDER BY 1`;
    values = [a.holdingId, Date.parse(f.from), Date.parse(f.to) + 86400000, Date.now()];
  } else if (["receivables", "payables", "invoiced-sales"].includes(id)) {
    const sales = id === "invoiced-sales";
    sql = `SELECT ${breakdown === "month" || breakdown === "default" && sales ? "substring(invoice_date,1,7)" : "status"} label,sum(${sales ? "total_minor" : "outstanding_minor"})::text value FROM finance_invoices WHERE company_id=? AND kind=? AND currency=? AND invoice_date BETWEEN ? AND ? AND status IN (${sales ? "'posted','partially_paid','paid'" : "'posted','partially_paid'"}) GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, id === "payables" ? "payable" : "receivable", f.currency, f.from, f.to];
  } else if (id === "cash-receipts") {
    const label = breakdown === "month" || breakdown === "default"
      ? "to_char(to_timestamp(j.posted_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : "'Posted receipts'";
    sql = `SELECT ${label} label,sum(p.amount_minor)::text value FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id
      WHERE p.company_id=? AND p.kind='receipt' AND p.currency=? AND p.status IN ('released','settled') AND j.status='posted'
      AND j.posted_at>=? AND j.posted_at<? AND NOT EXISTS(SELECT 1 FROM finance_payment_reversals r WHERE r.payment_id=p.id)
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, f.currency, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "close-blockers") {
    const label = breakdown === "month"
      ? "substring(latest.end_date,1,7)"
      : breakdown === "status" ? "latest.check_key" : "'Open close blockers'";
    sql = `WITH current_checks AS (
        SELECT DISTINCT ON (c.period_id,c.check_key) c.period_id,c.check_key,c.status,c.resolved_at,c.checked_at,c.id,p.end_date,p.state
        FROM finance_close_checks c JOIN finance_periods p ON p.id=c.period_id
        WHERE c.company_id=? AND p.company_id=? AND p.end_date BETWEEN ? AND ? AND p.state IN ('open','closing')
        ORDER BY c.period_id,c.check_key,c.checked_at DESC,c.id DESC
      )
      SELECT ${label} label,count(*)::text value FROM current_checks latest
      WHERE latest.status='block' AND latest.resolved_at IS NULL GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.companyId, f.from, f.to];
  } else if (id === "sales-fulfillment-rate") {
    const label = breakdown === "month" ? "substring(o.order_date,1,7)"
      : breakdown === "status" ? "o.state"
        : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')"
          : "'Eligible sales orders'";
    sql = `SELECT ${label} label,count(*) FILTER (WHERE o.state='fulfilled')::text numerator,count(*)::text denominator
      FROM commerce_orders o WHERE ${scopedAlias("o")} AND o.kind='sales'
        AND o.state IN ('confirmed','partially_fulfilled','fulfilled') AND o.order_date BETWEEN ? AND ?
        AND EXISTS(SELECT 1 FROM commerce_order_lines l WHERE l.order_id=o.id)
      GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, f.from, f.to];
  } else if (id === "sales-cancellations" || id === "sales-cancellation-value") {
    const label = breakdown === "branch"
      ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')"
      : breakdown === "month"
        ? "to_char(to_timestamp(o.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
        : id === "sales-cancellations" ? "'Cancelled sales orders'" : "'Confirmed cancellation value'";
    const amount = id === "sales-cancellations" ? "count(*)::text" : "sum(o.subtotal_minor::numeric+o.tax_minor::numeric)::text";
    const approved = id === "sales-cancellation-value" ? "AND o.approved_by IS NOT NULL AND o.currency=?" : "";
    sql = `SELECT ${label} label,${amount} value FROM commerce_orders o
      WHERE ${scopedAlias("o")} AND o.kind='sales' AND o.state='cancelled' ${approved}
        AND o.updated_at>=? AND o.updated_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, ...(id === "sales-cancellation-value" ? [f.currency] : []), Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "sales-orders" || id === "purchase-orders") {
    sql = `SELECT ${breakdown === "status" ? "o.state" : breakdown === "branch" ? "COALESCE((SELECT display_name FROM organization_nodes WHERE id=o.branch_id AND company_id=o.company_id),'Company-wide')" : "substring(o.order_date,1,7)"} label,sum(o.subtotal_minor+o.tax_minor)::text value FROM commerce_orders o WHERE ${scopedAlias("o")} AND o.kind=? AND o.currency=? AND o.order_date BETWEEN ? AND ? AND o.state IN ('confirmed','partially_fulfilled','fulfilled') GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, id === "sales-orders" ? "sales" : "purchase", f.currency, f.from, f.to];
  } else if (id === "open-po-receipts") {
    const label = breakdown === "status" ? "o.state"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')"
        : breakdown === "month" ? "substring(o.due_date,1,7)" : "'Open PO receipts'";
    sql = `SELECT ${label} label,count(*)::text value FROM commerce_orders o
      WHERE ${scopedAlias("o")} AND o.kind='purchase' AND o.state IN ('confirmed','partially_fulfilled')
        AND o.due_date BETWEEN ? AND ?
        AND EXISTS(SELECT 1 FROM commerce_order_lines l WHERE l.order_id=o.id AND l.quantity_milli>l.fulfilled_milli)
      GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, f.from, f.to];
  } else if (id === "invoice-match-exceptions") {
    const label = breakdown === "status"
      ? "CASE WHEN latest.status='returned' THEN 'Returned for correction' WHEN NOT latest.quantity_matches THEN 'Quantity variance' ELSE 'Subtotal variance' END"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=latest.branch_id AND n.company_id=latest.company_id),'Company-wide')"
        : breakdown === "month" ? "substring(latest.invoice_date,1,7)" : "'Supplier invoice match exceptions'";
    sql = `SELECT ${label} label,count(*)::text value FROM procurement_bill_matches latest
      WHERE ${scopedAlias("latest")} AND latest.invoice_date BETWEEN ? AND ? AND latest.currency=?
        AND (
          (latest.status='review' AND (NOT latest.quantity_matches OR latest.subtotal_minor<>latest.expected_subtotal_minor))
          OR (latest.status='returned' AND NOT EXISTS(
            SELECT 1 FROM procurement_bill_match_receipts returned_receipt
            JOIN procurement_bill_match_receipts replacement_receipt
              ON replacement_receipt.fulfillment_id=returned_receipt.fulfillment_id AND replacement_receipt.active=TRUE
            WHERE returned_receipt.match_id=latest.id AND replacement_receipt.match_id<>latest.id
          ))
        )
      GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, f.from, f.to, f.currency];
  } else if (id === "headcount") {
    const label = breakdown === "branch" || breakdown === "department"
      ? `COALESCE((SELECT display_name FROM organization_nodes WHERE id=e.${breakdown}_id AND company_id=e.company_id),'Unassigned')`
      : breakdown === "level" ? "COALESCE((SELECT 'Level '||level_number||' · '||name FROM staff_levels WHERE id=e.level_id AND company_id=e.company_id),'Unmapped')" : "'Active people'";
    sql = `WITH effective AS (SELECT DISTINCT ON (p.id) p.id person_id,e.* FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE ${scopedAlias("e")} AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY p.id,e.start_date DESC,e.id) SELECT ${label} label,count(*)::text value FROM effective e GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "approved-leave-duration") {
    const label = breakdown === "branch"
      ? "COALESCE((SELECT display_name FROM organization_nodes WHERE id=l.branch_id AND company_id=l.company_id),'Unassigned branch')"
      : breakdown === "department"
        ? "COALESCE((SELECT display_name FROM organization_nodes WHERE id=l.department_id AND company_id=l.company_id),'Unassigned department')"
        : "substring(l.start_date,1,7)";
    const readable = `(l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")})`;
    sql = `SELECT ${label} label,'days' unit,COALESCE(sum(COALESCE(l.calculated_units::numeric,l.days::numeric)*1000)::bigint,0)::text value
      FROM leave_requests l WHERE l.company_id=? AND l.state='approved' AND l.start_date BETWEEN ? AND ?
        AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND ${readable}
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, f.from, f.to, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, a.userId, a.assignmentId, a.userId, a.assignmentId];
  } else if (id === "authorized-overtime-duration") {
    const label = breakdown === "branch"
      ? "COALESCE((SELECT display_name FROM organization_nodes WHERE id=o.branch_id AND company_id=o.company_id),'Unassigned branch')"
      : breakdown === "department"
        ? "COALESCE((SELECT display_name FROM organization_nodes WHERE id=o.department_id AND company_id=o.company_id),'Unassigned department')"
        : "substring(h.work_date,1,7)";
    const readable = `(o.created_by=? OR ${participantSql("overtime", "o", "?", "?")})`;
    sql = `SELECT ${label} label,sum(h.minutes::numeric*60000)::bigint::text value
      FROM overtime_payroll_handoffs h JOIN overtime_requests o ON o.id=h.request_id AND o.company_id=h.company_id
      WHERE o.company_id=? AND o.state='approved' AND h.work_date BETWEEN ? AND ?
        AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND ${readable}
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, f.from, f.to, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, a.assignmentId];
  } else if (id === "payroll-summary") {
    const label = breakdown === "month" ? "substring(r.period_end,1,7)" : "'Posted net payroll'";
    sql = `SELECT ${label} label,COALESCE(sum(i.net_minor::numeric),0)::text value
      FROM hr_payroll_runs r
      JOIN hr_payroll_items i ON i.run_id=r.id AND i.status='posted'
      WHERE r.company_id=? AND r.currency=? AND r.period_end BETWEEN ? AND ? AND r.status='posted'
        AND EXISTS(SELECT 1 FROM hr_payroll_postings p JOIN finance_journals j ON j.id=p.journal_id
          WHERE p.run_id=r.id AND j.company_id=r.company_id AND j.status='posted')
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, f.currency, f.from, f.to];
  } else if (id === "my-tasks") {
    sql = `SELECT t.state label,count(*)::text value FROM staff_tasks t WHERE ${scopedAlias("t")} AND t.assignee_user_id=? AND t.state NOT IN ('draft','completed','cancelled') AND t.due_at>=? AND t.due_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, a.userId, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (operationsMetrics.includes(id as OperationsMetricId)) {
    const metric = id as OperationsMetricId, now = Date.now();
    referenceAtOverride = now;
    const visibility = operationsTaskVisibility(a, now), cohort = operationsTaskCohort(metric, "t", a, f, now);
    if (metric === "exception-resolution-time") {
      const label = breakdown === "month"
        ? `to_char(to_timestamp(t.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')`
        : "'Median overdue time to completion'";
      sql = `WITH visible_tasks AS (
          SELECT t.* FROM staff_tasks t WHERE ${visibility.sql} AND ${cohort.sql}
        ), grouped AS (
          SELECT ${label} label,
            ROUND(percentile_cont(0.5) WITHIN GROUP (ORDER BY (t.updated_at-t.due_at)::double precision))::bigint::text value
          FROM visible_tasks t GROUP BY 1
        ), overall AS (
          SELECT count(*)::text count_value,
            COALESCE(ROUND(percentile_cont(0.5) WITHIN GROUP (ORDER BY (updated_at-due_at)::double precision))::bigint,0)::text total
          FROM visible_tasks
        )
        SELECT grouped.label,grouped.value,overall.total,overall.count_value FROM overall LEFT JOIN grouped ON TRUE ORDER BY grouped.label`;
      values = [...visibility.values, ...cohort.values];
    } else {
      const ageBand = `CASE WHEN t.due_at>=${now}-86400000 THEN 'Less than 1 day'
        WHEN t.due_at>=${now}-604800000 THEN '1–7 days'
        WHEN t.due_at>=${now}-2592000000 THEN '8–30 days' ELSE 'Over 30 days' END`;
      const label = breakdown === "status" ? "t.state"
        : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=t.branch_id AND n.company_id=t.company_id),'Company-wide')"
          : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=t.department_id AND n.company_id=t.company_id),'No department')"
            : breakdown === "age-band" ? ageBand
              : metric === "operations-escalations" ? "'Overdue escalations sent to current reader'"
                : "'Overdue operational tasks'";
      sql = `SELECT ${label} label,count(*)::text value FROM staff_tasks t
        WHERE ${visibility.sql} AND ${cohort.sql} GROUP BY 1 ORDER BY 1`;
      values = [...visibility.values, ...cohort.values];
    }
  } else if (id === "inventory") {
    sql = "SELECT w.name label,count(*)::text value FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND s.quantity_milli>0 GROUP BY w.id,w.name ORDER BY 1";
    values = [a.companyId];
  } else if (id === "available-stock") {
    const label = breakdown === "product" ? "p.sku||' · '||p.name"
      : breakdown === "warehouse" ? "w.name"
        : "p.sku||' · '||p.name||' · '||w.name";
    const groupId = breakdown === "product" ? "p.id" : breakdown === "warehouse" ? "w.id" : "p.id||':'||w.id";
    sql = `WITH reservations AS (
        SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
        FROM inventory_active_reservations GROUP BY warehouse_id,product_id
      ), positions AS (
        SELECT w.id warehouse_id,w.name warehouse_name,w.branch_id,p.id product_id,p.sku,p.name product_name,p.unit,
          ${groupId} group_id,${label} group_label,
          GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0) available_milli
        FROM commerce_stock s
        JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=? AND w.status='active'
        JOIN commerce_products p ON p.id=s.product_id AND p.company_id=w.company_id AND p.status='active'
        LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
        WHERE (?::text IS NULL OR w.branch_id=?)
      )
      SELECT min(group_label) label,unit,sum(available_milli)::text value FROM positions
      WHERE available_milli>0
      GROUP BY group_id,unit ORDER BY 1,2 LIMIT 101`;
    values = [a.companyId, a.branchId, a.branchId];
  } else if (id === "inventory-shortages") {
    const label = breakdown === "warehouse" ? "w.name" : breakdown === "product" ? "p.sku||' · '||p.name" : "'Inventory shortages'";
    const groupId = breakdown === "warehouse" ? "w.id" : breakdown === "product" ? "p.id" : "'all'";
    sql = `WITH reservations AS (
        SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
        FROM inventory_active_reservations GROUP BY warehouse_id,product_id
      ), positions AS (
        SELECT r.id,r.version,w.id warehouse_id,w.name warehouse_name,p.id product_id,p.sku,p.name product_name,p.unit,
          r.current_reorder_point_milli::numeric reorder_point_milli,
          GREATEST(COALESCE(s.quantity_milli,0)::numeric-GREATEST(COALESCE(res.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0) available_milli,
          ${groupId} group_id,${label} group_label
        FROM inventory_reorder_policies r
        JOIN commerce_warehouses w ON w.id=r.warehouse_id AND w.company_id=r.company_id AND w.status='active'
        JOIN commerce_products p ON p.id=r.product_id AND p.company_id=r.company_id AND p.status='active'
        LEFT JOIN commerce_stock s ON s.warehouse_id=w.id AND s.product_id=p.id
        LEFT JOIN reservations res ON res.warehouse_id=w.id AND res.product_id=p.id
        WHERE r.company_id=? AND r.current_reorder_point_milli IS NOT NULL AND r.retired_at IS NULL
          AND (?::text IS NULL OR w.branch_id=?)
      )
      SELECT min(group_label) label,count(*) FILTER(WHERE available_milli<reorder_point_milli)::text value,count(*)::text configured_count
      FROM positions GROUP BY group_id ORDER BY label LIMIT 101`;
    values = [a.companyId, a.branchId, a.branchId];
  } else if (id === "inventory-movements") {
    const label = breakdown === "movement-type" ? "l.source_type"
      : breakdown === "warehouse" ? "w.name"
        : breakdown === "unit" ? "p.unit"
          : "substring(l.movement_date,1,7)";
    sql = `SELECT ${label} label,p.unit,sum(l.quantity_milli::numeric)::text value
      FROM inventory_ledger l
      JOIN commerce_warehouses w ON w.id=l.warehouse_id AND w.company_id=?
      JOIN commerce_products p ON p.id=l.product_id AND p.company_id=w.company_id
      WHERE (?::text IS NULL OR w.branch_id=?) AND l.movement_date BETWEEN ? AND ? AND l.quantity_milli<>0
      GROUP BY 1,p.unit ORDER BY 1,2 LIMIT 101`;
    values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
  } else if (id === "pos-sales") {
    sql = `SELECT ${breakdown === "status" ? "s.state" : breakdown === "branch" ? "COALESCE((SELECT display_name FROM organization_nodes WHERE id=s.branch_id AND company_id=s.company_id),'Company-wide')" : "substring(s.business_date,1,7)"} label,sum(s.total_minor)::text value FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=? AND s.business_date BETWEEN ? AND ? AND s.state='completed' GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to];
  } else if (id === "net-pos-sales") {
    const label = breakdown === "branch"
      ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=x.branch_id AND n.company_id=x.company_id),'Company-wide')"
      : "substring(x.event_date,1,7)";
    sql = `WITH sales AS (
        SELECT s.company_id,s.branch_id,s.business_date event_date,s.total_minor amount_minor
        FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
          AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      ), refunds AS (
        SELECT s.company_id,s.branch_id,r.accounting_date event_date,-r.amount_minor amount_minor
        FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
        WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
          AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed'
      ), net_events AS (SELECT * FROM sales UNION ALL SELECT * FROM refunds)
      SELECT ${label} label,COALESCE(sum(x.amount_minor),0)::text value FROM net_events x GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to, a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to];
  } else if (id === "pos-payment-mix") {
    const label = breakdown === "branch"
      ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=s.branch_id AND n.company_id=s.company_id),'Company-wide')"
      : breakdown === "month" ? "substring(s.business_date,1,7)" : "p.method";
    sql = `SELECT ${label} label,COALESCE(sum(p.amount_minor-COALESCE((SELECT sum(rp.amount_minor) FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)),0)::text value
      FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
      WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=? AND s.business_date BETWEEN ? AND ?
        AND s.state='completed' AND p.state='succeeded' GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to];
  } else if (id === "hotel-rooms") {
    sql = "SELECT p.name label,count(*)::text value FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='room' GROUP BY p.id,p.name ORDER BY 1";
    values = [a.companyId, a.branchId, a.branchId];
  } else if (id === "hotel-occupancy") {
    const label = breakdown === "month" ? "substring(r.data_json::jsonb->>'date',1,7)"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=p.branch_id AND n.company_id=p.company_id),'Unassigned branch')"
        : breakdown === "property" ? "p.name" : "'Hotel occupancy'";
    const capacity = "(r.data_json::jsonb->'roomMetrics'->>'capacity')";
    const sold = "(r.data_json::jsonb->'roomMetrics'->>'sold')";
    const valid = `(CASE WHEN ${capacity} ~ '^[1-9][0-9]{0,8}$' AND ${sold} ~ '^(0|[1-9][0-9]{0,8})$' THEN CASE WHEN ${capacity}::numeric>0 THEN ${sold}::numeric<=${capacity}::numeric ELSE FALSE END ELSE FALSE END)`;
    sql = `SELECT ${label} label,
        COALESCE(sum(CASE WHEN ${valid} THEN ${sold}::numeric ELSE 0::numeric END),0)::text numerator,
        COALESCE(sum(CASE WHEN ${valid} THEN ${capacity}::numeric ELSE 0::numeric END),0)::text denominator,
        count(*) FILTER (WHERE NOT ${valid})::text partial_count
      FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='night'
        AND r.data_json::jsonb->>'state'='completed' AND r.data_json::jsonb->>'date' BETWEEN ? AND ?
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
  } else if (id === "hotel-adr" || id === "hotel-revpar") {
    const label = breakdown === "month" ? "substring(n.business_date,1,7)"
      : breakdown === "branch" ? "COALESCE((SELECT n2.display_name FROM organization_nodes n2 WHERE n2.id=p.branch_id AND n2.company_id=p.company_id),'Unassigned branch')"
        : breakdown === "property" ? "p.name" : id === "hotel-adr" ? "'Hotel ADR'" : "'Hotel RevPAR'";
    const capacity = "(r.data_json::jsonb->'roomMetrics'->>'capacity')";
    const sold = "(r.data_json::jsonb->'roomMetrics'->>'sold')";
    const validMetrics = `(CASE WHEN ${capacity} ~ '^[1-9][0-9]{0,8}$' AND ${sold} ~ '^(0|[1-9][0-9]{0,8})$' THEN CASE WHEN ${capacity}::numeric>0 THEN ${sold}::numeric<=${capacity}::numeric ELSE FALSE END ELSE FALSE END)`;
    const denominator = id === "hotel-adr" ? "n.sold" : "n.capacity";
    sql = `WITH RECURSIVE scoped_properties AS (
        SELECT id,company_id,branch_id,name,config_json FROM pms_properties
        WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND config_json::jsonb->>'currency'=?
      ), candidate_entries AS (
        SELECT e.id,e.property_id,e.data_json::jsonb->>'date' business_date,
          e.data_json::jsonb->>'kind' entry_kind,e.data_json::jsonb->>'net' net_raw,
          e.data_json::jsonb->>'sourceKey' source_key,e.data_json::jsonb->>'originalId' original_id
        FROM pms_records e JOIN scoped_properties p ON p.id=e.property_id
        WHERE e.kind='entry' AND e.data_json::jsonb->>'date' BETWEEN ? AND ?
          AND e.data_json::jsonb->>'kind' IN ('charge','adjustment')
      ), entry_lineage(entry_id,property_id,business_date,net_raw,current_id,source_key,original_id,visited) AS (
        SELECT id,property_id,business_date,net_raw,id,source_key,original_id,ARRAY[id]::text[] FROM candidate_entries
        UNION ALL
        SELECT l.entry_id,l.property_id,l.business_date,l.net_raw,parent.id,
          parent.data_json::jsonb->>'sourceKey',parent.data_json::jsonb->>'originalId',l.visited||parent.id
        FROM entry_lineage l JOIN pms_records parent ON parent.id=l.original_id AND parent.property_id=l.property_id AND parent.kind='entry'
        WHERE COALESCE(l.source_key,'') NOT LIKE 'night:%' AND NOT (parent.id=ANY(l.visited))
      ), room_revenue AS (
        SELECT e.property_id,e.business_date,
          COALESCE(sum(CASE WHEN e.net_raw ~ '^-?(0|[1-9][0-9]*)$' THEN e.net_raw::numeric ELSE 0::numeric END),0) revenue_minor,
          count(*) FILTER (WHERE e.net_raw IS NULL OR e.net_raw !~ '^-?(0|[1-9][0-9]*)$') invalid_revenue_count
        FROM candidate_entries e
        WHERE EXISTS(SELECT 1 FROM entry_lineage l WHERE l.entry_id=e.id AND l.source_key LIKE 'night:%')
        GROUP BY e.property_id,e.business_date
      ), audit_nights AS (
        SELECT r.id,r.property_id,p.branch_id,p.name,r.data_json::jsonb->>'date' business_date,
          CASE WHEN ${capacity} ~ '^[1-9][0-9]{0,8}$' THEN ${capacity}::numeric ELSE 0::numeric END capacity,
          CASE WHEN ${sold} ~ '^(0|[1-9][0-9]{0,8})$' THEN ${sold}::numeric ELSE 0::numeric END sold,
          ${validMetrics} valid_metrics
        FROM pms_records r JOIN scoped_properties p ON p.id=r.property_id
        WHERE r.kind='night' AND r.data_json::jsonb->>'state'='completed'
          AND r.data_json::jsonb->>'date' BETWEEN ? AND ?
      )
      SELECT ${label} label,
        COALESCE(sum(CASE WHEN n.valid_metrics THEN COALESCE(rr.revenue_minor,0::numeric) ELSE 0::numeric END),0)::text numerator,
        COALESCE(sum(CASE WHEN n.valid_metrics THEN ${denominator} ELSE 0::numeric END),0)::text denominator,
        (count(*) FILTER (WHERE NOT n.valid_metrics)+COALESCE(sum(CASE WHEN n.valid_metrics THEN COALESCE(rr.invalid_revenue_count,0) ELSE 0 END),0))::text partial_count
      FROM audit_nights n JOIN scoped_properties p ON p.id=n.property_id
      LEFT JOIN room_revenue rr ON rr.property_id=n.property_id AND rr.business_date=n.business_date
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to, f.from, f.to];
  } else if (id === "hotel-folio-balance") {
    const label = breakdown === "branch"
      ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=p.branch_id AND n.company_id=p.company_id),'Unassigned branch')"
      : breakdown === "property" ? "p.name" : "'Open folio balances'";
    sql = `SELECT ${label} label,
        COALESCE(sum(l.debit_minor::numeric-l.credit_minor::numeric),0)::text value
      FROM pms_properties p
      JOIN pms_records f ON f.property_id=p.id AND f.kind='folio'
        AND f.data_json::jsonb->>'state' IN ('open','credit')
      JOIN pms_records e ON e.property_id=p.id AND e.kind='entry'
        AND e.data_json::jsonb->>'folioId'=f.id
      JOIN pms_postings pp ON pp.property_id=p.id AND pp.record_id=e.id
      JOIN finance_journals j ON j.id=pp.journal_id AND j.status='posted'
      JOIN finance_journal_lines l ON l.journal_id=j.id
        AND l.account_id=p.config_json::jsonb->'accounts'->>'receivable'
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?)
        AND p.config_json::jsonb->>'currency'=?
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.currency];
  } else if (id === "hotel-night-audit-blockers") {
    const label = breakdown === "month" ? "substring(j.business_date,1,7)"
      : breakdown === "status" ? "COALESCE(j.error_code,'PMS_JOB_FAILED')"
        : breakdown === "property" ? "p.name" : "'Blocked Night Audit jobs'";
    sql = `SELECT ${label} label,count(*)::text value
      FROM pms_jobs j JOIN pms_properties p ON p.id=j.property_id
      JOIN pms_records r ON r.id=j.id AND r.property_id=j.property_id AND r.kind='night'
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?)
        AND j.business_date BETWEEN ? AND ? AND j.state='blocked'
        AND r.data_json::jsonb->>'date'=j.business_date AND r.data_json::jsonb->>'state'='blocked'
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
  } else if (id === "projects") {
    sql = `SELECT to_char(to_timestamp(p.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*)::text value FROM staff_projects p WHERE ${scopedAlias("p")} AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)) AND p.created_at>=? AND p.created_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, a.userId, a.userId, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "overdue-project-milestones") {
    referenceAtOverride = Date.now();
    const ageBand = `CASE WHEN ${referenceAtOverride}-m.due_at<604800000 THEN '0-7 days' WHEN ${referenceAtOverride}-m.due_at<2592000000 THEN '8-30 days' ELSE 'Over 30 days' END`;
    const label = breakdown === "project" ? "p.title"
      : breakdown === "owner" ? "COALESCE(h.legal_name,m.owner_user_id)"
        : breakdown === "age-band" ? ageBand : "'Overdue project milestones'";
    const group = breakdown === "project" ? "p.id"
      : breakdown === "owner" ? "m.owner_user_id"
        : breakdown === "age-band" ? ageBand : "1";
    const cohort = projectMilestoneOverdueCohort(a, f, referenceAtOverride);
    const grouped = breakdown === "default" ? "" : `GROUP BY ${group}`;
    sql = `SELECT min(${label}) label,count(*)::text value FROM staff_project_milestones m
      JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
      LEFT JOIN hr_people h ON h.company_id=m.company_id AND h.user_id=m.owner_user_id AND h.merged_into_id IS NULL
      WHERE ${cohort.sql} ${grouped} ORDER BY 1 LIMIT 101`;
    values = cohort.values;
  } else if (id === "project-dependency-blockers") {
    const label = breakdown === "project" ? "p.title" : "'Blocked project dependencies'";
    const cohort = projectDependencyBlockerCohort(a, f);
    const grouping = breakdown === "project" ? "GROUP BY p.id" : "";
    sql = `SELECT min(${label}) label,count(*)::text value,count(*)::text count_value
      FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
      WHERE ${cohort.sql} ${grouping} ORDER BY 1 LIMIT 101`;
    values = cohort.values;
  } else if (id === "project-actual-hours") {
    const label = breakdown === "month" ? "substring(l.work_date::text,1,7)"
      : breakdown === "project" ? "p.title" : "'Tracked project hours'";
    const group = breakdown === "month" ? "substring(l.work_date::text,1,7)"
      : breakdown === "project" ? "p.id" : null;
    const grouping = group ? `GROUP BY ${group}` : "";
    sql = `SELECT min(${label}) label,(sum(l.minutes::numeric)*60000)::text value,count(*)::text count_value
      FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
        AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))
        AND l.state='approved' AND l.work_date BETWEEN ? AND ?
      ${grouping} ORDER BY 1 LIMIT 101`;
    values = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, f.from, f.to];
  } else if (id === "project-posted-cost") {
    const label = breakdown === "month" ? "substring(j.entry_date::text,1,7)"
      : breakdown === "project" ? "p.title" : "'Posted project cost'";
    const group = breakdown === "month" ? "substring(j.entry_date::text,1,7)"
      : breakdown === "project" ? "p.id" : null;
    const grouping = group ? `GROUP BY ${group}` : "";
    sql = `SELECT min(${label}) label,sum(a.amount_minor::numeric)::text value,count(*)::text count_value
      FROM staff_project_cost_allocations a JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id
      JOIN finance_journal_lines l ON l.id=a.journal_line_id JOIN finance_journals j ON j.id=l.journal_id
      JOIN finance_accounts ac ON ac.id=l.account_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
        AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))
        AND a.state='approved' AND j.status='posted' AND ac.category IN ('expense','other_expense')
        AND j.currency=? AND j.entry_date BETWEEN ? AND ?
      ${grouping} ORDER BY 1 LIMIT 101`;
    values = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, a.userId, a.userId, f.currency, f.from, f.to];
  } else if (id === "routines") {
    sql = `SELECT 'Published' label,count(*)::text value FROM routine_templates t WHERE ${scopedAlias("t")} AND t.assignee_user_id=? AND t.published=1`;
    values = [...scopeArgs, a.userId];
  } else if (id === "active-customers") {
    sql = "SELECT 'Active customers' label,count(*)::text value FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND merged_into_id IS NULL";
    values = [a.companyId];
  } else if (id === "recent-customer-activity") {
    const source = customerTimelineEvents(a);
    const label = breakdown === "month"
      ? "to_char(to_timestamp(e.at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : "e.source_type";
    sql = `SELECT ${label} label,count(*)::text value FROM (${source.sql}) e WHERE e.at>=? AND e.at<? GROUP BY 1 ORDER BY 1`;
    values = [...source.values, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "collaboration-thread-activity") {
    const source = socialActivityEvents(a);
    const label = breakdown === "month"
      ? "to_char(to_timestamp(e.at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : "e.source_type";
    sql = `SELECT ${label} label,count(*)::text value FROM (${source.sql}) e WHERE e.at>=? AND e.at<? GROUP BY 1 ORDER BY 1`;
    values = [...source.values, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "duplicate-reviews-pending") {
    sql = "SELECT 'Pending reviews' label,count(*)::text value FROM customer_duplicate_reviews WHERE company_id=? AND status='pending'";
    values = [a.companyId];
  } else if (id === "identity-change-events") {
    const label = breakdown === "month" || breakdown === "default"
      ? "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : "e.action";
    sql = `SELECT ${label} label,count(*)::text value FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id WHERE c.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "open-support-tickets" || id === "overdue-support-sla") {
    const overdue = id === "overdue-support-sla";
    const label = breakdown === "status" ? "t.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=t.branch_id AND n.company_id=t.company_id),'Company-wide')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=t.department_id AND n.company_id=t.company_id),'Unassigned')"
          : overdue ? "'Overdue SLA'" : "t.status";
    sql = `SELECT ${label} label,count(*)::text value FROM support_tickets t WHERE ${scopedAlias("t")} AND t.status NOT IN ('closed','cancelled') ${overdue ? "AND t.sla_clock_json->>'state'='running' AND (t.sla_clock_json->>'dueAt') ~ '^[0-9]+$' AND (t.sla_clock_json->>'dueAt')::bigint < floor(extract(epoch FROM clock_timestamp())*1000)::bigint" : ""} GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "active-support-incidents") {
    const label = breakdown === "status" ? "t.status"
      : breakdown === "severity" ? "t.severity"
        : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=t.branch_id AND n.company_id=t.company_id),'Company-wide')"
          : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=t.department_id AND n.company_id=t.company_id),'Unassigned')"
            : "'Active support incidents'";
    sql = `SELECT ${label} label,count(*)::text value FROM support_incidents t WHERE ${scopedAlias("t")} AND t.status IN ('open','investigating','mitigated') GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "active-assets" || id === "asset-condition-location") {
    const label = breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Company-wide')"
      : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')"
        : "a.condition";
    sql = `SELECT ${label} label,count(*)::text value FROM operational_assets a WHERE ${scopedAlias("a")} AND a.lifecycle_status='in_service' GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "asset-retirements") {
    const label = breakdown === "branch"
      ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Company-wide')"
      : breakdown === "month" || breakdown === "default"
        ? "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
        : "'Retired assets'";
    sql = `SELECT ${label} label,count(*)::text value FROM operational_asset_events e
      JOIN operational_assets a ON a.id=e.asset_id
      WHERE ${scopedAlias("a")} AND e.action='retire' AND e.lifecycle_status='retired' AND e.created_at>=? AND e.created_at<?
      GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "open-work-orders" || id === "blocked-maintenance") {
    const blocked = id === "blocked-maintenance";
    const label = breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=w.branch_id AND n.company_id=w.company_id),'Company-wide')"
      : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=w.department_id AND n.company_id=w.company_id),'Unassigned')"
        : blocked ? "'Blocked'" : "w.status";
    sql = `SELECT ${label} label,count(*)::text value FROM maintenance_work_orders w WHERE ${scopedAlias("w")} AND ${blocked ? "w.status='blocked'" : "w.status NOT IN ('completed','cancelled')"} GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "asset-downtime") {
    const label = breakdown === "asset" ? "s.asset_code || ' · ' || s.asset_name"
      : breakdown === "branch" ? "COALESCE(s.branch_name,'Company-wide')"
      : breakdown === "department" ? "COALESCE(s.department_name,'Unassigned')" : "s.month_label";
    const from = Date.parse(`${f.from}T00:00:00.000Z`), to = Date.parse(f.to) + 86400000;
    sql = `WITH raw_intervals AS (
        SELECT w.asset_id,w.branch_id,w.department_id,a.code asset_code,a.name asset_name,b.display_name branch_name,d.display_name department_name,
          greatest(w.downtime_started_at,?::bigint) start_at,least(w.downtime_ended_at,?::bigint) end_at,w.id work_order_id
        FROM maintenance_work_orders w JOIN operational_assets a ON a.id=w.asset_id AND a.company_id=w.company_id
        LEFT JOIN organization_nodes b ON b.id=w.branch_id AND b.company_id=w.company_id
        LEFT JOIN organization_nodes d ON d.id=w.department_id AND d.company_id=w.company_id
        WHERE ${scopedAlias("w")} AND w.status IN ('completed','cancelled')
          AND w.downtime_started_at IS NOT NULL AND w.downtime_ended_at IS NOT NULL
          AND w.downtime_ended_at>w.downtime_started_at AND w.downtime_started_at<? AND w.downtime_ended_at>?
      ), ordered AS (
        SELECT *,max(end_at) OVER(PARTITION BY asset_id ORDER BY start_at,end_at,work_order_id ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) prior_end
        FROM raw_intervals
      ), marked AS (
        SELECT *,sum(CASE WHEN prior_end IS NULL OR start_at>prior_end THEN 1 ELSE 0 END)
          OVER(PARTITION BY asset_id ORDER BY start_at,end_at,work_order_id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) interval_group
        FROM ordered
      ), merged AS (
        SELECT asset_id,min(asset_code) asset_code,min(asset_name) asset_name,min(branch_name) branch_name,min(department_name) department_name,
          interval_group,min(start_at) start_at,max(end_at) end_at
        FROM marked GROUP BY asset_id,interval_group
      ), monthly_segments AS (
        SELECT m.*,to_char(month_start,'YYYY-MM') month_label,
          greatest(m.start_at,floor(extract(epoch FROM month_start AT TIME ZONE 'UTC')*1000)::bigint) segment_start,
          least(m.end_at,floor(extract(epoch FROM (month_start+interval '1 month') AT TIME ZONE 'UTC')*1000)::bigint) segment_end
        FROM merged m CROSS JOIN LATERAL generate_series(
          date_trunc('month',to_timestamp(m.start_at/1000.0) AT TIME ZONE 'UTC'),
          date_trunc('month',to_timestamp((m.end_at-1)/1000.0) AT TIME ZONE 'UTC'),interval '1 month'
        ) AS months(month_start)
      )
      SELECT ${label} label,sum(segment_end-segment_start)::text value FROM monthly_segments s
      WHERE segment_end>segment_start GROUP BY 1 ORDER BY 1`;
    values = [from,to,...scopeArgs,to,from];
  } else if (id === "quality-holds" || id === "quality-releases") {
    const releases = id === "quality-releases";
    const label = breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=h.branch_id AND n.company_id=h.company_id),'Company-wide')"
      : breakdown === "month" || releases && breakdown === "default" ? "to_char(to_timestamp(h.released_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
        : breakdown === "status" ? "h.status" : releases ? "'Released'" : "h.status";
    sql = `SELECT ${label} label,count(*)::text value FROM inventory_quality_holds h WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND ${releases ? "h.status='released' AND h.released_at>=? AND h.released_at<?" : "h.status NOT IN ('released','cancelled','disposed')"} GROUP BY 1 ORDER BY 1`;
    values = releases
      ? [a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000]
      : [a.companyId, a.branchId, a.branchId];
  } else if (id === "inspection-pass-rate") {
    const label = breakdown === "month"
      ? "to_char(to_timestamp(i.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : breakdown === "branch"
        ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=h.branch_id AND n.company_id=h.company_id),'Company-wide')"
        : "'All finalized inspections'";
    sql = `SELECT ${label} label,
        count(*) FILTER (WHERE i.result='pass')::text numerator,count(*)::text denominator
      FROM inventory_quality_inspections i JOIN inventory_quality_holds h ON h.id=i.hold_id
      WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND i.created_at>=? AND i.created_at<?
      GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "opportunity-pipeline") {
    const label = breakdown === "status" ? "v.stage"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=l.branch_id AND n.company_id=l.company_id),'Company-wide')"
        : breakdown === "month" ? "substring(v.close_date,1,7)" : "v.stage";
    sql = `SELECT ${label} label,sum(v.expected_minor)::text value FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id WHERE ${scopedAlias("l")} AND v.currency=? AND v.close_date BETWEEN ? AND ? AND v.stage IN ('discovery','proposal','negotiation') GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, f.currency, f.from, f.to];
  } else if (id === "overdue-followups") {
    const label = breakdown === "status" ? "c.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.department_id AND n.company_id=o.company_id),'Unassigned')"
          : breakdown === "month" ? "substring(c.due_date,1,7)" : "'Overdue follow-ups'";
    sql = `SELECT ${label} label,count(*)::text value FROM customer_followups c JOIN commerce_orders o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${scopedAlias("o")} AND o.kind IN ('sales','quote') AND c.status='open' AND c.due_date BETWEEN ? AND ? AND c.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, f.from, f.to];
  } else if (id === "brand-profiles" || id === "brand-profile-updates") {
    const updates = id === "brand-profile-updates";
    const label = updates && (breakdown === "month" || breakdown === "default")
      ? "to_char(to_timestamp(p.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : breakdown === "branch"
        ? "b.display_name"
        : updates ? "'Updated profiles'" : "'Configured profiles'";
    sql = `SELECT ${label} label,count(*)::text value FROM organization_brand_profiles p
      JOIN organization_nodes b ON b.id=p.scope_id AND b.company_id=p.company_id AND b.kind='branch' AND b.status='active'
      WHERE p.scope_kind='branch' AND p.company_id=? AND (?::text IS NULL OR p.scope_id=?) ${updates ? "AND p.updated_at>=? AND p.updated_at<?" : ""} GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, ...(updates ? [Date.parse(f.from), Date.parse(f.to) + 86400000] : [])];
  } else if (id === "active-assignments") {
    const label = breakdown === "status" ? "x.scope_kind"
      : breakdown === "branch" ? `CASE WHEN x.scope_kind='holding' THEN 'Holding scope' WHEN x.scope_kind='branch' THEN COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=x.scope_node_id AND n.company_id=x.company_id),'Branch') ELSE COALESCE((SELECT b.display_name FROM organization_nodes d JOIN organization_nodes b ON b.id=d.branch_id AND b.company_id=d.company_id WHERE d.id=x.scope_node_id AND d.company_id=x.company_id),'Unassigned') END`
        : breakdown === "department" ? `CASE WHEN x.scope_kind='department' THEN COALESCE((SELECT display_name FROM organization_nodes WHERE id=x.scope_node_id AND company_id=x.company_id),'Department') ELSE 'Other scope' END`
          : "x.role";
    const scopeClause = a.departmentId
      ? "AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=?) OR (x.scope_kind='department' AND x.scope_node_id=?))"
      : a.branchId
        ? "AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=?) OR (x.scope_kind='department' AND x.scope_node_id IN (SELECT id FROM organization_nodes WHERE company_id=x.company_id AND branch_id=?)))"
        : "";
    const now = Date.now();
    sql = `SELECT ${label} label,count(*)::text value FROM assignments x WHERE x.company_id=? AND x.revoked_at IS NULL AND x.valid_from<=? AND x.valid_until>? AND iam_assignment_active(x.id,?) ${scopeClause} GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, now, now, now, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])];
  } else if (id === "pending-access-reviews") {
    const label = breakdown === "status" ? "i.scope_kind"
      : breakdown === "branch" ? `CASE WHEN i.scope_kind='holding' THEN 'Holding scope' WHEN i.scope_kind='branch' THEN COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=i.scope_node_id AND n.company_id=i.company_id),'Branch') ELSE COALESCE((SELECT b.display_name FROM organization_nodes d JOIN organization_nodes b ON b.id=d.branch_id AND b.company_id=d.company_id WHERE d.id=i.scope_node_id AND d.company_id=i.company_id),'Unassigned') END`
        : breakdown === "department" ? `CASE WHEN i.scope_kind='department' THEN COALESCE((SELECT display_name FROM organization_nodes WHERE id=i.scope_node_id AND company_id=i.company_id),'Department') ELSE 'Other scope' END`
          : "'Pending reviews'";
    const scopeClause = a.departmentId
      ? "AND (i.scope_kind='holding' OR (i.scope_kind='branch' AND i.scope_node_id=?) OR (i.scope_kind='department' AND i.scope_node_id=?))"
      : a.branchId
        ? "AND (i.scope_kind='holding' OR (i.scope_kind='branch' AND i.scope_node_id=?) OR (i.scope_kind='department' AND i.scope_node_id IN (SELECT id FROM organization_nodes WHERE company_id=i.company_id AND branch_id=?)))"
        : "";
    sql = `SELECT ${label} label,count(*)::text value FROM iam_access_review_items i JOIN iam_access_review_campaigns c ON c.id=i.campaign_id AND c.company_id=i.company_id WHERE i.company_id=? AND i.decision='pending' AND c.status='open' ${scopeClause} GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])];
  } else if (id === "active-organization-nodes") {
    const label = breakdown === "status" ? "n.kind"
      : breakdown === "branch" ? `COALESCE((SELECT b.display_name FROM organization_nodes b WHERE b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id),'Branch')`
        : "n.kind";
    sql = `SELECT ${label} label,count(*)::text value FROM organization_nodes n WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active' AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?) GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, a.branchId, a.departmentId, a.departmentId];
  } else if (id === "pending-structure-changes") {
    const label = breakdown === "status" ? "c.node_kind"
      : breakdown === "branch" ? `COALESCE((SELECT display_name FROM organization_nodes WHERE id=c.branch_id AND company_id=c.company_id),'Company-wide')`
        : "c.node_kind";
    sql = `SELECT ${label} label,count(*)::text value FROM organization_node_changes c WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?) GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId];
  } else if (id === "marketing-lead-candidates") {
    const label = breakdown === "status" ? "c.status"
      : breakdown === "month" ? "to_char(to_timestamp(c.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
        : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')"
          : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')"
            : "c.status";
    sql = `SELECT ${label} label,count(*)::text value FROM marketing_lead_candidates c WHERE ${scopedAlias("c")} GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "marketing-engagement-events") {
    const label = breakdown === "status" ? "e.event_type"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')"
          : "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
    sql = `SELECT ${label} label,count(*)::text value FROM marketing_engagement_events e JOIN marketing_lead_candidates c ON c.id=e.candidate_id WHERE ${scopedAlias("c")} AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "completed-manufacturing-orders" || id === "manufacturing-orders-with-scrap") {
    const scrap = id === "manufacturing-orders-with-scrap";
    const label = breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')"
      : "to_char(to_timestamp(o.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
    sql = `SELECT ${label} label,count(*)::text value FROM manufacturing_orders o WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND o.status='completed' AND o.updated_at>=? AND o.updated_at<? ${scrap ? "AND o.reported_scrap_milli>0" : ""} GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "production-cost") {
    const label = breakdown === "month" ? "to_char(to_timestamp(j.posted_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')"
        : breakdown === "product" ? "p.name"
          : breakdown === "warehouse" ? "w.name"
            : "'Posted production costs'";
    const groupKey = breakdown === "product" ? ",p.id" : breakdown === "warehouse" ? ",w.id" : breakdown === "branch" ? ",o.branch_id" : "";
    sql = `SELECT ${label} label,COALESCE(sum(m.amount_minor::numeric),0)::text value
      FROM manufacturing_cost_movements m
      JOIN manufacturing_orders o ON o.id=m.order_id AND o.company_id=m.company_id
      JOIN manufacturing_specifications s ON s.id=o.specification_id
      JOIN commerce_products p ON p.id=s.output_product_id AND p.company_id=o.company_id
      JOIN commerce_warehouses w ON w.id=o.warehouse_id AND w.company_id=o.company_id
      JOIN finance_journals j ON j.id=m.journal_id AND j.company_id=m.company_id AND j.status='posted' AND j.currency=?
      JOIN commerce_settings cs ON cs.company_id=o.company_id AND cs.currency=j.currency
      WHERE m.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND o.status='completed'
        AND m.movement_kind IN ('material_issue','material_return','labor') AND j.posted_at>=? AND j.posted_at<?
      GROUP BY 1${groupKey} ORDER BY 1`;
    values = [f.currency, a.companyId, a.branchId, a.branchId, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "planning-shortage-scenarios") {
    const label = breakdown === "status" ? "s.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=s.branch_id AND n.company_id=s.company_id),'Branch')"
        : "substring(s.due_date,1,7)";
    sql = `SELECT ${label} label,count(*)::text value FROM planning_scenarios s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.due_date BETWEEN ? AND ? AND jsonb_path_exists(COALESCE(s.shortages_json::jsonb,'[]'::jsonb),'$[*] ? (@.shortageMilli > 0)') GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
  } else if (["proposed-planning-recommendations", "capacity-feasible-recommendations"].includes(id)) {
    const feasible = id === "capacity-feasible-recommendations";
    const label = breakdown === "status" ? "r.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=s.branch_id AND n.company_id=s.company_id),'Branch')"
        : "substring(r.feasible_date,1,7)";
    sql = `SELECT ${label} label,count(*)::text value FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND ${feasible ? "r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes" : "r.status='proposed'"} AND r.feasible_date BETWEEN ? AND ? GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, a.branchId, a.branchId, f.from, f.to];
  } else if (id === "open-ecommerce-carts") {
    const label = breakdown === "status" ? "c.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')"
          : "to_char(to_timestamp(c.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
    sql = `SELECT ${label} label,count(*)::text value FROM ecommerce_carts c WHERE ${scopedAlias("c")} AND c.status='open' GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "completed-ecommerce-checkouts") {
    const label = breakdown === "status" ? "x.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')"
          : "to_char(to_timestamp(x.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
    sql = `SELECT ${label} label,count(*)::text value FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id WHERE ${scopedAlias("c")} AND x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (["ecommerce-return-links", "ecommerce-fulfillment-links"].includes(id)) {
    const returns = id === "ecommerce-return-links";
    const label = breakdown === "status" ? `${returns ? "r" : "l"}.status`
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')"
          : `to_char(to_timestamp(${returns ? "r" : "l"}.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')`;
    const relation = returns
      ? "ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id"
      : "ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id";
    const alias = returns ? "r" : "l";
    sql = `SELECT ${label} label,count(*)::text value FROM ${relation} WHERE ${scopedAlias("c")} AND ${alias}.created_at>=? AND ${alias}.created_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "active-benefit-elections") {
    sql = "SELECT e.status label,count(*)::text value FROM benefit_elections e WHERE e.company_id=? AND e.status<>'cancelled' AND e.effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.effective_to IS NULL OR e.effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) GROUP BY e.status ORDER BY e.status";
    values = [a.companyId];
  } else if (id === "published-benefit-plans") {
    sql = "SELECT 'Published plans' label,count(*)::text value FROM benefit_plans WHERE company_id=? AND status='published' AND effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (effective_to IS NULL OR effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))";
    values = [a.companyId];
  } else if (id === "learning-completions") {
    const label = breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Branch')"
      : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')"
        : "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
    sql = `SELECT ${label} label,count(*)::text value FROM learning_assignment_events e JOIN learning_assignments a ON a.id=e.assignment_id JOIN learning_courses c ON c.id=a.course_id WHERE ${scopedAlias("a")} AND e.action='complete' AND e.status='completed' AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "overdue-required-learning") {
    const label = breakdown === "status" ? "a.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Branch')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')"
          : "c.title";
    sql = `SELECT ${label} label,count(*)::text value FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE ${scopedAlias("a")} AND c.required_for IS NOT NULL AND a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND a.status IN ('assigned','in_progress') GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "open-requisitions") {
    const label = breakdown === "status" ? "r.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=r.branch_id AND n.company_id=r.company_id),'Branch')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=r.department_id AND n.company_id=r.company_id),'Unassigned')"
          : "r.job_title";
    sql = `SELECT ${label} label,count(*)::text value FROM talent_requisitions r WHERE ${scopedAlias("r")} AND r.status='open' GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "application-stage-counts") {
    const label = breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=r.branch_id AND n.company_id=r.company_id),'Branch')"
      : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=r.department_id AND n.company_id=r.company_id),'Unassigned')"
        : "a.status";
    sql = `SELECT ${label} label,count(*)::text value FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id WHERE ${scopedAlias("r")} AND a.updated_at>=? AND a.updated_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "open-service-orders") {
    const label = breakdown === "status" ? "o.status"
      : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')"
        : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.department_id AND n.company_id=o.company_id),'Unassigned')"
          : "o.status";
    sql = `SELECT ${label} label,count(*)::text value FROM field_service_orders o WHERE ${scopedAlias("o")} AND o.status NOT IN ('completed','cancelled','billed') GROUP BY 1 ORDER BY 1`;
    values = scopeArgs;
  } else if (id === "service-billing-releases") {
    const label = breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')"
      : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.department_id AND n.company_id=o.company_id),'Unassigned')"
        : "to_char(to_timestamp(b.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
    sql = `SELECT ${label} label,count(*)::text value FROM field_service_billing_releases b JOIN field_service_orders o ON o.id=b.order_id WHERE ${scopedAlias("o")} AND b.created_at>=? AND b.created_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "field-visit-completion") {
    const label = breakdown === "month" ? "to_char(to_timestamp(o.scheduled_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
      : breakdown === "status" ? "o.status"
        : breakdown === "branch" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')"
          : breakdown === "department" ? "COALESCE((SELECT n.display_name FROM organization_nodes n WHERE n.id=o.department_id AND n.company_id=o.company_id),'Unassigned')"
            : breakdown === "technician" ? "COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.company_id=o.company_id AND p.user_id=o.technician_id AND p.merged_into_id IS NULL LIMIT 1),'Unassigned')"
              : "'Field visits'";
    sql = `SELECT ${label} label,count(*) FILTER (WHERE o.status IN ('completed','billed') AND o.visit_completed_at IS NOT NULL)::text numerator,count(*)::text denominator
      FROM field_service_orders o WHERE ${scopedAlias("o")} AND o.scheduled_at IS NOT NULL
        AND o.status IN ('scheduled','dispatched','in_visit','exception','completed','billed')
        AND o.scheduled_at>=? AND o.scheduled_at<? GROUP BY 1 ORDER BY 1`;
    values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
  } else if (id === "approved-agent-versions") {
    sql = "SELECT 'Approved versions' label,count(*)::text value FROM agent_versions v JOIN agent_catalog c ON c.id=v.agent_id WHERE c.company_id=? AND c.status='approved' AND v.status='approved'";
    values = [a.companyId];
  } else if (id === "agent-invocation-outcomes") {
    const label = breakdown === "status" ? "e.outcome"
      : "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
    sql = `SELECT ${label} label,count(*)::text value FROM ai_invocation_evidence e WHERE e.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1`;
    values = [a.companyId, Date.parse(f.from), Date.parse(f.to) + 86400000];
  }
  if (["legal-records", "legal-contract-value", "legal-exposure"].includes(id)) {
    const visibility = legalVisibility(a);
    const table = id === "legal-contract-value" ? "legal_contracts" : id === "legal-exposure" ? "legal_matters" : null;
    const field = id === "legal-contract-value" ? "value" : "exposure";
    sql = `SELECT i.state label,${table ? `coalesce(sum(coalesce((t.data->>'${field}')::bigint,0)),0)::text` : "count(*)::text"} value FROM legal_items i ${table ? `JOIN ${table} t ON t.item_id=i.id` : ""} WHERE ${visibility.sql} AND i.updated_at>=? AND i.updated_at<? ${table ? "AND i.currency=?" : ""} GROUP BY i.state ORDER BY i.state`;
    values = [...visibility.values, Date.parse(f.from), Date.parse(f.to) + 86400000, ...(table ? [f.currency] : [])];
  } else if (id === "active-legal-obligations") {
    const visibility = legalVisibility(a);
    const label = breakdown === "status" ? "s.state"
      : breakdown === "month" ? "coalesce(to_char(to_date(s.due_date,'YYYY-MM-DD'),'YYYY-MM'),'No due date')"
        : "s.category";
    sql = `SELECT ${label} label,count(DISTINCT s.id)::text value
      FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id
      WHERE ${visibility.sql} AND i.kind IN ('contract','obligation','license','ip','matter')
        AND i.state NOT IN ('archived','closed','cancelled','terminated','expired','completed')
        AND s.state NOT IN ('completed','waived','cancelled','closed','acknowledged','accepted','billed')
      GROUP BY 1 ORDER BY 1`;
    values = visibility.values;
  } else if (id === "active-legal-holds") {
    const visibility = legalVisibility(a);
    const label = breakdown === "status" ? "h.state" : "i.kind";
    sql = `SELECT ${label} label,count(DISTINCT h.id)::text value
      FROM legal_holds h JOIN legal_items i ON i.id=h.item_id
      WHERE ${visibility.sql} AND h.state IN ('active','release_requested')
      GROUP BY 1 ORDER BY 1`;
    values = visibility.values;
  }

  const raw = (await sourceDb.prepare(sql).bind(...values).all<{ label: string; unit?: string; value?: string | number | bigint; numerator?: string | number | bigint; denominator?: string | number | bigint; total?: string | number | bigint; count_value?: string | number | bigint; partial_count?: string | number | bigint; configured_count?: string | number | bigint }>()).results;
  if (id === "exception-resolution-time") {
    metricTotalOverride = raw[0]?.total === undefined ? "0" : String(exactInteger(raw[0].total));
  }
  if (id === "hotel-occupancy" || id === "hotel-adr" || id === "hotel-revpar")
    metricPartialOverride = raw.some((row) => exactInteger(row.partial_count ?? "0") > BigInt(0));
  if (id === "inventory-shortages")
    metricEmptyOverride = raw.reduce((total, row) => total + exactInteger(row.configured_count ?? "0"), BigInt(0)) === BigInt(0);
  if (id === "overdue-project-milestones") metricEmptyOverride = exactInteger(raw[0]?.value ?? "0") === BigInt(0);
  if (id === "project-dependency-blockers") metricEmptyOverride = exactInteger(raw[0]?.value ?? "0") === BigInt(0);
  if (id === "project-actual-hours" || id === "project-posted-cost")
    metricEmptyOverride = raw.reduce((total, row) => total + exactInteger(row.count_value ?? "0"), BigInt(0)) === BigInt(0);
  const roomRateMetric = id === "hotel-adr" || id === "hotel-revpar";
  type ExactMetricRow = { label: string; value: string; unit?: string; numerator?: string; denominator?: string };
  const valueRows = (id === "exception-resolution-time" && exactInteger(raw[0]?.count_value ?? "0") === BigInt(0))
    || (id === "overdue-project-milestones" && metricEmptyOverride)
    || (id === "project-dependency-blockers" && metricEmptyOverride)
    || ((id === "project-actual-hours" || id === "project-posted-cost") && metricEmptyOverride)
    ? []
    : roomRateMetric
      ? raw.filter((row) => exactInteger(row.denominator ?? "0") > BigInt(0))
      : raw;
  const exactRows: ExactMetricRow[] = valueRows.map((row) => spec.unit === "percent"
    ? { label: row.label, value: percentHundredths(row.numerator ?? "0", row.denominator ?? "0"), numerator: String(exactInteger(row.numerator ?? "0")), denominator: String(exactInteger(row.denominator ?? "0")) }
    : roomRateMetric
      ? { label: row.label, value: moneyRatioMinor(row.numerator ?? "0", row.denominator ?? "0"), numerator: String(exactInteger(row.numerator ?? "0")), denominator: String(exactInteger(row.denominator ?? "0")) }
      : spec.unit === "quantity"
        ? { label: row.label, value: String(exactInteger(row.value ?? "0")), unit: row.unit ?? "" }
        : { label: row.label, value: String(exactInteger(row.value ?? "0")) });
  if (roomRateMetric) {
    metricTotalOverride = moneyRatioMinor(
      sumExact(exactRows.map((row) => row.numerator ?? "0")),
      sumExact(exactRows.map((row) => row.denominator ?? "0")),
    );
    if (raw.some((row) => exactInteger(row.denominator ?? "0") === BigInt(0) && exactInteger(row.numerator ?? "0") !== BigInt(0)))
      metricPartialOverride = true;
  }
  const quantityUnits = spec.unit === "quantity" ? [...new Set(exactRows.map((row) => "unit" in row ? row.unit ?? "" : ""))] : [];
  const total = metricTotalOverride ?? (spec.unit === "percent"
    ? percentHundredths(sumExact(exactRows.map((row) => "numerator" in row ? row.numerator ?? "0" : "0")), sumExact(exactRows.map((row) => "denominator" in row ? row.denominator ?? "0" : "0")))
    : spec.unit === "quantity" ? quantityUnits.length === 1 ? sumExact(exactRows.map((row) => row.value)) : null : sumExact(exactRows.map((row) => row.value)));
  const quantityRowsTruncated = spec.unit === "quantity" && exactRows.length > 100;
  const hasMetricRows = id === "inventory-shortages" || id === "overdue-project-milestones" || id === "project-dependency-blockers" || id === "project-actual-hours" || id === "project-posted-cost" ? !metricEmptyOverride : id === "exception-resolution-time" || roomRateMetric ? exactRows.length > 0 : raw.length > 0;
  const rows = spec.unit === "quantity"
    ? exactRows.slice(0, 100).map(({ label, value, unit }) => ({ label, value, unit }))
    : roomRateMetric && exactRows.length > 100
    ? [...exactRows.slice(0, 99).map(({ label, value }) => ({ label, value })), {
      label: "Other",
      value: moneyRatioMinor(
        sumExact(exactRows.slice(99).map((row) => row.numerator ?? "0")),
        sumExact(exactRows.slice(99).map((row) => row.denominator ?? "0")),
      ),
    }]
    : exactRows.length > 100
    ? [...exactRows.slice(0, 99).map(({ label, value }) => ({ label, value })), {
      label: "Other",
      value: spec.unit === "percent"
        ? percentHundredths(sumExact(exactRows.slice(99).map((row) => "numerator" in row ? row.numerator ?? "0" : "0")), sumExact(exactRows.slice(99).map((row) => "denominator" in row ? row.denominator ?? "0" : "0")))
        : sumExact(exactRows.slice(99).map((row) => row.value)),
    }]
    : exactRows.map(({ label, value }) => ({ label, value }));

  const referenceAt = referenceAtOverride ?? Date.now();
  const watermarks = spec.holdingOwnerOnly
    ? await sourceDb.prepare(`SELECT a.version::text account_version,
        (SELECT md5(COALESCE(string_agg(jsonb_build_array(e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at)::text,',' ORDER BY e.id),'')) FROM billing_enrollments e WHERE e.holding_id=a.holding_id) enrollment_revision,
        (SELECT md5(COALESCE(string_agg(jsonb_build_array(i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind)::text,',' ORDER BY i.id),'')) FROM billing_invoices i WHERE i.holding_id=a.holding_id) invoice_revision,
        (SELECT md5(COALESCE(string_agg(jsonb_build_array(b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at)::text,',' ORDER BY b.holding_id),'')) FROM beta_billing b WHERE b.holding_id=a.holding_id) legacy_subscription_revision,
        (SELECT md5(COALESCE(string_agg(jsonb_build_array(p.id,p.revision,p.name,p.effective_month)::text,',' ORDER BY p.id,p.revision),'')) FROM billing_plans p WHERE p.id IN (SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id)) plan_revision
        FROM billing_accounts a WHERE a.holding_id=?`).bind(a.holdingId).first<Record<string, string>>().then((row) => row ? [{ table_name: "billing_snapshot", revision: JSON.stringify(row) }] : [])
    : (await sourceDb.prepare(`SELECT table_name,revision::text revision FROM work_cache_revisions WHERE table_name IN (${spec.sourceTables.map(() => "?").join(",")}) ORDER BY table_name`)
      .bind(...spec.sourceTables).all<{ table_name: string; revision: string }>()).results;
  const sourceRevision = await sha256(JSON.stringify(watermarks));
  const queryDefinitionRevision = `${id}:1`;
  const scopeLevel = spec.holdingOwnerOnly ? "holding" : ["my-tasks", "projects", "overdue-project-milestones", "project-dependency-blockers", "project-actual-hours", "project-posted-cost", "routines", "approved-leave-duration", "authorized-overtime-duration", ...operationsMetrics].includes(id) ? "personal"
    : a.departmentId ? "department" : a.branchId ? "branch" : "company";
  const authorityRevision = await sha256(JSON.stringify([a.holdingId, a.companyId, a.assignmentId, a.branchId, a.departmentId, a.iam?.roleRevision, a.iam?.identityVersion]));
  const scope = spec.holdingOwnerOnly ? "Selected Holding billing" : `${a.departmentId ? "Selected department" : a.branchId ? "Selected branch" : "Selected company"}${scopeLevel === "personal" ? " · Current reader only" : ""}`;
  const queryId = await sha256(JSON.stringify({ metric: id, queryDefinitionRevision, breakdown, filters: f, scope: [a.holdingId, a.companyId, a.assignmentId, a.branchId, a.departmentId, a.userId] }));
  const revision = await sha256(JSON.stringify({ schemaVersion: 2, queryDefinitionRevision, queryId, sourceRevision, rows }));
  const groupedBy = breakdown === "default" ? (id === "invoiced-sales" ? "month" : "adapter default") : breakdown;
  const definition = `${spec.formula} Date basis: ${spec.dateBasis}. Grouping: ${groupedBy}.${id === "headcount" && breakdown !== "default" ? " For people with multiple effective engagements, the latest start date then engagement ID chooses one group." : ""}`;

  return {
    schemaVersion: 2,
    metric: id,
    total,
    rows,
    unit: spec.unit,
    valueScale: spec.unit === "quantity" ? 3 : spec.unit === "count" || spec.unit === "duration" ? 0 : 2,
    currency: spec.unit === "money" ? f.currency : null,
    formula: spec.formula,
    definition,
    scope,
    scopeBoundary: { level: scopeLevel, revision: authorityRevision },
    referenceAt,
    updatedAt: referenceAt,
    sourceRevision,
    queryDefinitionRevision,
    revision,
    status: hasMetricRows ? quantityRowsTruncated || metricPartialOverride ? "partial" : "ready" : "empty",
    sourceReferences: [{ moduleId: spec.moduleId, datasets: spec.sourceTables, queryId, revision: sourceRevision, referencedAt: referenceAt }],
  } satisfies MetricData;
}

export function metricUnavailable(id: MetricId, reason: string, status: "unavailable" | "failed" = "unavailable", currency = "THB"): MetricData {
  const spec = specs[id];
  const now = Date.now();
  return {
    schemaVersion: 2, metric: id, total: "0", rows: [], unit: spec.unit, valueScale: spec.unit === "quantity" ? 3 : spec.unit === "count" || spec.unit === "duration" ? 0 : 2,
    currency: spec.unit === "money" ? currency : null, formula: spec.formula, definition: spec.description,
    scope: spec.holdingOwnerOnly ? "Unavailable Holding billing" : "Unavailable", scopeBoundary: { level: spec.holdingOwnerOnly ? "holding" : "company", revision: "unavailable" }, referenceAt: now, updatedAt: now,
    sourceRevision: "unavailable", queryDefinitionRevision: `${id}:1`, revision: "unavailable", status, sourceReferences: [], error: reason,
  };
}
