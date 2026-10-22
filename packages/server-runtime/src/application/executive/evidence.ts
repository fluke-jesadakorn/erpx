import type {readBenefitEvidence,searchBenefitEvidence} from "@erpx/server-benefits/public/executive-evidence";
import {benefitEvidenceMetrics} from "@erpx/server-benefits/public/rules/executive-evidence";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { z } from 'zod';
import { type Filters, type MetricId } from "@erpx/server-runtime/public/rules/canvas-domain";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { CustomerActivityItem } from '@erpx/server-customer_360/public/customer-timeline';
import type { SocialActivityEvent } from '@erpx/server-collaboration/public/social-activity';
import type { PmsRecord } from '@erpx/server-pms/public/rules/pms-model';
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import { organizationChangeHref } from "@erpx/server-organization/public/rules/organization-domain";
import { BusinessRuleError } from '@erpx/server-shared/index';
import { operationsMetrics, type OperationsMetricId } from '@erpx/server-runtime/domain/executive/operations-metric-ids';
import { providerId, payrollSummaryMetric, payrollSummaryProviderId, payrollSummaryFields, approvedLeaveMetric, authorizedOvertimeMetric, peopleLeaveOvertimeProviderId, approvedLeaveFields, authorizedOvertimeFields, hotelOccupancyMetric, hotelOccupancyProviderId, hotelOccupancyFields, hotelRoomRevenueMetrics, HotelRoomRevenueMetric, hotelRoomRevenueFields, hotelGuestAccountBalanceMetric, hotelGuestAccountBalanceProviderId, hotelGuestAccountBalanceFields, hotelNightAuditBlockersMetric, hotelNightAuditBlockersProviderId, hotelNightAuditBlockersFields, procurementProviderId, salesProviderId, qualityProviderId, financeMetrics, FinanceMetricId, salesMetrics, SalesMetricId, salesFulfillmentMetric, fieldVisitMetric, manufacturingCostMetric, customerActivityMetric, socialActivityMetric, legalMetrics, LegalMetricId, legalProviderId, maintenanceDowntimeMetric, maintenanceProviderId, supportIncidentMetric, supportIncidentProviderId, supportIncidentFields, supportTicketMetrics, SupportTicketMetricId, supportTicketProviderId, duplicateReviewMetric, duplicateReviewProviderId, duplicateReviewFields, closeBlockersMetric, closeBlockersProviderId, closeBlockersFields, identityChangeMetric, identityChangeProviderId, identityChangeFields, organizationNodeMetric, pendingStructureMetric, organizationProviderId, organizationNodeFields, pendingStructureFields, activeAssignmentsMetric, pendingAccessReviewsMetric, activeAssignmentsProviderId, pendingAccessReviewsProviderId, activeSubscriptionsMetric, overdueInvoicesMetric, activeSubscriptionsProviderId, overdueInvoicesProviderId, activeSubscriptionFields, legacySubscriptionFields, overdueInvoiceFields, activeAssignmentFields, pendingAccessReviewFields, maintenanceSearchFields, maintenanceDetailFields, qualityMetrics, searchableMetrics, searchFields, detailFields, procurementSearchFields, procurementDetailFields, salesSearchFields, salesDetailFields, salesOrderSearchFields, salesOrderDetailFields, commonFilters, querySchema, procurementQuerySchema, salesQuerySchema, salesFulfillmentQuerySchema, fieldVisitQuerySchema, manufacturingCostQuerySchema, qualityQuerySchema, customerActivityQuerySchema, socialActivityQuerySchema, legalQuerySchema, maintenanceQuerySchema, supportIncidentQuerySchema, supportTicketQuerySchema, duplicateReviewQuerySchema, closeBlockersQuerySchema, identityChangeQuerySchema, organizationNodeQuerySchema, pendingStructureQuerySchema, activeAssignmentsQuerySchema, pendingAccessReviewsQuerySchema, activeSubscriptionsQuerySchema, overdueInvoicesQuerySchema, payrollSummaryQuerySchema, peopleLeaveOvertimeQuerySchema, hotelOccupancyQuerySchema, hotelGuestAccountBalanceQuerySchema, hotelNightAuditBlockersQuerySchema, availableStockMetric, availableStockProviderId, availableStockFields, availableStockQuerySchema, inventoryShortagesMetric, inventoryShortagesProviderId, inventoryShortagesFields, inventoryShortagesQuerySchema, stockedLocationsMetric, stockedLocationsQuerySchema, inventoryMovementsMetric, inventoryMovementsProviderId, inventoryMovementFields, inventoryMovementsQuerySchema, operationsProviderId, operationsQuerySchema, projectMilestoneMetric, projectMilestoneProviderId, projectMilestoneFields, projectMilestoneQuerySchema, projectDependencyMetric, projectDependencyProviderId, projectDependencyFields, projectDependencyQuerySchema, projectActualMetrics, ProjectActualMetric, projectActualProviderId, projectActualHoursFields, projectPostedCostFields, projectActualQuerySchema, InvoiceEvidenceRow, predicate, cursorValue, encodeCursor, LegalEvidenceRow, LegalHoldEvidenceRow, legalEvidenceCursor, encodeLegalEvidenceCursor, PayrollSummaryEvidenceRow, payrollSummaryCursor, encodePayrollSummaryCursor, peopleRequestCursor, encodePeopleRequestCursor, HotelOccupancyEvidenceRow, hotelOccupancyCursorValue, encodeHotelOccupancyCursor, HotelGuestAccountBalanceEvidenceRow, hotelGuestAccountBalanceCursorValue, encodeHotelGuestAccountBalanceCursor, HotelNightAuditBlockerEvidenceRow, hotelNightAuditBlockerCursorValue, encodeHotelNightAuditBlockerCursor, SupplierBillEvidenceRow, SalesOrderEvidenceRow, SalesEvidenceCursor, salesCursorValue, encodeSalesCursor, SalesFulfillmentEvidenceRow, salesFulfillmentFields, salesFulfillmentCursorValue, encodeSalesFulfillmentCursor, FieldVisitEvidenceRow, fieldVisitFields, fieldVisitCursorValue, encodeFieldVisitCursor, QualityInspectionRow, qualitySearchFields, qualityDetailFields, qualityCursorValue, encodeQualityCursor, ManufacturingCostEvidenceRow, manufacturingCostProviderId, manufacturingCostSearchFields, manufacturingCostDetailFields, manufacturingCostCursorValue, encodeManufacturingCostCursor, customerActivityProviderId, customerActivityFields, customerActivityCursor, encodeCustomerActivityCursor, socialActivityProviderId, socialActivityFields, socialActivityCursor, encodeSocialActivityCursor, SupportIncidentEvidenceRow, supportIncidentCursorValue, encodeSupportIncidentCursor, encodeSupportIncidentRecordId, decodeSupportIncidentRecordId, supportIncidentNumber, SupportTicketEvidenceRow, supportTicketCursorValue, encodeSupportTicketCursor, encodeSupportTicketRecordId, decodeSupportTicketRecordId, supportTicketNumber, supportTicketFieldsFor, DuplicateReviewEvidenceRow, duplicateReviewCursorValue, encodeDuplicateReviewCursor, encodeDuplicateReviewRecordId, decodeDuplicateReviewRecordId, duplicateReviewNumber, IdentityChangeEvidenceRow, identityChangeCursorValue, encodeIdentityChangeCursor, encodeIdentityChangeRecordId, decodeIdentityChangeRecordId, identityChangeNumber, OrganizationNodeEvidenceRow, PendingStructureEvidenceRow, organizationEvidenceCursor, encodeOrganizationRecordId, decodeOrganizationRecordId, organizationEvidenceNumber, ActiveAssignmentEvidenceRow, PendingAccessReviewEvidenceRow, identityAccessCursor, encodeIdentityAccessRecordId, decodeIdentityAccessRecordId, CommercialSubscriptionRow, OverdueInvoiceEvidenceRow, commercialCursor, encodeCommercialSubscriptionId, decodeCommercialSubscriptionId, encodeOverdueInvoiceId, decodeOverdueInvoiceId, CloseBlockerEvidenceRow, closeBlockerCursorValue, encodeCloseBlockerCursor, encodeCloseBlockerRecordId, decodeCloseBlockerRecordId, closeBlockerNumber, closeBlockerCount, maintenanceCursorValue, encodeMaintenanceCursor, maintenanceEvidenceNumber, AvailableStockEvidenceRow, InventoryShortageEvidenceRow, InventoryMovementEvidenceRow, inventoryMovementCursorValue, encodeInventoryMovementCursor, encodeInventoryMovementRecordId, decodeInventoryMovementRecordId, inventoryMovementHref, availableStockCursorValue, encodeAvailableStockCursor, encodeAvailableStockRecordId, decodeAvailableStockRecordId, milliQuantity, ProjectMilestoneEvidenceRow, projectMilestoneCursor, ProjectDependencyEvidenceRow, projectDependencyBlockers, ProjectActualEvidenceRow, projectActualCursor, OperationsEvidenceRow, operationsEvidenceFields, operationsRecordId, decodeOperationsRecordId, operationsEvidenceCursor } from '@erpx/server-runtime/domain/executive/evidence';
import type { Clock } from '@erpx/server-shared/index';
import type { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import type { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
import type { assertProductionModuleEnabled } from "@erpx/server-pms/public/hospitality-rollout";
import type { getSupplierBill } from "@erpx/server-procurement/public/supplier-bill-matching";
import type { getQualityHold } from "@erpx/server-quality/public/quality";
import type { fieldServiceDetail } from "@erpx/server-field_service/public/field-service";
import type { productionDetail } from "@erpx/server-manufacturing/public/production";
import type { maintenanceDowntimeEvidenceRows } from "@erpx/server-maintenance/public/asset-maintenance";
import type { getSupportIncident } from "@erpx/server-support/public/support-incidents";
import type { canReadOrganizationChanges } from "@erpx/server-organization/public/organization-work";
import type { loadPms } from "../../../../../packages/server-pms/src/public/pms";
import type { getOvertime } from "@erpx/server-people_payroll/public/overtime";
import type { readAgentEvidence, searchAgentEvidence } from "@erpx/server-runtime/public/executive/agent-evidence";
import type { readCrmEvidence, searchCrmEvidence } from "@erpx/server-runtime/public/executive/crm-evidence";
import type { readBrandEvidence, searchBrandEvidence } from "@erpx/server-runtime/public/executive/brand-evidence";
import type { readLearningEvidence, searchLearningEvidence } from "@erpx/server-runtime/public/executive/learning-evidence";
import type { readTalentEvidence, searchTalentEvidence } from "@erpx/server-runtime/public/executive/talent-evidence";
import type { readPosEvidence, searchPosEvidence } from "@erpx/server-runtime/public/executive/pos-evidence";
import type { readMarketingEvidence, searchMarketingEvidence } from "@erpx/server-runtime/public/executive/marketing-evidence";
import type { readPlanningEvidence, searchPlanningEvidence } from "@erpx/server-runtime/public/executive/planning-evidence";
import { agentEvidenceMetrics } from '@erpx/server-runtime/domain/executive/agent-evidence';
import { crmEvidenceMetrics } from '@erpx/server-runtime/domain/executive/crm-evidence';
import { brandEvidenceMetrics } from '@erpx/server-runtime/domain/executive/brand-evidence';
import { learningEvidenceMetrics } from '@erpx/server-runtime/domain/executive/learning-evidence';
import { talentEvidenceMetrics } from '@erpx/server-runtime/domain/executive/talent-evidence';
import { posEvidenceMetrics } from '@erpx/server-runtime/domain/executive/pos-evidence';
import { marketingEvidenceMetrics } from '@erpx/server-runtime/domain/executive/marketing-evidence';
import { planningEvidenceMetrics } from '@erpx/server-runtime/domain/executive/planning-evidence';
function operationsTaskHref(id: string) { return '/staff-tasks/' + encodeURIComponent(id); }
function projectMilestoneHref(id: string) { return '/projects/' + encodeURIComponent(id); }
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface ExecutiveEvidenceRepository {
    factsAudit(actor: WorkContext, provider: string, metric: "legal-records" | "legal-contract-value" | "legal-exposure" | "active-legal-obligations" | "active-legal-holds" | "active-subscriptions" | "overdue-invoices" | "receivables" | "payables" | "invoiced-sales" | "cash-receipts" | "close-blockers" | "sales-orders" | "sales-fulfillment-rate" | "sales-cancellations" | "sales-cancellation-value" | "purchase-orders" | "open-po-receipts" | "invoice-match-exceptions" | "headcount" | "approved-leave-duration" | "authorized-overtime-duration" | "payroll-summary" | "my-tasks" | "open-operations-exceptions" | "exception-resolution-time" | "operations-escalations" | "inventory" | "available-stock" | "inventory-shortages" | "inventory-movements" | "pos-sales" | "net-pos-sales" | "pos-payment-mix" | "hotel-rooms" | "hotel-occupancy" | "hotel-adr" | "hotel-revpar" | "hotel-guest-account-balance" | "hotel-night-audit-blockers" | "projects" | "overdue-project-milestones" | "project-actual-hours" | "project-posted-cost" | "project-dependency-blockers" | "routines" | "active-customers" | "recent-customer-activity" | "duplicate-reviews-pending" | "identity-change-events" | "open-support-tickets" | "overdue-support-sla" | "active-support-incidents" | "active-assets" | "asset-condition-location" | "asset-retirements" | "open-work-orders" | "asset-downtime" | "blocked-maintenance" | "quality-holds" | "quality-releases" | "inspection-pass-rate" | "opportunity-pipeline" | "overdue-followups" | "brand-profiles" | "brand-profile-updates" | "active-assignments" | "pending-access-reviews" | "active-organization-nodes" | "pending-structure-changes" | "marketing-lead-candidates" | "marketing-engagement-events" | "completed-manufacturing-orders" | "manufacturing-orders-with-scrap" | "production-cost" | "planning-shortage-scenarios" | "proposed-planning-recommendations" | "capacity-feasible-recommendations" | "open-ecommerce-carts" | "completed-ecommerce-checkouts" | "ecommerce-return-links" | "ecommerce-fulfillment-links" | "active-benefit-elections" | "published-benefit-plans" | "learning-completions" | "overdue-required-learning" | "open-requisitions" | "application-stage-counts" | "open-service-orders" | "service-billing-releases" | "field-visit-completion" | "approved-agent-versions" | "agent-invocation-outcomes" | "collaboration-thread-activity", action: "search" | "open", fields: string[], queryHash: string, now: number, auditRows: {
        id: string;
        version: number | null;
    }[]): Promise<void>;
    factsSearchLegalEvidence(actor: WorkContext, isObligation: boolean, cursor: {
        id: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "active-legal-obligations" | "active-legal-holds";
        cursor?: string | undefined;
    }): Promise<{
        rows: LegalEvidenceRow[] | LegalHoldEvidenceRow[];
    }>;
    factsReadLegalEvidence(actor: WorkContext, isObligation: boolean, recordId: string): Promise<{
        row: LegalEvidenceRow | LegalHoldEvidenceRow | null;
    }>;
    factsSearchFinanceEvidence(type: {
        kind: string;
        statuses: string[];
    }, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "receivables" | "payables" | "invoiced-sales";
        cursor?: string | undefined;
    }, actor: WorkContext, cursor: {
        invoiceDate: string;
        id: string;
    } | null): Promise<{
        rows: InvoiceEvidenceRow[];
    }>;
    factsReadFinanceEvidence(type: {
        kind: string;
        statuses: string[];
    }, recordId: string, actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }): Promise<{
        row: (InvoiceEvidenceRow & {
            subtotal_minor: string;
            tax_minor: string;
            posted_at: number | null;
            party_name: string;
        }) | null;
    }>;
    factsSearchPayrollSummaryEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "payroll-summary";
        cursor?: string | undefined;
    }, cursor: {
        periodEnd: string;
        id: string;
    } | null): Promise<{
        rows: PayrollSummaryEvidenceRow[];
    }>;
    factsReadPayrollSummaryEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: PayrollSummaryEvidenceRow | null;
    }>;
    factsSearchPeopleLeaveOvertimeEvidence(leave: boolean, actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "approved-leave-duration" | "authorized-overtime-duration";
        cursor?: string | undefined;
    }, cursor: {
        date: string;
        id: string;
    } | null): Promise<{
        rows: {
            id: string;
            number?: string;
            start_date?: string;
            end_date?: string;
            units?: string;
            days?: number;
            work_date?: string;
            minutes?: string;
            request_version?: number;
            state: string;
            version: number;
        }[];
    }>;
    factsSearchHotelOccupancyEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "hotel-occupancy" | "hotel-adr" | "hotel-revpar";
        cursor?: string | undefined;
    }, cursor: {
        businessDate: string;
        id: string;
    } | null): Promise<{
        rows: HotelOccupancyEvidenceRow[];
    }>;
    factsHotelNightRoomRevenue(propertyId: string, actor: WorkContext, businessDate: string): Promise<{
        revenue_minor: string;
        invalid_revenue_count: string;
    } | null>;
    factsReadHotelOccupancyEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, metric: "hotel-occupancy" | "hotel-adr" | "hotel-revpar", recordId: string): Promise<{
        row: HotelOccupancyEvidenceRow | null;
    }>;
    factsSearchHotelGuestAccountBalanceEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "hotel-guest-account-balance";
        cursor?: string | undefined;
    }, cursor: {
        propertyId: string;
        id: string;
    } | null): Promise<{
        rows: HotelGuestAccountBalanceEvidenceRow[];
    }>;
    factsReadHotelGuestAccountBalanceEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: HotelGuestAccountBalanceEvidenceRow | null;
    }>;
    factsSearchHotelNightAuditBlockerEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "hotel-night-audit-blockers";
        cursor?: string | undefined;
    }, cursor: {
        businessDate: string;
        id: string;
    } | null): Promise<{
        rows: HotelNightAuditBlockerEvidenceRow[];
    }>;
    factsReadHotelNightAuditBlockerEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: HotelNightAuditBlockerEvidenceRow | null;
    }>;
    factsSearchSupplierBillEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "invoice-match-exceptions";
        cursor?: string | undefined;
    }, cursor: {
        invoiceDate: string;
        id: string;
    } | null): Promise<{
        rows: SupplierBillEvidenceRow[];
    }>;
    factsReadSupplierBillEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        latest: SupplierBillEvidenceRow;
    }>;
    factsSearchSalesOrderEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "sales-orders" | "sales-cancellations" | "sales-cancellation-value";
        cursor?: string | undefined;
    }, cursor: SalesEvidenceCursor | null): Promise<{
        rows: SalesOrderEvidenceRow[];
    }>;
    factsReadSalesOrderEvidence(actor: WorkContext, metric: "sales-orders" | "sales-cancellations" | "sales-cancellation-value", filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: SalesOrderEvidenceRow;
    }>;
    factsSearchSalesFulfillmentEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "sales-fulfillment-rate";
        cursor?: string | undefined;
    }, cursor: {
        orderDate: string;
        id: string;
    } | null): Promise<{
        rows: SalesFulfillmentEvidenceRow[];
    }>;
    factsReadSalesFulfillmentEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: SalesFulfillmentEvidenceRow;
    }>;
    factsSearchFieldVisitEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "field-visit-completion";
        cursor?: string | undefined;
    }, cursor: {
        scheduledAt: number;
        id: string;
    } | null): Promise<{
        rows: FieldVisitEvidenceRow[];
    }>;
    factsSearchQualityInspectionEvidence(input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "quality-holds" | "quality-releases" | "inspection-pass-rate";
        cursor?: string | undefined;
    }, actor: WorkContext, cursor: {
        createdAt: number;
        id: string;
    } | null): Promise<{
        rows: QualityInspectionRow[];
    }>;
    factsSearchQualityHoldEvidence(releases: boolean, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "quality-holds" | "quality-releases" | "inspection-pass-rate";
        cursor?: string | undefined;
    }, actor: WorkContext, cursor: {
        createdAt: number;
        id: string;
    } | null): Promise<{
        rows: QualityInspectionRow[];
    }>;
    factsReadQualityInspectionEvidence(recordId: string, actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }): Promise<{
        row: (QualityInspectionRow & {
            evidence: string;
            inspected_by: string;
        }) | null;
    }>;
    factsSearchManufacturingCostEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "production-cost";
        cursor?: string | undefined;
    }, cursor: {
        postedAt: number;
        id: string;
    } | null): Promise<{
        rows: ManufacturingCostEvidenceRow[];
    }>;
    factsReadManufacturingCostEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: ManufacturingCostEvidenceRow;
    }>;
    factsSearchCustomerActivityEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "recent-customer-activity";
        cursor?: string | undefined;
    }, cursor: {
        at: number;
        id: string;
    } | null): Promise<{
        rows: CustomerActivityItem[];
    }>;
    factsReadCustomerActivityEvidence(actor: WorkContext, recordId: string, filters: {
        from: string;
        to: string;
        currency: string;
    }): Promise<{
        row: CustomerActivityItem | null;
    }>;
    factsSearchSocialActivityEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "collaboration-thread-activity";
        cursor?: string | undefined;
    }, cursor: {
        at: number;
        id: string;
    } | null): Promise<{
        rows: SocialActivityEvent[];
    }>;
    factsReadSocialActivityEvidence(actor: WorkContext, recordId: string, filters: {
        from: string;
        to: string;
        currency: string;
    }): Promise<{
        row: SocialActivityEvent | null;
    }>;
    factsSearchSupportIncidentEvidence(cursor: {
        updatedAt: number;
        id: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "active-support-incidents";
        cursor?: string | undefined;
    }, actor: WorkContext): Promise<{
        rows: SupportIncidentEvidenceRow[];
    }>;
    factsSearchSupportTicketEvidence(input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "open-support-tickets" | "overdue-support-sla";
        cursor?: string | undefined;
    }, cursor: {
        updatedAt: number;
        id: string;
    } | null, overdue: boolean, actor: WorkContext): Promise<{
        rows: SupportTicketEvidenceRow[];
    }>;
    factsReadSupportTicketEvidence(overdue: boolean, metric: "open-support-tickets" | "overdue-support-sla", id: string, actor: WorkContext): Promise<{
        row: SupportTicketEvidenceRow | null;
    }>;
    factsSearchDuplicateReviewEvidence(cursor: {
        updatedAt: number;
        id: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "duplicate-reviews-pending";
        cursor?: string | undefined;
    }, actor: WorkContext): Promise<{
        rows: DuplicateReviewEvidenceRow[];
    }>;
    factsReadDuplicateReviewEvidence(actor: WorkContext, id: string): Promise<{
        row: DuplicateReviewEvidenceRow | null;
    }>;
    factsSearchIdentityChangeEvidence(cursor: {
        at: number;
        id: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "identity-change-events";
        cursor?: string | undefined;
    }, actor: WorkContext): Promise<{
        rows: IdentityChangeEvidenceRow[];
    }>;
    factsReadIdentityChangeEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, id: string): Promise<{
        row: IdentityChangeEvidenceRow | null;
    }>;
    factsSearchOrganizationNodeEvidence(cursor: {
        name: string | undefined;
        at: number | undefined;
        id: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "active-organization-nodes";
        cursor?: string | undefined;
    }, actor: WorkContext): Promise<{
        rows: OrganizationNodeEvidenceRow[];
    }>;
    factsReadOrganizationNodeEvidence(actor: WorkContext, id: string): Promise<{
        row: OrganizationNodeEvidenceRow | null;
    }>;
    factsSearchPendingStructureEvidence(cursor: {
        name: string | undefined;
        at: number | undefined;
        id: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "pending-structure-changes";
        cursor?: string | undefined;
    }, actor: WorkContext): Promise<{
        rows: PendingStructureEvidenceRow[];
    }>;
    factsReadPendingStructureEvidence(actor: WorkContext, id: string): Promise<{
        row: PendingStructureEvidenceRow | null;
    }>;
    factsIdentityAccessUserNames(ids: string[]): Promise<Map<string, string>>;
    factsSearchActiveAssignmentEvidence(actor: WorkContext, cursor: {
        userId: string;
        role: string;
        createdAt: undefined;
        recordId: string;
    } | {
        userId: undefined;
        role: undefined;
        createdAt: number;
        recordId: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "active-assignments";
        cursor?: string | undefined;
    }, now: number): Promise<{
        rows: ActiveAssignmentEvidenceRow[];
    }>;
    factsReadActiveAssignmentEvidence(actor: WorkContext, now: number, id: string): Promise<{
        row: ActiveAssignmentEvidenceRow | null;
    }>;
    factsSearchPendingAccessReviewEvidence(actor: WorkContext, cursor: {
        userId: string;
        role: string;
        createdAt: undefined;
        recordId: string;
    } | {
        userId: undefined;
        role: undefined;
        createdAt: number;
        recordId: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "pending-access-reviews";
        cursor?: string | undefined;
    }): Promise<{
        rows: PendingAccessReviewEvidenceRow[];
    }>;
    factsReadPendingAccessReviewEvidence(actor: WorkContext, id: string): Promise<{
        row: PendingAccessReviewEvidenceRow | null;
    }>;
    factsSearchCommercialSubscriptions(cursor: {
        sourceKind: "enrollment" | "legacy";
        recordId: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "active-subscriptions";
        cursor?: string | undefined;
    }, actor: WorkContext, now: number): Promise<{
        rows: CommercialSubscriptionRow[];
    }>;
    factsReadCommercialSubscription(decoded: {
        sourceKind: "enrollment" | "legacy";
        recordId: string;
    }, actor: WorkContext, now: number): Promise<{
        row: CommercialSubscriptionRow | null;
    }>;
    factsSearchOverdueInvoiceEvidence(cursorValue: {
        dueAt: number;
        id: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "overdue-invoices";
        cursor?: string | undefined;
    }, actor: WorkContext, end: number, now: number): Promise<{
        rows: OverdueInvoiceEvidenceRow[];
    }>;
    factsReadOverdueInvoiceEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, now: number, id: string): Promise<{
        row: OverdueInvoiceEvidenceRow | null;
    }>;
    factsSearchCloseBlockerEvidence(cursor: {
        endDate: string;
        checkKey: string;
        id: string;
    } | null, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "close-blockers";
        cursor?: string | undefined;
    }, actor: WorkContext): Promise<{
        rows: CloseBlockerEvidenceRow[];
    }>;
    factsReadCloseBlockerEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, id: string): Promise<{
        row: CloseBlockerEvidenceRow | null;
    }>;
    factsSearchAvailableStockEvidence(input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "available-stock";
        cursor?: string | undefined;
    }, actor: WorkContext, cursor: {
        warehouseId: string;
        productId: string;
    } | null): Promise<{
        rows: AvailableStockEvidenceRow[];
    }>;
    factsReadAvailableStockEvidence(actor: WorkContext, ids: {
        warehouseId: string;
        productId: string;
    }): Promise<{
        row: AvailableStockEvidenceRow | null;
    }>;
    factsSearchInventoryShortageEvidence(input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "inventory-shortages";
        cursor?: string | undefined;
    }, actor: WorkContext, cursor: {
        warehouseId: string;
        productId: string;
    } | null): Promise<{
        rows: InventoryShortageEvidenceRow[];
    }>;
    factsReadInventoryShortageEvidence(actor: WorkContext, ids: {
        warehouseId: string;
        productId: string;
    }): Promise<{
        row: InventoryShortageEvidenceRow | null;
    }>;
    factsSearchStockedLocationEvidence(input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "inventory";
        cursor?: string | undefined;
    }, actor: WorkContext, cursor: {
        warehouseId: string;
        productId: string;
    } | null): Promise<{
        rows: AvailableStockEvidenceRow[];
    }>;
    factsReadStockedLocationEvidence(actor: WorkContext, ids: {
        warehouseId: string;
        productId: string;
    }): Promise<{
        row: AvailableStockEvidenceRow | null;
    }>;
    factsSearchInventoryMovementEvidence(input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "inventory-movements";
        cursor?: string | undefined;
    }, actor: WorkContext, cursor: {
        movementDate: string;
        id: string;
    } | null): Promise<{
        rows: InventoryMovementEvidenceRow[];
    }>;
    factsReadInventoryMovementEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, ids: {
        movementId: string;
    }): Promise<{
        row: InventoryMovementEvidenceRow | null;
    }>;
    factsSearchProjectMilestoneEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "overdue-project-milestones";
        cursor?: string | undefined;
    }, referenceAt: number, cursor: {
        dueAt: number;
        id: string;
    } | null): Promise<{
        rows: ProjectMilestoneEvidenceRow[];
    }>;
    factsReadProjectMilestoneEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, referenceAt: number, id: string): Promise<{
        row: ProjectMilestoneEvidenceRow | null;
    }>;
    factsSearchProjectDependencyEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "project-dependency-blockers";
        cursor?: string | undefined;
    }, cursor: {
        dueAt: number;
        id: string;
    } | null): Promise<{
        rows: ProjectDependencyEvidenceRow[];
    }>;
    factsReadProjectDependencyEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, id: string): Promise<{
        row: ProjectDependencyEvidenceRow | null;
    }>;
    factsSearchProjectActualEvidence(metric: "project-actual-hours" | "project-posted-cost", input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "project-actual-hours" | "project-posted-cost";
        cursor?: string | undefined;
    }, actor: WorkContext, cursor: {
        date: string;
        id: string;
    } | null): Promise<{
        rows: ProjectActualEvidenceRow[];
    }>;
    factsReadProjectActualEvidence(metric: "project-actual-hours" | "project-posted-cost", actor: WorkContext, id: string, filters: {
        from: string;
        to: string;
        currency: string;
    }): Promise<{
        row: ProjectActualEvidenceRow | null;
    }>;
    factsSearchOperationsEvidence(actor: WorkContext, referenceAt: number, metric: "open-operations-exceptions" | "exception-resolution-time" | "operations-escalations", input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "open-operations-exceptions" | "exception-resolution-time" | "operations-escalations";
        cursor?: string | undefined;
    }, cursor: {
        sortAt: number;
        id: string;
    } | null): Promise<{
        rows: OperationsEvidenceRow[];
    }>;
    factsReadOperationsEvidence(actor: WorkContext, referenceAt: number, metric: "open-operations-exceptions" | "exception-resolution-time" | "operations-escalations", filters: {
        from: string;
        to: string;
        currency: string;
    }, taskId: string): Promise<{
        row: OperationsEvidenceRow | null;
    }>;
    leaveEvidenceRow(actor: WorkContext, filters: Filters, recordId: string): Promise<{
        id: string;
        number: string;
        start_date: string;
        end_date: string;
        units: string;
        days: number;
        state: string;
        version: number;
    } | null>;
    leaveEvidenceVisibility(actor: WorkContext, recordId: string): Promise<Record<string, unknown> | null>;
}
export interface ExecutiveEvidencePorts {
    repository: ExecutiveEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    getSupplierBill(actor: WorkContext, id: string): ReturnType<typeof getSupplierBill>;
    getQualityHold(actor: WorkContext, id: string): ReturnType<typeof getQualityHold>;
    fieldServiceDetail(actor: WorkContext, id: string): ReturnType<typeof fieldServiceDetail>;
    productionDetail(actor: WorkContext, id: string): ReturnType<typeof productionDetail>;
    getSupportIncident(actor: WorkContext, id: string): ReturnType<typeof getSupportIncident>;
    canReadOrganizationChanges(...args: Tail<Parameters<typeof canReadOrganizationChanges>>): ReturnType<typeof canReadOrganizationChanges>;
    loadPms(...args: Tail<Parameters<typeof loadPms>>): ReturnType<typeof loadPms>;
    getOvertime(actor: WorkContext, id: string): ReturnType<typeof getOvertime>;
    maintenanceDowntimeEvidenceRows(actor: WorkContext, input: Parameters<typeof maintenanceDowntimeEvidenceRows>[1]): ReturnType<typeof maintenanceDowntimeEvidenceRows>;
    searchAgentEvidence(...args: Tail<Parameters<typeof searchAgentEvidence>>): ReturnType<typeof searchAgentEvidence>;
    readAgentEvidence(...args: Tail<Parameters<typeof readAgentEvidence>>): ReturnType<typeof readAgentEvidence>;
    searchCrmEvidence(...args: Tail<Parameters<typeof searchCrmEvidence>>): ReturnType<typeof searchCrmEvidence>;
    readCrmEvidence(...args: Tail<Parameters<typeof readCrmEvidence>>): ReturnType<typeof readCrmEvidence>;
    searchBrandEvidence(...args: Tail<Parameters<typeof searchBrandEvidence>>): ReturnType<typeof searchBrandEvidence>;
    readBrandEvidence(...args: Tail<Parameters<typeof readBrandEvidence>>): ReturnType<typeof readBrandEvidence>;
    searchLearningEvidence(...args: Tail<Parameters<typeof searchLearningEvidence>>): ReturnType<typeof searchLearningEvidence>;
    readLearningEvidence(...args: Tail<Parameters<typeof readLearningEvidence>>): ReturnType<typeof readLearningEvidence>;
    searchTalentEvidence(...args: Tail<Parameters<typeof searchTalentEvidence>>): ReturnType<typeof searchTalentEvidence>;
    readTalentEvidence(...args: Tail<Parameters<typeof readTalentEvidence>>): ReturnType<typeof readTalentEvidence>;
    searchPosEvidence(...args: Tail<Parameters<typeof searchPosEvidence>>): ReturnType<typeof searchPosEvidence>;
    readPosEvidence(...args: Tail<Parameters<typeof readPosEvidence>>): ReturnType<typeof readPosEvidence>;
    searchMarketingEvidence(...args: Tail<Parameters<typeof searchMarketingEvidence>>): ReturnType<typeof searchMarketingEvidence>;
    readMarketingEvidence(...args: Tail<Parameters<typeof readMarketingEvidence>>): ReturnType<typeof readMarketingEvidence>;
    searchBenefitEvidence(...args: Tail<Parameters<typeof searchBenefitEvidence>>): ReturnType<typeof searchBenefitEvidence>;
    readBenefitEvidence(...args: Tail<Parameters<typeof readBenefitEvidence>>): ReturnType<typeof readBenefitEvidence>;
    searchPlanningEvidence(...args: Tail<Parameters<typeof searchPlanningEvidence>>): ReturnType<typeof searchPlanningEvidence>;
    readPlanningEvidence(...args: Tail<Parameters<typeof readPlanningEvidence>>): ReturnType<typeof readPlanningEvidence>;
}
export function createExecutiveEvidenceApplication(ports: ExecutiveEvidencePorts) {
    const r = ports.repository;
    async function requireFinanceEvidence(actor: WorkContext, metric: FinanceMetricId) {
        await ports.currentAuthority(actor);
        if (!searchableMetrics.has(metric) || !hasPermission(actor, "finance.read") || actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "finance");
    }
    async function audit(actor: WorkContext, provider: string, metric: MetricId, action: "search" | "open", rows: {
        id: string;
        version: number | null;
    }[], fields: string[], queryHash: string) {
        if (!rows.length && action === "search")
            return;
        const now = ports.clock.now();
        const auditRows = rows.length ? rows : [{ id: "", version: 0 }];
        await r.factsAudit(actor, provider, metric, action, fields, queryHash, now, auditRows);
    }
    async function requireLegalEvidence(actor: WorkContext, metric: LegalMetricId) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "legal.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
    }
    async function searchLegalEvidence(actor: WorkContext, raw: unknown) {
        const input = legalQuerySchema.parse(raw);
        await requireLegalEvidence(actor, input.metric);
        const cursor = legalEvidenceCursor(input.cursor);
        const isObligation = input.metric === "active-legal-obligations";
        const { rows } = await r.
            factsSearchLegalEvidence(actor, isObligation, cursor, input);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const fields = isObligation
            ? ["title", "category", "due_date", "state", "version", "legal_item.title", "legal_item.kind", "legal_item.version"]
            : ["state", "created_at", "legal_item.title", "legal_item.kind", "legal_item.version"];
        const queryHash = await ports.digest(JSON.stringify({ providerId: legalProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, legalProviderId, input.metric, "search", items.flatMap((row) => [
            ...(isObligation ? [{ id: row.id, version: Number((row as LegalEvidenceRow).version) }] : []),
            { id: row.item_id, version: Number(row.item_version) },
        ]), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: legalProviderId,
            metric: input.metric,
            fields,
            items: items.map((row) => ({
                id: row.id,
                label: isObligation ? (row as LegalEvidenceRow).title : row.item_title,
                summary: isObligation
                    ? `${(row as LegalEvidenceRow).category} · ${row.state}${(row as LegalEvidenceRow).due_date ? ` · Due ${(row as LegalEvidenceRow).due_date}` : ""} · ${row.item_kind}: ${row.item_title}`
                    : `Legal hold · ${row.state} · ${row.item_kind}: ${row.item_title}`,
                status: row.state,
                version: Number("version" in row ? row.version : row.item_version),
                href: `/legal?record=${encodeURIComponent(row.item_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeLegalEvidenceCursor(items.at(-1)!.id) : null,
            queryHash,
        };
    }
    async function readLegalEvidence(actor: WorkContext, metric: LegalMetricId, recordId: string, filters: Filters) {
        await requireLegalEvidence(actor, metric);
        const isObligation = metric === "active-legal-obligations";
        const { row } = await r.factsReadLegalEvidence(actor, isObligation, recordId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
        const record = isObligation
            ? { id: row.id, item_id: row.item_id, title: (row as LegalEvidenceRow).title, category: (row as LegalEvidenceRow).category, due_date: (row as LegalEvidenceRow).due_date, status: row.state, version: Number((row as LegalEvidenceRow).version), item_title: row.item_title, item_kind: row.item_kind, item_version: Number(row.item_version), href: `/legal?record=${encodeURIComponent(row.item_id)}` }
            : { id: row.id, item_id: row.item_id, status: row.state, created_at: Number((row as LegalHoldEvidenceRow).created_at), item_title: row.item_title, item_kind: row.item_kind, item_version: Number(row.item_version), href: `/legal?record=${encodeURIComponent(row.item_id)}` };
        const recordVersion = Number("version" in row ? row.version : row.item_version);
        const fields = isObligation
            ? ["title", "category", "due_date", "state", "version", "legal_item.title", "legal_item.kind", "legal_item.version"]
            : ["state", "created_at", "legal_item.title", "legal_item.kind", "legal_item.version"];
        const queryHash = await ports.digest(JSON.stringify({ providerId: legalProviderId, metric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, legalProviderId, metric, "open", [
            { id: row.id, version: isObligation ? recordVersion : null },
            { id: row.item_id, version: Number(row.item_version) },
        ], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: legalProviderId, metric, fields, record, queryHash };
    }
    async function searchFinanceEvidence(actor: WorkContext, raw: unknown) {
        const input = querySchema.parse(raw);
        await requireFinanceEvidence(actor, input.metric);
        const type = predicate(input.metric);
        const cursor = cursorValue(input.cursor);
        const { rows } = await r.
            factsSearchFinanceEvidence(type, input, actor, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, providerId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), searchFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: input.metric,
            fields: searchFields,
            items: items.map((row) => ({ id: row.id, invoice_number: row.invoice_number, kind: row.kind, invoice_date: row.invoice_date, due_date: row.due_date, currency: row.currency, total_minor: row.total_minor, outstanding_minor: row.outstanding_minor, status: row.status, version: row.version, label: row.invoice_number, summary: `${row.kind} · ${row.invoice_date} · ${row.status}`, amount: row.total_minor, href: `/finance/invoices/${encodeURIComponent(row.id)}` })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readFinanceEvidence(actor: WorkContext, metric: FinanceMetricId, recordId: string, filters: Filters) {
        await requireFinanceEvidence(actor, metric);
        const type = predicate(metric);
        const { row } = await r.factsReadFinanceEvidence(type, recordId, actor, filters);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, providerId, metric, "open", [{ id: row.id, version: row.version }], detailFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric,
            fields: detailFields,
            record: { id: row.id, invoice_number: row.invoice_number, kind: row.kind, invoice_date: row.invoice_date, due_date: row.due_date, currency: row.currency, total_minor: row.total_minor, outstanding_minor: row.outstanding_minor, status: row.status, version: row.version, subtotal_minor: row.subtotal_minor, tax_minor: row.tax_minor, posted_at: row.posted_at, party_name: row.party_name, href: `/finance/invoices/${encodeURIComponent(row.id)}` },
            queryHash,
        };
    }
    async function requirePayrollSummaryEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "payroll.read") || !hasPermission(actor, "module.people_payroll.read") || actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === payrollSummaryMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "people_payroll");
    }
    async function searchPayrollSummaryEvidence(actor: WorkContext, input: z.infer<typeof payrollSummaryQuerySchema>) {
        await requirePayrollSummaryEvidence(actor);
        const cursor = payrollSummaryCursor(input.cursor);
        const { rows } = await r.
            factsSearchPayrollSummaryEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: payrollSummaryProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, payrollSummaryProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: row.version })), payrollSummaryFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: payrollSummaryProviderId,
            metric: input.metric,
            fields: payrollSummaryFields,
            items: items.map((row) => ({
                id: row.id,
                label: `${row.period_start} to ${row.period_end}`,
                summary: `Posted · ${row.currency} · Pay date ${row.pay_date}`,
                amount: row.net_minor,
                currency: row.currency,
                status: row.status,
                version: row.version,
                href: `/people/payroll/runs/${encodeURIComponent(row.id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodePayrollSummaryCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readPayrollSummaryEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requirePayrollSummaryEvidence(actor);
        const { row } = await r.factsReadPayrollSummaryEvidence(actor, filters, recordId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = { ...row, href: `/people/payroll/runs/${encodeURIComponent(row.id)}` };
        const queryHash = await ports.digest(JSON.stringify({ providerId: payrollSummaryProviderId, metric: payrollSummaryMetric, filters, scope: [actor.companyId, actor.assignmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, payrollSummaryProviderId, payrollSummaryMetric, "open", [{ id: row.id, version: row.version }], payrollSummaryFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: payrollSummaryProviderId, metric: payrollSummaryMetric, fields: payrollSummaryFields, record, queryHash };
    }
    async function requirePeopleLeaveOvertimeEvidence(actor: WorkContext, metric: typeof approvedLeaveMetric | typeof authorizedOvertimeMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "people.read") || !hasPermission(actor, "module.people_payroll.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "people_payroll");
    }
    async function searchPeopleLeaveOvertimeEvidence(actor: WorkContext, raw: unknown) {
        const input = peopleLeaveOvertimeQuerySchema.parse(raw);
        const cursor = peopleRequestCursor(input.cursor);
        await requirePeopleLeaveOvertimeEvidence(actor, input.metric);
        const leave = input.metric === approvedLeaveMetric;
        const fields = leave ? approvedLeaveFields : authorizedOvertimeFields;
        const { rows } = await r.
            factsSearchPeopleLeaveOvertimeEvidence(leave, actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: peopleLeaveOvertimeProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, peopleLeaveOvertimeProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: row.version })), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: peopleLeaveOvertimeProviderId,
            metric: input.metric,
            fields,
            items: items.map((row) => leave ? ({
                id: row.id,
                label: row.number!,
                summary: `Approved leave · ${row.start_date} to ${row.end_date} · ${row.units} days`,
                status: row.state,
                version: row.version,
                href: `/tasks/people_payroll/${encodeURIComponent(row.id)}`,
            }) : ({
                id: row.id,
                label: `Overtime · ${row.work_date}`,
                summary: `Approved payroll handoff · ${row.minutes} minutes`,
                status: row.state,
                version: row.version,
                href: `/overtime/${encodeURIComponent(row.id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodePeopleRequestCursor(leave ? items.at(-1)!.start_date! : items.at(-1)!.work_date!, items.at(-1)!.id) : null,
            queryHash,
        };
    }
    async function readPeopleLeaveOvertimeEvidence(actor: WorkContext, metric: typeof approvedLeaveMetric | typeof authorizedOvertimeMetric, recordId: string, filters: Filters) {
        await requirePeopleLeaveOvertimeEvidence(actor, metric);
        const leave = metric === approvedLeaveMetric;
        const fields = leave ? approvedLeaveFields : authorizedOvertimeFields;
        let record: Record<string, unknown>;
        if (leave) {
            // Keep evidence reads on the exact same personal cohort as search and the
            // aggregate. `getLeave` also serves the separately-authorized medical
            // evidence workflow, whose broader `leave.evidence.read` grant must not
            // widen this executive metric.
            const row = await r.leaveEvidenceRow(actor, filters, recordId);
            if (!row) {
                const visible = await r.leaveEvidenceVisibility(actor, recordId);
                if (!visible)
                    throw new BusinessRuleError(404, "NOT_FOUND", "Leave request not found.");
                throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
            }
            record = {
                id: row.id, number: row.number, start_date: row.start_date, end_date: row.end_date,
                units: row.units, state: row.state, version: row.version,
                href: `/tasks/people_payroll/${encodeURIComponent(row.id)}`,
            };
        }
        else {
            const row = await ports.getOvertime(actor, recordId);
            const handoff = row.handoff as {
                work_date?: string;
                minutes?: number;
                request_version?: number;
            } | null;
            if (row.state !== "approved" || !handoff || !handoff.work_date || handoff.work_date < filters.from || handoff.work_date > filters.to
                || handoff.request_version !== row.version
                || actor.branchId && row.branch_id !== actor.branchId
                || actor.departmentId && row.department_id !== actor.departmentId)
                throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
            record = {
                id: row.id, work_date: handoff.work_date, minutes: String(handoff.minutes ?? 0), request_version: handoff.request_version,
                state: row.state, version: row.version, href: `/overtime/${encodeURIComponent(row.id)}`,
            };
        }
        const queryHash = await ports.digest(JSON.stringify({ providerId: peopleLeaveOvertimeProviderId, metric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, peopleLeaveOvertimeProviderId, metric, "open", [{ id: recordId, version: Number(record.version) }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: peopleLeaveOvertimeProviderId, metric, fields, record, queryHash };
    }
    async function requireHotelOccupancyEvidence(actor: WorkContext, metric: typeof hotelOccupancyMetric | HotelRoomRevenueMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "pms.read") || !hasPermission(actor, "pms.reports") || actor.departmentId || actor.beta)
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "pms");
    }
    async function searchHotelOccupancyEvidence(actor: WorkContext, input: z.infer<typeof hotelOccupancyQuerySchema>) {
        await requireHotelOccupancyEvidence(actor, input.metric);
        const cursor = hotelOccupancyCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchHotelOccupancyEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: hotelOccupancyProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, hotelOccupancyProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), hotelOccupancyFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: hotelOccupancyProviderId,
            metric: input.metric,
            fields: hotelOccupancyFields,
            items: items.map((row) => ({
                id: row.id,
                label: `${row.property_name} · ${row.business_date}`,
                summary: `Completed Night Audit · ${row.sold} sold / ${row.capacity} sellable room nights`,
                status: row.state,
                version: row.version,
                href: `/pms/reports?property=${encodeURIComponent(row.property_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeHotelOccupancyCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function hotelNightRoomRevenue(actor: WorkContext, propertyId: string, businessDate: string) {
        return await r.factsHotelNightRoomRevenue(propertyId, actor, businessDate);
    }
    async function readHotelOccupancyEvidence(actor: WorkContext, metric: typeof hotelOccupancyMetric | HotelRoomRevenueMetric, recordId: string, filters: Filters) {
        await requireHotelOccupancyEvidence(actor, metric);
        const { row } = await r.factsReadHotelOccupancyEvidence(actor, filters, metric, recordId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const capacity = Number(row.capacity);
        const sold = Number(row.sold);
        if (row.state !== "completed" || !Number.isSafeInteger(capacity) || capacity < 1 || !Number.isSafeInteger(sold) || sold < 0 || sold > capacity)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: row.id,
            property_id: row.property_id,
            property_name: row.property_name,
            currency: row.property_currency,
            business_date: row.business_date,
            state: row.state,
            sold: String(sold),
            capacity: String(capacity),
            version: row.version,
            href: `/pms/reports?property=${encodeURIComponent(row.property_id)}`,
        };
        const revenue = metric === hotelOccupancyMetric ? null : await hotelNightRoomRevenue(actor, row.property_id, row.business_date);
        if (metric !== hotelOccupancyMetric) {
            Object.assign(record, { room_revenue_minor: revenue?.revenue_minor ?? "0", invalid_revenue_count: revenue?.invalid_revenue_count ?? "0" });
        }
        const fields = metric === hotelOccupancyMetric ? hotelOccupancyFields : hotelRoomRevenueFields;
        const queryHash = await ports.digest(JSON.stringify({ providerId: hotelOccupancyProviderId, metric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, hotelOccupancyProviderId, metric, "open", [{ id: record.id, version: record.version }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: hotelOccupancyProviderId, metric, fields, record, queryHash };
    }
    async function requireHotelGuestAccountBalanceEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "pms.read") || !hasPermission(actor, "pms.reports") || actor.departmentId || actor.beta)
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === hotelGuestAccountBalanceMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "pms");
    }
    async function searchHotelGuestAccountBalanceEvidence(actor: WorkContext, input: z.infer<typeof hotelGuestAccountBalanceQuerySchema>) {
        await requireHotelGuestAccountBalanceEvidence(actor);
        const cursor = hotelGuestAccountBalanceCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchHotelGuestAccountBalanceEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: hotelGuestAccountBalanceProviderId, metric: input.metric, currency: input.filters.currency, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, hotelGuestAccountBalanceProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), hotelGuestAccountBalanceFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: hotelGuestAccountBalanceProviderId,
            metric: input.metric,
            fields: hotelGuestAccountBalanceFields,
            items: items.map((row) => ({
                id: row.id,
                label: `${row.property_name} · Open guest account`,
                summary: `${row.balance_minor} ${row.currency} posted balance · ${row.state}`,
                status: row.state,
                version: row.version,
                amount: row.balance_minor,
                currency: row.currency,
                href: `/pms/guest-accounts?property=${encodeURIComponent(row.property_id)}&guestAccount=${encodeURIComponent(row.id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeHotelGuestAccountBalanceCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readHotelGuestAccountBalanceEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireHotelGuestAccountBalanceEvidence(actor);
        const { row } = await r.factsReadHotelGuestAccountBalanceEvidence(actor, filters, recordId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const snapshot = await ports.loadPms(actor, row.property_id);
        const guestAccount = snapshot.records.find((candidate): candidate is PmsRecord<"guest_account"> => candidate.id === row.id && candidate.kind === "guest_account");
        if (!guestAccount || guestAccount.version !== Number(row.version) || !["open", "credit"].includes(guestAccount.data.state))
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: guestAccount.id,
            property_name: snapshot.property.name,
            currency: row.currency,
            state: guestAccount.data.state,
            posted_receivable_balance_minor: row.balance_minor,
            version: guestAccount.version,
            href: `/pms/guest-accounts?property=${encodeURIComponent(snapshot.property.id)}&guestAccount=${encodeURIComponent(guestAccount.id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: hotelGuestAccountBalanceProviderId, metric: hotelGuestAccountBalanceMetric, currency: filters.currency, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, hotelGuestAccountBalanceProviderId, hotelGuestAccountBalanceMetric, "open", [{ id: record.id, version: record.version }], hotelGuestAccountBalanceFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: hotelGuestAccountBalanceProviderId, metric: hotelGuestAccountBalanceMetric, fields: hotelGuestAccountBalanceFields, record, queryHash };
    }
    async function requireHotelNightAuditBlockerEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "pms.read") || !hasPermission(actor, "pms.reports") || actor.departmentId || actor.beta)
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === hotelNightAuditBlockersMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "pms");
    }
    async function searchHotelNightAuditBlockerEvidence(actor: WorkContext, input: z.infer<typeof hotelNightAuditBlockersQuerySchema>) {
        await requireHotelNightAuditBlockerEvidence(actor);
        const cursor = hotelNightAuditBlockerCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchHotelNightAuditBlockerEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: hotelNightAuditBlockersProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, hotelNightAuditBlockersProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), hotelNightAuditBlockersFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: hotelNightAuditBlockersProviderId,
            metric: input.metric,
            fields: hotelNightAuditBlockersFields,
            items: items.map((row) => ({
                id: row.id,
                label: `${row.property_name} · ${row.business_date}`,
                summary: `Blocked Night Audit job · ${row.error_code}`,
                status: row.state,
                version: row.version,
                href: `/pms/night-audit?property=${encodeURIComponent(row.property_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeHotelNightAuditBlockerCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readHotelNightAuditBlockerEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireHotelNightAuditBlockerEvidence(actor);
        const { row } = await r.factsReadHotelNightAuditBlockerEvidence(actor, filters, recordId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: row.id,
            property_id: row.property_id,
            property_name: row.property_name,
            business_date: row.business_date,
            state: row.state,
            error_code: row.error_code,
            version: Number(row.version),
            href: `/pms/night-audit?property=${encodeURIComponent(row.property_id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: hotelNightAuditBlockersProviderId, metric: hotelNightAuditBlockersMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, hotelNightAuditBlockersProviderId, hotelNightAuditBlockersMetric, "open", [{ id: record.id, version: record.version }], hotelNightAuditBlockersFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: hotelNightAuditBlockersProviderId, metric: hotelNightAuditBlockersMetric, fields: hotelNightAuditBlockersFields, record, queryHash };
    }
    async function requireProcurementEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "finance.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === "invoice-match-exceptions" && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "procurement");
    }
    async function searchSupplierBillEvidence(actor: WorkContext, input: z.infer<typeof procurementQuerySchema>) {
        await requireProcurementEvidence(actor);
        const cursor = cursorValue(input.cursor);
        const { rows } = await r.
            factsSearchSupplierBillEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: procurementProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, procurementProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), procurementSearchFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: procurementProviderId,
            metric: input.metric,
            fields: procurementSearchFields,
            items: items.map((row) => ({ id: row.id, invoice_number: row.reference, kind: "payable", invoice_date: row.invoice_date, due_date: row.due_date, currency: row.currency, total_minor: row.total_minor, outstanding_minor: "0", status: row.status, version: row.version, party_name: row.supplier_name, order_number: row.order_number, label: row.reference, summary: `${row.supplier_name} · ${row.order_number} · ${row.status}`, amount: row.total_minor, href: `/supplier-bills/${encodeURIComponent(row.id)}` })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readSupplierBillEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireProcurementEvidence(actor);
        const { latest } = await r.factsReadSupplierBillEvidence(actor, filters, recordId);
        if (!latest)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const source = await ports.getSupplierBill(actor, recordId);
        const record = {
            id: latest.id,
            reference: latest.reference,
            supplier_name: latest.supplier_name,
            order_number: latest.order_number,
            invoice_date: latest.invoice_date,
            due_date: latest.due_date,
            currency: latest.currency,
            expected_subtotal_minor: latest.expected_subtotal_minor,
            subtotal_minor: latest.subtotal_minor,
            tax_minor: latest.tax_minor,
            status: latest.status,
            quantity_matches: latest.quantity_matches,
            version: latest.version,
            fulfillment_version: Number(source.fulfillment_version),
            receipt_count: Number(source.receipt_count),
            href: `/supplier-bills/${encodeURIComponent(recordId)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: procurementProviderId, metric: "invoice-match-exceptions", filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, procurementProviderId, "invoice-match-exceptions", "open", [{ id: record.id, version: record.version }], procurementDetailFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: procurementProviderId, metric: "invoice-match-exceptions", fields: procurementDetailFields, record, queryHash };
    }
    async function requireSalesEvidence(actor: WorkContext, metric: SalesMetricId) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "commerce.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "sales");
    }
    async function searchSalesOrderEvidence(actor: WorkContext, input: z.infer<typeof salesQuerySchema>) {
        await requireSalesEvidence(actor, input.metric);
        const cursor = salesCursorValue(input.cursor, input.metric);
        const { rows } = await r.
            factsSearchSalesOrderEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: salesProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        const fields = input.metric === "sales-orders" ? salesOrderSearchFields : salesSearchFields;
        await audit(actor, salesProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: salesProviderId,
            metric: input.metric,
            fields,
            items: items.map((row) => ({
                id: row.id,
                ...(input.metric === "sales-orders" || input.metric === "sales-cancellation-value" ? { currency: row.currency, total_minor: row.total_minor, amount: row.total_minor } : {}),
                status: row.status, version: row.version,
                label: row.number,
                summary: input.metric === "sales-orders"
                    ? `Sales order · ${row.order_date} · ${row.status}`
                    : `Cancelled · ${new Date(Number(row.updated_at)).toISOString()} UTC${row.prior_approval ? " · Previously approved" : " · Draft"}`,
                href: `/commerce/orders/${encodeURIComponent(row.id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeSalesCursor(items.at(-1)!, input.metric) : null,
            queryHash,
        };
    }
    async function readSalesOrderEvidence(actor: WorkContext, metric: SalesMetricId, recordId: string, filters: Filters) {
        await requireSalesEvidence(actor, metric);
        const { row } = await r.factsReadSalesOrderEvidence(actor, metric, filters, recordId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: row.id, number: row.number, order_date: row.order_date, ...(metric === "sales-orders" ? {} : { cancelled_at: Number(row.updated_at) }), currency: row.currency,
            subtotal_minor: row.subtotal_minor, tax_minor: row.tax_minor, total_minor: row.total_minor,
            status: row.status, version: row.version, ...(metric === "sales-orders" ? {} : { prior_approval: row.prior_approval }),
            href: `/commerce/orders/${encodeURIComponent(row.id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: salesProviderId, metric, filters, recordId }));
        await ports.currentAuthority(actor);
        const fields = metric === "sales-orders" ? salesOrderDetailFields : salesDetailFields;
        await audit(actor, salesProviderId, metric, "open", [{ id: row.id, version: row.version }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: salesProviderId, metric, fields, record, queryHash };
    }
    async function requireSalesFulfillmentEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "commerce.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === salesFulfillmentMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "sales");
    }
    async function searchSalesFulfillmentEvidence(actor: WorkContext, input: z.infer<typeof salesFulfillmentQuerySchema>) {
        await requireSalesFulfillmentEvidence(actor);
        const cursor = salesFulfillmentCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchSalesFulfillmentEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: salesProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, salesProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), salesFulfillmentFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: salesProviderId,
            metric: input.metric,
            fields: salesFulfillmentFields,
            items: items.map((row) => ({ id: row.id, label: row.number, summary: `${row.status} · ${row.order_date}`, status: row.status, version: row.version, href: `/commerce/orders/${encodeURIComponent(row.id)}` })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeSalesFulfillmentCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readSalesFulfillmentEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireSalesFulfillmentEvidence(actor);
        const { row } = await r.factsReadSalesFulfillmentEvidence(actor, filters, recordId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = { ...row, href: `/commerce/orders/${encodeURIComponent(row.id)}` };
        const queryHash = await ports.digest(JSON.stringify({ providerId: salesProviderId, metric: salesFulfillmentMetric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, salesProviderId, salesFulfillmentMetric, "open", [{ id: row.id, version: row.version }], salesFulfillmentFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: salesProviderId, metric: salesFulfillmentMetric, fields: salesFulfillmentFields, record, queryHash };
    }
    async function requireFieldVisitEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "records.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === fieldVisitMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "field_service");
    }
    async function searchFieldVisitEvidence(actor: WorkContext, input: z.infer<typeof fieldVisitQuerySchema>) {
        await requireFieldVisitEvidence(actor);
        const cursor = fieldVisitCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchFieldVisitEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: "field-service-orders", metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, "field-service-orders", input.metric, "search", items.map(({ id, version }) => ({ id, version })), fieldVisitFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: "field-service-orders",
            metric: input.metric,
            fields: fieldVisitFields,
            items: items.map((row) => ({ id: row.id, label: row.id, summary: `${row.status} · ${new Date(Number(row.scheduled_at)).toISOString()} UTC`, status: row.status, version: row.version, href: `/field-service/${encodeURIComponent(row.id)}` })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeFieldVisitCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readFieldVisitEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireFieldVisitEvidence(actor);
        let order: Record<string, unknown>;
        try {
            ({ order } = await ports.fieldServiceDetail(actor, recordId));
        }
        catch (error) {
            if (error instanceof BusinessRuleError && error.status === 404)
                throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
            throw error;
        }
        const scheduledAt = Number(order.scheduled_at), status = String(order.status);
        const eligible = Number.isSafeInteger(scheduledAt) && scheduledAt >= Date.parse(filters.from) && scheduledAt < Date.parse(filters.to) + 86400000
            && ["scheduled", "dispatched", "in_visit", "exception", "completed", "billed"].includes(status);
        if (!eligible)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const completedAt = order.visit_completed_at === null || order.visit_completed_at === undefined ? null : Number(order.visit_completed_at);
        const record = { id: recordId, scheduled_at: scheduledAt, visit_completed_at: completedAt, status, version: Number(order.version), href: `/field-service/${encodeURIComponent(recordId)}` };
        const queryHash = await ports.digest(JSON.stringify({ providerId: "field-service-orders", metric: fieldVisitMetric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, "field-service-orders", fieldVisitMetric, "open", [{ id: recordId, version: record.version }], fieldVisitFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: "field-service-orders", metric: fieldVisitMetric, fields: fieldVisitFields, record, queryHash };
    }
    async function requireQualityEvidence(actor: WorkContext, metric: typeof qualityMetrics[number]) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "quality.read") || actor.departmentId)
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "quality");
    }
    async function searchQualityInspectionEvidence(actor: WorkContext, raw: unknown) {
        const input = qualityQuerySchema.parse(raw);
        await requireQualityEvidence(actor, input.metric);
        if (input.metric !== "inspection-pass-rate")
            return searchQualityHoldEvidence(actor, input);
        const cursor = qualityCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchQualityInspectionEvidence(input, actor, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: qualityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, qualityProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: Number(row.reviewed_version) })), qualitySearchFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: qualityProviderId,
            metric: input.metric,
            fields: qualitySearchFields,
            items: items.map((row) => ({
                id: row.id,
                label: row.product_name,
                summary: `${row.result === "pass" ? "Passed" : "Failed"} · ${row.warehouse_name} · ${new Date(Number(row.created_at)).toISOString()} UTC`,
                status: row.result,
                version: Number(row.reviewed_version),
                href: `/quality/${encodeURIComponent(row.hold_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeQualityCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function searchQualityHoldEvidence(actor: WorkContext, input: z.infer<typeof qualityQuerySchema>) {
        const releases = input.metric === "quality-releases";
        const cursor = qualityCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchQualityHoldEvidence(releases, input, actor, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const searchFields = releases
            ? ["status", "released_at", "version", "product_name", "warehouse_name"]
            : ["status", "updated_at", "version", "product_name", "warehouse_name"];
        const queryHash = await ports.digest(JSON.stringify({ providerId: qualityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, qualityProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: Number(row.reviewed_version) })), searchFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: qualityProviderId,
            metric: input.metric,
            fields: searchFields,
            items: items.map((row) => ({
                id: row.id,
                label: row.product_name,
                summary: `${releases ? "Released" : "Open hold"} · ${row.warehouse_name} · ${row.result}`,
                status: row.result,
                version: Number(row.reviewed_version),
                href: `/quality/${encodeURIComponent(row.hold_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeQualityCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readQualityInspectionEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireQualityEvidence(actor, "inspection-pass-rate");
        const { row } = await r.factsReadQualityInspectionEvidence(recordId, actor, filters);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const reviewedVersion = Number(row.reviewed_version);
        const record = {
            id: row.id,
            hold_id: row.hold_id,
            result: row.result,
            evidence: row.evidence,
            inspected_by: row.inspected_by,
            inspected_at: Number(row.created_at),
            reviewed_version: reviewedVersion,
            product_name: row.product_name,
            warehouse_name: row.warehouse_name,
            href: `/quality/${encodeURIComponent(row.hold_id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: qualityProviderId, metric: "inspection-pass-rate", filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, qualityProviderId, "inspection-pass-rate", "open", [{ id: row.id, version: reviewedVersion }], qualityDetailFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: qualityProviderId, metric: "inspection-pass-rate", fields: qualityDetailFields, record, queryHash };
    }
    async function readQualityHoldEvidence(actor: WorkContext, metric: "quality-holds" | "quality-releases", recordId: string, filters: Filters) {
        await requireQualityEvidence(actor, metric);
        const hold = await ports.getQualityHold(actor, recordId);
        const releasedAt = hold.released_at === null || hold.released_at === undefined ? null : Number(hold.released_at);
        const inSelectedPeriod = releasedAt !== null && releasedAt >= Date.parse(filters.from) && releasedAt < Date.parse(filters.to) + 86400000;
        const isReleased = hold.status === "released";
        if (metric === "quality-holds" ? ["released", "cancelled", "disposed"].includes(String(hold.status)) : !isReleased || !inSelectedPeriod)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const fields = ["product_name", "warehouse_name", "reason", "status", "due_date", "created_at", "updated_at", "version", "released_by", "released_at", "release_reason"];
        const record = {
            id: String(hold.id),
            product_name: String(hold.product_name),
            warehouse_name: String(hold.warehouse_name),
            reason: String(hold.reason),
            status: String(hold.status),
            due_date: String(hold.due_date),
            created_at: Number(hold.created_at),
            updated_at: Number(hold.updated_at),
            version: Number(hold.version),
            released_by: hold.released_by === null ? null : String(hold.released_by),
            released_at: releasedAt,
            release_reason: hold.release_reason === null ? null : String(hold.release_reason),
            href: `/quality/${encodeURIComponent(String(hold.id))}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: qualityProviderId, metric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, qualityProviderId, metric, "open", [{ id: record.id, version: record.version }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: qualityProviderId, metric, fields, record, queryHash };
    }
    async function requireManufacturingCostEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "records.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === manufacturingCostMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "manufacturing");
    }
    async function searchManufacturingCostEvidence(actor: WorkContext, input: z.infer<typeof manufacturingCostQuerySchema>) {
        await requireManufacturingCostEvidence(actor);
        const cursor = manufacturingCostCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchManufacturingCostEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: manufacturingCostProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, manufacturingCostProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), manufacturingCostSearchFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: manufacturingCostProviderId,
            metric: input.metric,
            fields: manufacturingCostSearchFields,
            items: items.map((row) => ({ id: row.id, label: row.order_id, summary: `${row.movement_kind.replaceAll("_", " ")} · ${row.product_name} · ${new Date(Number(row.posted_at)).toISOString().slice(0, 10)}`, amount: row.amount_minor, currency: row.currency, status: row.journal_status, version: row.version, href: `/manufacturing/${encodeURIComponent(row.order_id)}` })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeManufacturingCostCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readManufacturingCostEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireManufacturingCostEvidence(actor);
        const { row } = await r.factsReadManufacturingCostEvidence(actor, filters, recordId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        // Use the module detail reader as the final record-level authority check. Only the
        // cost movement projection below is returned to Executive; other order detail stays in its module.
        await ports.productionDetail(actor, row.order_id);
        const record = {
            id: row.id, order_id: row.order_id, movement_kind: row.movement_kind, amount_minor: row.amount_minor,
            posted_at: Number(row.posted_at), journal_status: row.journal_status, currency: row.currency,
            order_status: row.order_status, order_version: Number(row.order_version), product_name: row.product_name,
            warehouse_name: row.warehouse_name, version: Number(row.version), href: `/manufacturing/${encodeURIComponent(row.order_id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: manufacturingCostProviderId, metric: manufacturingCostMetric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, manufacturingCostProviderId, manufacturingCostMetric, "open", [{ id: row.id, version: record.version }], manufacturingCostDetailFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: manufacturingCostProviderId, metric: manufacturingCostMetric, fields: manufacturingCostDetailFields, record, queryHash };
    }
    async function requireCustomerActivityEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "commerce.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === customerActivityMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
    }
    async function searchCustomerActivityEvidence(actor: WorkContext, input: z.infer<typeof customerActivityQuerySchema>) {
        await requireCustomerActivityEvidence(actor);
        const cursor = customerActivityCursor(input.cursor);
        const { rows } = await r.
            factsSearchCustomerActivityEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: customerActivityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, customerActivityProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version: Number(version) })), customerActivityFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: customerActivityProviderId,
            metric: input.metric,
            fields: customerActivityFields,
            items: items.map((row) => ({
                id: row.id,
                label: row.customer_name,
                summary: `${row.source_type.replaceAll("_", " ")} · ${row.title} · ${row.status} · ${new Date(Number(row.at)).toISOString().slice(0, 10)}`,
                status: row.status,
                version: Number(row.version),
                href: `/customers/${encodeURIComponent(row.customer_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeCustomerActivityCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readCustomerActivityEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireCustomerActivityEvidence(actor);
        const { row } = await r.factsReadCustomerActivityEvidence(actor, recordId, filters);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const queryHash = await ports.digest(JSON.stringify({ providerId: customerActivityProviderId, metric: customerActivityMetric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, customerActivityProviderId, customerActivityMetric, "open", [{ id: row.id, version: Number(row.version) }], customerActivityFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: customerActivityProviderId, metric: customerActivityMetric, fields: customerActivityFields, record: { ...row, version: Number(row.version), at: Number(row.at) }, queryHash };
    }
    async function requireSocialActivityEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "records.read"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === socialActivityMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "collaboration");
    }
    async function searchSocialActivityEvidence(actor: WorkContext, input: z.infer<typeof socialActivityQuerySchema>) {
        await requireSocialActivityEvidence(actor);
        const cursor = socialActivityCursor(input.cursor);
        const { rows } = await r.
            factsSearchSocialActivityEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: socialActivityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, socialActivityProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version: Number(version) })), socialActivityFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: socialActivityProviderId,
            metric: input.metric,
            fields: socialActivityFields,
            items: items.map((row) => ({
                id: row.id,
                label: row.title,
                summary: `${row.source_type} · ${row.activity_actor} · ${new Date(Number(row.at)).toISOString().slice(0, 10)}`,
                status: row.source_type,
                version: Number(row.version),
                href: row.href,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeSocialActivityCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readSocialActivityEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireSocialActivityEvidence(actor);
        const { row } = await r.factsReadSocialActivityEvidence(actor, recordId, filters);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const queryHash = await ports.digest(JSON.stringify({ providerId: socialActivityProviderId, metric: socialActivityMetric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, socialActivityProviderId, socialActivityMetric, "open", [{ id: row.id, version: Number(row.version) }], socialActivityFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: socialActivityProviderId, metric: socialActivityMetric, fields: socialActivityFields, record: { ...row, at: Number(row.at), version: Number(row.version) }, queryHash };
    }
    async function requireSupportIncidentEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "support.read") || !catalog.some((item) => item.id === supportIncidentMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "support");
    }
    async function searchSupportIncidentEvidence(actor: WorkContext, input: z.infer<typeof supportIncidentQuerySchema>) {
        await requireSupportIncidentEvidence(actor);
        const cursor = supportIncidentCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchSupportIncidentEvidence(cursor, input, actor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: supportIncidentProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, supportIncidentProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: supportIncidentNumber(row.version, "version") })), supportIncidentFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: supportIncidentProviderId,
            metric: input.metric,
            fields: supportIncidentFields,
            items: items.map((row) => ({
                id: encodeSupportIncidentRecordId(row.id),
                label: row.title,
                summary: `${row.severity} · ${row.status} · Due ${row.due_date}`,
                status: row.status,
                href: `/support/incidents/${encodeURIComponent(row.id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeSupportIncidentCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readSupportIncidentEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireSupportIncidentEvidence(actor);
        const id = decodeSupportIncidentRecordId(recordId), row = await ports.getSupportIncident(actor, id);
        if (!["open", "investigating", "mitigated"].includes(String(row.status)))
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const version = supportIncidentNumber(row.version, "version");
        const record = {
            id: recordId,
            title: String(row.title),
            severity: String(row.severity),
            status: String(row.status),
            due_date: String(row.due_date),
            created_at: supportIncidentNumber(row.created_at, "creation time"),
            updated_at: supportIncidentNumber(row.updated_at, "update time"),
            version,
            href: `/support/incidents/${encodeURIComponent(id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: supportIncidentProviderId, metric: supportIncidentMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, supportIncidentProviderId, supportIncidentMetric, "open", [{ id, version }], supportIncidentFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: supportIncidentProviderId, metric: supportIncidentMetric, fields: supportIncidentFields, record, queryHash };
    }
    async function requireSupportTicketEvidence(actor: WorkContext, metric: SupportTicketMetricId) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "support.read") || !catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "support");
    }
    async function searchSupportTicketEvidence(actor: WorkContext, input: z.infer<typeof supportTicketQuerySchema>) {
        await requireSupportTicketEvidence(actor, input.metric);
        const cursor = supportTicketCursorValue(input.cursor);
        const overdue = input.metric === "overdue-support-sla";
        const { rows } = await r.
            factsSearchSupportTicketEvidence(input, cursor, overdue, actor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const fields = supportTicketFieldsFor(input.metric);
        const queryHash = await ports.digest(JSON.stringify({ providerId: supportTicketProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, supportTicketProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: supportTicketNumber(row.version, "version") })), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: supportTicketProviderId,
            metric: input.metric,
            fields,
            items: items.map((row) => {
                const dueAt = overdue ? supportTicketNumber(row.sla_due_at, "SLA due time") : null;
                return {
                    id: encodeSupportTicketRecordId(row.id),
                    label: row.title,
                    summary: overdue ? `Overdue SLA · due ${new Date(dueAt!).toISOString()}` : `${row.status} · due ${row.due_date}`,
                    status: row.status,
                    href: `/support/${encodeURIComponent(row.id)}`,
                };
            }),
            hasMore,
            nextCursor: hasMore && items.length ? encodeSupportTicketCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readSupportTicketEvidence(actor: WorkContext, metric: SupportTicketMetricId, recordId: string, filters: Filters) {
        await requireSupportTicketEvidence(actor, metric);
        const id = decodeSupportTicketRecordId(recordId);
        const overdue = metric === "overdue-support-sla";
        const { row } = await r.factsReadSupportTicketEvidence(overdue, metric, id, actor);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const fields = supportTicketFieldsFor(metric);
        const record = {
            id: recordId,
            title: String(row.title),
            status: String(row.status),
            due_date: String(row.due_date),
            updated_at: supportTicketNumber(row.updated_at, "update time"),
            version: supportTicketNumber(row.version, "version"),
            ...(overdue ? { sla_state: String(row.sla_state), sla_due_at: supportTicketNumber(row.sla_due_at, "SLA due time") } : {}),
            href: `/support/${encodeURIComponent(id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: supportTicketProviderId, metric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, supportTicketProviderId, metric, "open", [{ id, version: record.version }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: supportTicketProviderId, metric, fields, record, queryHash };
    }
    async function requireDuplicateReviewEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "commerce.read") || !hasPermission(actor, "commerce.sales") || actor.branchId || actor.departmentId || !catalog.some((item) => item.id === duplicateReviewMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "customer_authority");
    }
    async function searchDuplicateReviewEvidence(actor: WorkContext, input: z.infer<typeof duplicateReviewQuerySchema>) {
        await requireDuplicateReviewEvidence(actor);
        const cursor = duplicateReviewCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchDuplicateReviewEvidence(cursor, input, actor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: duplicateReviewProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, duplicateReviewProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: duplicateReviewNumber(row.version, "version") })), duplicateReviewFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: duplicateReviewProviderId,
            metric: input.metric,
            fields: duplicateReviewFields,
            items: items.map((row) => ({
                id: encodeDuplicateReviewRecordId(row.id),
                label: `${row.left_code} · ${row.left_name} ↔ ${row.right_code} · ${row.right_name}`,
                summary: `Pending · updated ${new Date(duplicateReviewNumber(row.updated_at, "update time")).toISOString().slice(0, 10)}`,
                status: row.status,
                href: `/customer-duplicates/${encodeURIComponent(row.id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeDuplicateReviewCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readDuplicateReviewEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireDuplicateReviewEvidence(actor);
        const id = decodeDuplicateReviewRecordId(recordId);
        const { row } = await r.factsReadDuplicateReviewEvidence(actor, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: recordId,
            left_name: row.left_name,
            left_code: row.left_code,
            right_name: row.right_name,
            right_code: row.right_code,
            status: row.status,
            created_at: duplicateReviewNumber(row.created_at, "creation time"),
            updated_at: duplicateReviewNumber(row.updated_at, "update time"),
            version: duplicateReviewNumber(row.version, "version"),
            href: `/customer-duplicates/${encodeURIComponent(id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: duplicateReviewProviderId, metric: duplicateReviewMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, duplicateReviewProviderId, duplicateReviewMetric, "open", [{ id, version: record.version }], duplicateReviewFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: duplicateReviewProviderId, metric: duplicateReviewMetric, fields: duplicateReviewFields, record, queryHash };
    }
    async function requireIdentityChangeEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "commerce.read") || !hasPermission(actor, "commerce.sales") || actor.branchId || actor.departmentId || !catalog.some((item) => item.id === identityChangeMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "customer_authority");
    }
    async function searchIdentityChangeEvidence(actor: WorkContext, input: z.infer<typeof identityChangeQuerySchema>) {
        await requireIdentityChangeEvidence(actor);
        const cursor = identityChangeCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchIdentityChangeEvidence(cursor, input, actor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: identityChangeProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, identityChangeProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: identityChangeNumber(row.version, "event version") })), identityChangeFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: identityChangeProviderId,
            metric: input.metric,
            fields: identityChangeFields,
            items: items.map((row) => ({
                id: encodeIdentityChangeRecordId(row.id),
                label: `${row.customer_code} · ${row.customer_name} · ${row.action}`,
                summary: `Change ${row.change_status} · ${new Date(identityChangeNumber(row.created_at, "event time")).toISOString().slice(0, 10)}`,
                status: row.action,
                href: `/customer-identity/${encodeURIComponent(row.change_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeIdentityChangeCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readIdentityChangeEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireIdentityChangeEvidence(actor);
        const id = decodeIdentityChangeRecordId(recordId);
        const { row } = await r.factsReadIdentityChangeEvidence(actor, filters, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: recordId,
            change_id: row.change_id,
            action: row.action,
            customer_version: identityChangeNumber(row.customer_version, "customer version"),
            created_at: identityChangeNumber(row.created_at, "event time"),
            version: identityChangeNumber(row.version, "event version"),
            change_status: row.change_status,
            change_version: identityChangeNumber(row.change_version, "change version"),
            customer_name: row.customer_name,
            customer_code: row.customer_code,
            href: `/customer-identity/${encodeURIComponent(row.change_id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: identityChangeProviderId, metric: identityChangeMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, identityChangeProviderId, identityChangeMetric, "open", [{ id: row.id, version: record.version }], identityChangeFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: identityChangeProviderId, metric: identityChangeMetric, fields: identityChangeFields, record, queryHash };
    }
    async function requireOrganizationEvidence(actor: WorkContext, metric: typeof organizationNodeMetric | typeof pendingStructureMetric) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "organization.manage") || !catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "organization");
    }
    async function searchOrganizationNodeEvidence(actor: WorkContext, input: z.infer<typeof organizationNodeQuerySchema>) {
        await requireOrganizationEvidence(actor, organizationNodeMetric);
        const cursor = organizationEvidenceCursor(input.cursor);
        const { rows } = await r.
            factsSearchOrganizationNodeEvidence(cursor, input, actor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: organizationProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, organizationProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.hierarchy_revision, "hierarchy revision") })), organizationNodeFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: organizationProviderId,
            metric: input.metric,
            fields: organizationNodeFields,
            items: items.map((row) => ({
                id: encodeOrganizationRecordId(organizationNodeMetric, row.id),
                label: `${row.display_name} · ${row.kind}`,
                summary: `${row.kind} · ${row.branch_name ?? "Company-wide"}`,
                status: row.status,
                href: "/people/organization",
            })),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ name: items.at(-1)!.display_name, id: items.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readOrganizationNodeEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireOrganizationEvidence(actor, organizationNodeMetric);
        const id = decodeOrganizationRecordId(recordId, organizationNodeMetric);
        const { row } = await r.factsReadOrganizationNodeEvidence(actor, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: recordId,
            kind: row.kind,
            slug: row.slug,
            display_name: row.display_name,
            status: row.status,
            branch_name: row.branch_name ?? "Company-wide",
            parent_name: row.parent_name ?? "Company-wide",
            hierarchy_revision: organizationEvidenceNumber(row.hierarchy_revision, "hierarchy revision"),
            href: "/people/organization",
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: organizationProviderId, metric: organizationNodeMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, organizationProviderId, organizationNodeMetric, "open", [{ id: row.id, version: record.hierarchy_revision }], organizationNodeFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: organizationProviderId, metric: organizationNodeMetric, fields: organizationNodeFields, record, queryHash };
    }
    async function requirePendingStructureEvidence(actor: WorkContext) {
        await requireOrganizationEvidence(actor, pendingStructureMetric);
        if (actor.departmentId || !await ports.canReadOrganizationChanges(actor))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
    }
    async function searchPendingStructureEvidence(actor: WorkContext, input: z.infer<typeof pendingStructureQuerySchema>) {
        await requirePendingStructureEvidence(actor);
        const cursor = organizationEvidenceCursor(input.cursor);
        const { rows } = await r.
            factsSearchPendingStructureEvidence(cursor, input, actor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: organizationProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, organizationProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.version, "change version") })), pendingStructureFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: organizationProviderId,
            metric: input.metric,
            fields: pendingStructureFields,
            items: items.map((row) => ({
                id: encodeOrganizationRecordId(pendingStructureMetric, row.id),
                label: `${row.action} ${row.node_kind} · ${row.display_name}`,
                summary: `Submitted · ${row.branch_name}`,
                status: row.state,
                href: organizationChangeHref(row.id, actor.companyId),
            })),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ at: organizationEvidenceNumber(items.at(-1)!.updated_at, "update time"), id: items.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readPendingStructureEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requirePendingStructureEvidence(actor);
        const id = decodeOrganizationRecordId(recordId, pendingStructureMetric);
        const { row } = await r.factsReadPendingStructureEvidence(actor, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: recordId,
            action: row.action,
            node_kind: row.node_kind,
            display_name: row.display_name,
            state: row.state,
            version: organizationEvidenceNumber(row.version, "change version"),
            created_at: organizationEvidenceNumber(row.created_at, "creation time"),
            updated_at: organizationEvidenceNumber(row.updated_at, "update time"),
            branch_name: row.branch_name,
            href: organizationChangeHref(row.id, actor.companyId),
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: organizationProviderId, metric: pendingStructureMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, organizationProviderId, pendingStructureMetric, "open", [{ id: row.id, version: record.version }], pendingStructureFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: organizationProviderId, metric: pendingStructureMetric, fields: pendingStructureFields, record, queryHash };
    }
    async function identityAccessUserNames(userIds: string[]) {
        const ids = [...new Set(userIds)].slice(0, 50);
        if (!ids.length)
            return new Map<string, string>();
        return await r.factsIdentityAccessUserNames(ids);
    }
    async function requireIdentityAccessEvidence(actor: WorkContext, metric: typeof activeAssignmentsMetric | typeof pendingAccessReviewsMetric) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor), permission = metric === activeAssignmentsMetric ? "iam.read" : "iam.review";
        if (!hasPermission(actor, permission) || !catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "identity_access");
    }
    async function searchActiveAssignmentEvidence(actor: WorkContext, input: z.infer<typeof activeAssignmentsQuerySchema>) {
        await requireIdentityAccessEvidence(actor, activeAssignmentsMetric);
        const cursor = identityAccessCursor(input.cursor);
        if (cursor && cursor.userId === undefined)
            throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
        const now = ports.clock.now();
        const { rows } = await r.
            factsSearchActiveAssignmentEvidence(actor, cursor, input, now);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const userNames = await identityAccessUserNames(items.map((row) => row.user_id));
        const queryHash = await ports.digest(JSON.stringify({ providerId: activeAssignmentsProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, activeAssignmentsProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.version, "assignment version") })), activeAssignmentFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: activeAssignmentsProviderId,
            metric: input.metric,
            fields: activeAssignmentFields,
            items: items.map((row) => ({
                id: encodeIdentityAccessRecordId(activeAssignmentsMetric, row.id),
                label: `${userNames.get(row.user_id) ?? row.user_id} · ${row.role_name}`,
                summary: `${row.scope_kind} · ${row.scope_name} · expires ${new Date(organizationEvidenceNumber(row.valid_until, "expiry time")).toISOString().slice(0, 10)}`,
                status: "active",
                href: "/people/settings?section=users",
            })),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ userId: items.at(-1)!.user_id, role: items.at(-1)!.role, recordId: items.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readActiveAssignmentEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireIdentityAccessEvidence(actor, activeAssignmentsMetric);
        const id = decodeIdentityAccessRecordId(recordId, activeAssignmentsMetric);
        const now = ports.clock.now();
        const { row } = await r.factsReadActiveAssignmentEvidence(actor, now, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const names = await identityAccessUserNames([row.user_id]);
        const record = {
            id: recordId,
            user_id: row.user_id,
            user_name: names.get(row.user_id) ?? null,
            role: row.role,
            role_name: row.role_name,
            scope_kind: row.scope_kind,
            scope_node_id: row.scope_node_id,
            scope_name: row.scope_name,
            branch_name: row.branch_name,
            source: row.source,
            valid_from: organizationEvidenceNumber(row.valid_from, "start time"),
            valid_until: organizationEvidenceNumber(row.valid_until, "expiry time"),
            version: organizationEvidenceNumber(row.version, "assignment version"),
            href: "/people/settings?section=users",
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: activeAssignmentsProviderId, metric: activeAssignmentsMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, activeAssignmentsProviderId, activeAssignmentsMetric, "open", [{ id: row.id, version: record.version }], activeAssignmentFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: activeAssignmentsProviderId, metric: activeAssignmentsMetric, fields: activeAssignmentFields, record, queryHash };
    }
    async function searchPendingAccessReviewEvidence(actor: WorkContext, input: z.infer<typeof pendingAccessReviewsQuerySchema>) {
        await requireIdentityAccessEvidence(actor, pendingAccessReviewsMetric);
        const cursor = identityAccessCursor(input.cursor);
        if (cursor && cursor.createdAt === undefined)
            throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
        const { rows } = await r.
            factsSearchPendingAccessReviewEvidence(actor, cursor, input);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const userNames = await identityAccessUserNames(items.map((row) => row.user_id));
        const queryHash = await ports.digest(JSON.stringify({ providerId: pendingAccessReviewsProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, pendingAccessReviewsProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.campaign_version, "campaign version") })), pendingAccessReviewFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: pendingAccessReviewsProviderId,
            metric: input.metric,
            fields: pendingAccessReviewFields,
            items: items.map((row) => ({
                id: encodeIdentityAccessRecordId(pendingAccessReviewsMetric, row.id),
                label: `${userNames.get(row.user_id) ?? row.user_id} · ${row.role}`,
                summary: `${row.campaign_name} · ${row.scope_name} · due ${new Date(organizationEvidenceNumber(row.campaign_due_at, "campaign due time")).toISOString().slice(0, 10)}`,
                status: row.decision,
                href: `/people/settings?section=reviews#${encodeURIComponent(row.campaign_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ createdAt: organizationEvidenceNumber(items.at(-1)!.campaign_created_at, "campaign creation time"), recordId: items.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readPendingAccessReviewEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireIdentityAccessEvidence(actor, pendingAccessReviewsMetric);
        const id = decodeIdentityAccessRecordId(recordId, pendingAccessReviewsMetric);
        const { row } = await r.factsReadPendingAccessReviewEvidence(actor, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const names = await identityAccessUserNames([row.user_id]);
        const record = {
            id: recordId,
            campaign_id: row.campaign_id,
            campaign_name: row.campaign_name,
            campaign_status: row.campaign_status,
            campaign_due_at: organizationEvidenceNumber(row.campaign_due_at, "campaign due time"),
            campaign_version: organizationEvidenceNumber(row.campaign_version, "campaign version"),
            user_id: row.user_id,
            user_name: names.get(row.user_id) ?? null,
            role: row.role,
            scope_kind: row.scope_kind,
            scope_node_id: row.scope_node_id,
            scope_name: row.scope_name,
            branch_name: row.branch_name,
            source: row.source,
            valid_until: organizationEvidenceNumber(row.valid_until, "assignment expiry"),
            assignment_version: organizationEvidenceNumber(row.assignment_version, "source assignment version"),
            decision: row.decision,
            href: `/people/settings?section=reviews#${encodeURIComponent(row.campaign_id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: pendingAccessReviewsProviderId, metric: pendingAccessReviewsMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, pendingAccessReviewsProviderId, pendingAccessReviewsMetric, "open", [{ id: row.id, version: record.campaign_version }], pendingAccessReviewFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: pendingAccessReviewsProviderId, metric: pendingAccessReviewsMetric, fields: pendingAccessReviewFields, record, queryHash };
    }
    async function requireCommercialEvidence(actor: WorkContext, metric: typeof activeSubscriptionsMetric | typeof overdueInvoicesMetric) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (actor.role !== "holding_owner" || actor.assignmentScope !== "holding" || !actor.holdingId || !hasPermission(actor, "company.owner")
            || !catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "commercial");
    }
    async function searchCommercialSubscriptions(actor: WorkContext, input: z.infer<typeof activeSubscriptionsQuerySchema>) {
        await requireCommercialEvidence(actor, activeSubscriptionsMetric);
        const cursor = commercialCursor(input.cursor);
        const now = ports.clock.now();
        const { rows } = await r.
            factsSearchCommercialSubscriptions(cursor, input, actor, now);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: activeSubscriptionsProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.holdingId, actor.companyId, actor.assignmentId] }));
        await ports.currentAuthority(actor);
        const enrollmentRows = items.filter((row) => row.source_kind === "enrollment");
        const legacyRows = items.filter((row) => row.source_kind === "legacy");
        await audit(actor, activeSubscriptionsProviderId, input.metric, "search", enrollmentRows.map((row) => ({ id: row.record_id, version: organizationEvidenceNumber(row.record_version, "billing account version") })), activeSubscriptionFields, queryHash);
        await audit(actor, activeSubscriptionsProviderId, input.metric, "search", legacyRows.map((row) => ({ id: row.record_id, version: organizationEvidenceNumber(row.record_version, "legacy subscription update time") })), legacySubscriptionFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: activeSubscriptionsProviderId,
            metric: input.metric,
            fields: [...new Set([...activeSubscriptionFields, ...legacySubscriptionFields])],
            items: items.map((row) => ({
                id: encodeCommercialSubscriptionId(row.source_kind, row.record_id),
                label: `${row.plan_name} · ${row.status}`,
                summary: row.source_kind === "legacy" ? "Legacy subscription snapshot" : `Started ${new Date(organizationEvidenceNumber(row.started_at!, "subscription start time")).toISOString().slice(0, 10)}${row.effective_at ? ` · Ends ${new Date(organizationEvidenceNumber(row.effective_at, "subscription effective time")).toISOString().slice(0, 10)}` : ""}`,
                status: row.status,
                href: `/settings?section=usage&subscription=${row.source_kind}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ sourceKind: items.at(-1)!.source_kind, recordId: items.at(-1)!.record_id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readCommercialSubscription(actor: WorkContext, recordId: string, filters: Filters) {
        await requireCommercialEvidence(actor, activeSubscriptionsMetric);
        const decoded = decodeCommercialSubscriptionId(recordId);
        const now = ports.clock.now();
        const { row } = await r.factsReadCommercialSubscription(decoded, actor, now);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const fields = row.source_kind === "legacy" ? legacySubscriptionFields : activeSubscriptionFields;
        const record = {
            id: recordId,
            source: row.source_kind,
            plan_name: row.plan_name,
            status: row.status,
            started_at: row.started_at === null ? null : organizationEvidenceNumber(row.started_at, "subscription start time"),
            ends_at: row.effective_at === null ? null : organizationEvidenceNumber(row.effective_at, "subscription end time"),
            version: organizationEvidenceNumber(row.record_version, row.source_kind === "legacy" ? "legacy subscription update time" : "billing account version"),
            href: `/settings?section=usage&subscription=${row.source_kind}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: activeSubscriptionsProviderId, metric: activeSubscriptionsMetric, filters, scope: [actor.holdingId, actor.companyId, actor.assignmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, activeSubscriptionsProviderId, activeSubscriptionsMetric, "open", [{ id: row.record_id, version: record.version }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: activeSubscriptionsProviderId, metric: activeSubscriptionsMetric, fields, record, queryHash };
    }
    async function searchOverdueInvoiceEvidence(actor: WorkContext, input: z.infer<typeof overdueInvoicesQuerySchema>) {
        await requireCommercialEvidence(actor, overdueInvoicesMetric);
        const cursorValue = (() => {
            if (!input.cursor)
                return null;
            try {
                const value = JSON.parse(Buffer.from(input.cursor, "base64url").toString("utf8")) as {
                    dueAt?: unknown;
                    id?: unknown;
                };
                return { dueAt: z.number().int().safe().parse(value.dueAt), id: z.string().min(1).max(200).parse(value.id) };
            }
            catch {
                throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
            }
        })();
        const now = ports.clock.now();
        const end = Date.parse(input.filters.to) + 86400000;
        const { rows } = await r.
            factsSearchOverdueInvoiceEvidence(cursorValue, input, actor, end, now);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: overdueInvoicesProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.holdingId, actor.companyId, actor.assignmentId] }));
        await ports.currentAuthority(actor);
        await audit(actor, overdueInvoicesProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.updated_at, "invoice update time") })), overdueInvoiceFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: overdueInvoicesProviderId,
            metric: input.metric,
            fields: overdueInvoiceFields,
            items: items.map((row) => ({
                id: encodeOverdueInvoiceId(row.id),
                label: `${row.kind === "main" ? "Main invoice" : "Adjustment invoice"} · ${row.status}`,
                summary: `Due ${new Date(organizationEvidenceNumber(row.due_at, "invoice due time")).toISOString().slice(0, 10)} · Service period ${row.month}`,
                amount: String(row.total_satang),
                currency: "THB",
                status: row.status,
                href: `/settings?section=usage&invoice=${encodeURIComponent(row.id)}`,
            })),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ dueAt: organizationEvidenceNumber(items.at(-1)!.due_at, "invoice due time"), id: items.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readOverdueInvoiceEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireCommercialEvidence(actor, overdueInvoicesMetric);
        const id = decodeOverdueInvoiceId(recordId);
        const now = ports.clock.now();
        const { row } = await r.factsReadOverdueInvoiceEvidence(actor, filters, now, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: recordId,
            kind: row.kind,
            status: row.status,
            due_at: organizationEvidenceNumber(row.due_at, "invoice due time"),
            total_satang: String(row.total_satang),
            updated_at: organizationEvidenceNumber(row.updated_at, "invoice update time"),
            month: row.month,
            href: `/settings?section=usage&invoice=${encodeURIComponent(row.id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: overdueInvoicesProviderId, metric: overdueInvoicesMetric, filters, scope: [actor.holdingId, actor.companyId, actor.assignmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, overdueInvoicesProviderId, overdueInvoicesMetric, "open", [{ id: row.id, version: record.updated_at }], overdueInvoiceFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: overdueInvoicesProviderId, metric: overdueInvoicesMetric, fields: overdueInvoiceFields, record, queryHash };
    }
    async function requireCloseBlockerEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "finance.read") || actor.branchId || actor.departmentId || !catalog.some((item) => item.id === closeBlockersMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "finance");
    }
    async function searchCloseBlockerEvidence(actor: WorkContext, input: z.infer<typeof closeBlockersQuerySchema>) {
        await requireCloseBlockerEvidence(actor);
        const cursor = closeBlockerCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchCloseBlockerEvidence(cursor, input, actor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: closeBlockersProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        // Close-check rows have no numeric version column; use a row-local version marker while source_version distinguishes check runs.
        await audit(actor, closeBlockersProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: 1 })), closeBlockersFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: closeBlockersProviderId,
            metric: input.metric,
            fields: closeBlockersFields,
            items: items.map((row) => ({
                id: encodeCloseBlockerRecordId(row.id),
                label: `${row.period_name} · ${row.check_key}`,
                summary: `Block · ${closeBlockerCount(row.count_value)} current items · checked ${new Date(closeBlockerNumber(row.checked_at, "check time")).toISOString().slice(0, 10)}`,
                status: row.status,
                href: "/finance/close",
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeCloseBlockerCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readCloseBlockerEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireCloseBlockerEvidence(actor);
        const id = decodeCloseBlockerRecordId(recordId);
        const { row } = await r.factsReadCloseBlockerEvidence(actor, filters, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: recordId,
            period_id: row.period_id,
            period_name: row.period_name,
            period_end_date: row.end_date,
            period_state: row.period_state,
            check_key: row.check_key,
            source_version: row.source_version,
            status: row.status,
            count_value: closeBlockerCount(row.count_value),
            checked_at: closeBlockerNumber(row.checked_at, "check time"),
            href: "/finance/close",
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: closeBlockersProviderId, metric: closeBlockersMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, closeBlockersProviderId, closeBlockersMetric, "open", [{ id: row.id, version: 1 }], closeBlockersFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: closeBlockersProviderId, metric: closeBlockersMetric, fields: closeBlockersFields, record, queryHash };
    }
    async function requireAvailableStockEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "commerce.read") || actor.departmentId || !catalog.some((item) => item.id === availableStockMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "inventory");
    }
    async function searchAvailableStockEvidence(actor: WorkContext, input: z.infer<typeof availableStockQuerySchema>) {
        await requireAvailableStockEvidence(actor);
        const cursor = availableStockCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchAvailableStockEvidence(input, actor, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: availableStockProviderId, metric: input.metric, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, availableStockProviderId, input.metric, "search", items.map((row) => ({ id: encodeAvailableStockRecordId(row), version: Number(row.stock_version) })), availableStockFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: availableStockProviderId,
            metric: input.metric,
            fields: availableStockFields,
            items: items.map((row) => ({
                id: encodeAvailableStockRecordId(row),
                label: `${row.sku} · ${row.product_name}`,
                summary: `${row.warehouse_name} · Available ${milliQuantity(row.available_milli)} ${row.unit}`,
                status: "available",
                version: Number(row.stock_version),
                href: "/commerce?view=stock",
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeAvailableStockCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readAvailableStockEvidence(actor: WorkContext, recordId: string) {
        await requireAvailableStockEvidence(actor);
        const ids = decodeAvailableStockRecordId(recordId);
        const { row } = await r.factsReadAvailableStockEvidence(actor, ids);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
        const record = {
            id: recordId, sku: row.sku, product_name: row.product_name, unit: row.unit, warehouse_name: row.warehouse_name,
            on_hand_milli: row.on_hand_milli, reserved_milli: row.reserved_milli, quarantined_milli: row.quarantined_milli,
            available_milli: row.available_milli, stock_version: Number(row.stock_version), href: "/commerce?view=stock",
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: availableStockProviderId, metric: availableStockMetric, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, availableStockProviderId, availableStockMetric, "open", [{ id: recordId, version: record.stock_version }], availableStockFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: availableStockProviderId, metric: availableStockMetric, fields: availableStockFields, record, queryHash };
    }
    async function requireInventoryShortageEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "commerce.read") || actor.departmentId || !catalog.some((item) => item.id === inventoryShortagesMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "inventory");
    }
    async function searchInventoryShortageEvidence(actor: WorkContext, input: z.infer<typeof inventoryShortagesQuerySchema>) {
        await requireInventoryShortageEvidence(actor);
        const cursor = availableStockCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchInventoryShortageEvidence(input, actor, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: inventoryShortagesProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, inventoryShortagesProviderId, input.metric, "search", items.map((row) => ({ id: encodeAvailableStockRecordId(row), version: Number(row.policy_version) })), inventoryShortagesFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: inventoryShortagesProviderId,
            metric: input.metric,
            fields: inventoryShortagesFields,
            items: items.map((row) => ({
                id: encodeAvailableStockRecordId(row),
                label: `${row.sku} · ${row.product_name}`,
                summary: `${row.warehouse_name} · Available ${milliQuantity(row.available_milli)} ${row.unit}; approved point ${milliQuantity(row.approved_reorder_point_milli)} ${row.unit}`,
                status: "shortage",
                version: Number(row.policy_version),
                href: "/commerce?view=stock",
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeAvailableStockCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readInventoryShortageEvidence(actor: WorkContext, recordId: string) {
        await requireInventoryShortageEvidence(actor);
        const ids = decodeAvailableStockRecordId(recordId);
        const { row } = await r.factsReadInventoryShortageEvidence(actor, ids);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
        const record = {
            id: recordId, sku: row.sku, product_name: row.product_name, unit: row.unit, warehouse_name: row.warehouse_name,
            on_hand_milli: row.on_hand_milli, reserved_milli: row.reserved_milli, quarantined_milli: row.quarantined_milli,
            available_milli: row.available_milli, approved_reorder_point_milli: row.approved_reorder_point_milli,
            policy_version: Number(row.policy_version), stock_version: Number(row.stock_version), href: "/commerce?view=stock",
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: inventoryShortagesProviderId, metric: inventoryShortagesMetric, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, inventoryShortagesProviderId, inventoryShortagesMetric, "open", [{ id: recordId, version: record.policy_version }], inventoryShortagesFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: inventoryShortagesProviderId, metric: inventoryShortagesMetric, fields: inventoryShortagesFields, record, queryHash };
    }
    async function requireStockedLocationEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "commerce.read") || actor.branchId || actor.departmentId || !catalog.some((item) => item.id === stockedLocationsMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "inventory");
    }
    async function searchStockedLocationEvidence(actor: WorkContext, input: z.infer<typeof stockedLocationsQuerySchema>) {
        await requireStockedLocationEvidence(actor);
        const cursor = availableStockCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchStockedLocationEvidence(input, actor, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: availableStockProviderId, metric: input.metric, scope: [actor.companyId, actor.assignmentId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, availableStockProviderId, input.metric, "search", items.map((row) => ({ id: encodeAvailableStockRecordId(row), version: Number(row.stock_version) })), availableStockFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: availableStockProviderId,
            metric: input.metric,
            fields: availableStockFields,
            items: items.map((row) => ({
                id: encodeAvailableStockRecordId(row),
                label: `${row.sku} · ${row.product_name}`,
                summary: `${row.warehouse_name} · On hand ${milliQuantity(row.on_hand_milli)} ${row.unit}`,
                status: "stocked",
                version: Number(row.stock_version),
                href: "/commerce?view=stock",
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeAvailableStockCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readStockedLocationEvidence(actor: WorkContext, recordId: string) {
        await requireStockedLocationEvidence(actor);
        const ids = decodeAvailableStockRecordId(recordId);
        const { row } = await r.factsReadStockedLocationEvidence(actor, ids);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
        const record = {
            id: recordId, sku: row.sku, product_name: row.product_name, unit: row.unit, warehouse_name: row.warehouse_name,
            on_hand_milli: row.on_hand_milli, reserved_milli: row.reserved_milli, quarantined_milli: row.quarantined_milli,
            available_milli: row.available_milli, stock_version: Number(row.stock_version), href: "/commerce?view=stock",
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: availableStockProviderId, metric: stockedLocationsMetric, scope: [actor.companyId, actor.assignmentId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, availableStockProviderId, stockedLocationsMetric, "open", [{ id: recordId, version: record.stock_version }], availableStockFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: availableStockProviderId, metric: stockedLocationsMetric, fields: availableStockFields, record, queryHash };
    }
    async function requireInventoryMovementEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "commerce.read") || actor.departmentId || !catalog.some((item) => item.id === inventoryMovementsMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "inventory");
    }
    async function searchInventoryMovementEvidence(actor: WorkContext, input: z.infer<typeof inventoryMovementsQuerySchema>) {
        await requireInventoryMovementEvidence(actor);
        const cursor = inventoryMovementCursorValue(input.cursor);
        const { rows } = await r.
            factsSearchInventoryMovementEvidence(input, actor, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: inventoryMovementsProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        // Ledger movement rows are append-only events and have no mutable version column; revision 1 identifies their sole event version.
        await audit(actor, inventoryMovementsProviderId, input.metric, "search", items.map((row) => ({ id: encodeInventoryMovementRecordId(row), version: 1 })), inventoryMovementFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: inventoryMovementsProviderId,
            metric: input.metric,
            fields: inventoryMovementFields,
            items: items.map((row) => ({
                id: encodeInventoryMovementRecordId(row),
                label: `${row.sku} · ${row.product_name}`,
                summary: `${row.movement_date} · ${row.source_type.replaceAll("_", " ")} · ${milliQuantity(row.quantity_milli)} ${row.unit} · ${row.warehouse_name}`,
                status: row.source_type,
                href: inventoryMovementHref(row),
            })),
            hasMore,
            nextCursor: hasMore && items.length ? encodeInventoryMovementCursor(items.at(-1)!) : null,
            queryHash,
        };
    }
    async function readInventoryMovementEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireInventoryMovementEvidence(actor);
        const ids = decodeInventoryMovementRecordId(recordId);
        const { row } = await r.factsReadInventoryMovementEvidence(actor, filters, ids);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = {
            id: recordId, movement_date: row.movement_date, source_type: row.source_type, source_id: row.source_id, source_number: row.source_number,
            sku: row.sku, product_name: row.product_name, unit: row.unit, warehouse_name: row.warehouse_name, quantity_milli: row.quantity_milli,
            href: inventoryMovementHref(row),
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: inventoryMovementsProviderId, metric: inventoryMovementsMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, inventoryMovementsProviderId, inventoryMovementsMetric, "open", [{ id: recordId, version: 1 }], inventoryMovementFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: inventoryMovementsProviderId, metric: inventoryMovementsMetric, fields: inventoryMovementFields, record, queryHash };
    }
    async function requireMaintenanceEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === maintenanceDowntimeMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "maintenance");
    }
    async function searchMaintenanceDowntimeEvidence(actor: WorkContext, input: z.infer<typeof maintenanceQuerySchema>) {
        await requireMaintenanceEvidence(actor);
        const cursor = maintenanceCursorValue(input.cursor), rows = await ports.maintenanceDowntimeEvidenceRows(actor, {
            from: Date.parse(`${input.filters.from}T00:00:00.000Z`), to: Date.parse(input.filters.to) + 86400000,
            query: input.query, limit: input.limit, cursor: cursor ?? undefined,
        });
        const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: maintenanceProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
        await ports.currentAuthority(actor);
        await audit(actor, maintenanceProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: maintenanceEvidenceNumber(row.version, "version") })), maintenanceSearchFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: maintenanceProviderId,
            metric: input.metric,
            fields: maintenanceSearchFields,
            items: items.map((row) => ({
                id: row.id, title: row.title, status: row.status, asset_code: row.asset_code, asset_name: row.asset_name,
                downtime_started_at: maintenanceEvidenceNumber(row.downtime_started_at, "downtime start"),
                downtime_ended_at: maintenanceEvidenceNumber(row.downtime_ended_at, "downtime end"),
                version: maintenanceEvidenceNumber(row.version, "version"), href: `/maintenance/${encodeURIComponent(row.id)}`,
                label: row.title, summary: `${row.asset_code} · ${row.asset_name} · ${row.status}`,
            })),
            hasMore, nextCursor: hasMore && items.length ? encodeMaintenanceCursor(items.at(-1)!) : null, queryHash,
        };
    }
    async function readMaintenanceDowntimeEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireMaintenanceEvidence(actor);
        const [row] = await ports.maintenanceDowntimeEvidenceRows(actor, {
            from: Date.parse(`${filters.from}T00:00:00.000Z`), to: Date.parse(filters.to) + 86400000,
            query: "", limit: 1, recordId,
        });
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const startedAt = maintenanceEvidenceNumber(row.downtime_started_at, "downtime start"), endedAt = maintenanceEvidenceNumber(row.downtime_ended_at, "downtime end");
        const record = {
            id: row.id, title: row.title, status: row.status, asset_code: row.asset_code, asset_name: row.asset_name,
            downtime_started_at: startedAt, downtime_ended_at: endedAt, duration_ms: String(BigInt(endedAt) - BigInt(startedAt)),
            version: maintenanceEvidenceNumber(row.version, "version"), href: `/maintenance/${encodeURIComponent(row.id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: maintenanceProviderId, metric: maintenanceDowntimeMetric, filters, recordId }));
        await ports.currentAuthority(actor);
        await audit(actor, maintenanceProviderId, maintenanceDowntimeMetric, "open", [{ id: record.id, version: record.version }], maintenanceDetailFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: maintenanceProviderId, metric: maintenanceDowntimeMetric, fields: maintenanceDetailFields, record, queryHash };
    }
    async function requireProjectMilestoneEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "projects")
            || !catalog.some((item) => item.id === projectMilestoneMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "projects");
    }
    async function searchProjectMilestoneEvidence(actor: WorkContext, raw: unknown) {
        const input = projectMilestoneQuerySchema.parse(raw);
        await requireProjectMilestoneEvidence(actor);
        const referenceAt = ports.clock.now();
        const cursor = projectMilestoneCursor(input.cursor);
        const { rows } = await r.
            factsSearchProjectMilestoneEvidence(actor, input, referenceAt, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const fields = projectMilestoneFields;
        const queryHash = await ports.digest(JSON.stringify({ providerId: projectMilestoneProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], referenceAt }));
        await ports.currentAuthority(actor);
        await audit(actor, projectMilestoneProviderId, projectMilestoneMetric, "search", items.map((row) => ({ id: row.id, version: row.version })), fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: projectMilestoneProviderId, metric: projectMilestoneMetric, fields, items: items.map((row) => ({
                id: row.id, label: row.title, summary: `${row.project_title} · Due ${new Date(row.due_at).toISOString().slice(0, 10)} · ${row.owner_name}`,
                status: row.state, version: row.version, href: projectMilestoneHref(row.project_id),
            })), hasMore, nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ dueAt: items.at(-1)!.due_at, id: items.at(-1)!.id })).toString("base64url") : null, queryHash };
    }
    async function readProjectMilestoneEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireProjectMilestoneEvidence(actor);
        const id = z.string().min(1).max(200).parse(recordId);
        const referenceAt = ports.clock.now();
        const { row } = await r.factsReadProjectMilestoneEvidence(actor, filters, referenceAt, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
        const record = { id: row.id, project_id: row.project_id, project_title: row.project_title, title: row.title, due_at: row.due_at, owner_user_id: row.owner_user_id, owner_name: row.owner_name, state: row.state, version: row.version, href: projectMilestoneHref(row.project_id) };
        const queryHash = await ports.digest(JSON.stringify({ providerId: projectMilestoneProviderId, metric: projectMilestoneMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], recordId, referenceAt }));
        await ports.currentAuthority(actor);
        await audit(actor, projectMilestoneProviderId, projectMilestoneMetric, "open", [{ id: row.id, version: row.version }], projectMilestoneFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: projectMilestoneProviderId, metric: projectMilestoneMetric, fields: projectMilestoneFields, record, queryHash };
    }
    async function requireProjectDependencyEvidence(actor: WorkContext) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "projects")
            || !catalog.some((item) => item.id === projectDependencyMetric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "projects");
    }
    async function searchProjectDependencyEvidence(actor: WorkContext, raw: unknown) {
        const input = projectDependencyQuerySchema.parse(raw);
        await requireProjectDependencyEvidence(actor);
        const cursor = projectMilestoneCursor(input.cursor);
        const { rows } = await r.
            factsSearchProjectDependencyEvidence(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const fields = projectDependencyFields;
        const queryHash = await ports.digest(JSON.stringify({ providerId: projectDependencyProviderId, metric: projectDependencyMetric,
            filters: input.filters, query: input.query, cursor: input.cursor ?? null,
            scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId],
            rows: items.map((row) => ({ id: row.id, version: row.version, blockers: projectDependencyBlockers(row.blockers_json) })) }));
        await ports.currentAuthority(actor);
        await audit(actor, projectDependencyProviderId, projectDependencyMetric, "search", items.flatMap((row) => [
            { id: row.id, version: row.version }, ...projectDependencyBlockers(row.blockers_json).map((blocker) => ({ id: blocker.id, version: blocker.version })),
        ]), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: projectDependencyProviderId, metric: projectDependencyMetric, fields,
            items: items.map((row) => {
                const blockers = projectDependencyBlockers(row.blockers_json);
                return { id: row.id, label: row.title, summary: `${row.project_title} · ${blockers.length} incomplete prerequisite${blockers.length === 1 ? "" : "s"}`,
                    status: row.state, version: row.version, href: canonicalUiHref(`/holdings/${encodeURIComponent(actor.holdingSlug)}/projects/${encodeURIComponent(row.project_id)}`) };
            }),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ dueAt: items.at(-1)!.due_at, id: items.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readProjectDependencyEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        await requireProjectDependencyEvidence(actor);
        const id = z.string().min(1).max(200).parse(recordId);
        const { row } = await r.factsReadProjectDependencyEvidence(actor, filters, id);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
        const blockers = projectDependencyBlockers(row.blockers_json);
        const record = { id: row.id, project_id: row.project_id, project_title: row.project_title, title: row.title,
            due_at: row.due_at, state: row.state, version: row.version, prerequisites: blockers,
            href: canonicalUiHref(`/holdings/${encodeURIComponent(actor.holdingSlug)}/projects/${encodeURIComponent(row.project_id)}`) };
        const queryHash = await ports.digest(JSON.stringify({ providerId: projectDependencyProviderId, metric: projectDependencyMetric,
            filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId],
            record: { id: row.id, version: row.version, blockers } }));
        await ports.currentAuthority(actor);
        await audit(actor, projectDependencyProviderId, projectDependencyMetric, "open", [
            { id: row.id, version: row.version }, ...blockers.map((blocker) => ({ id: blocker.id, version: blocker.version })),
        ], projectDependencyFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: projectDependencyProviderId, metric: projectDependencyMetric, fields: projectDependencyFields, record, queryHash };
    }
    async function requireProjectActualEvidence(actor: WorkContext, metric: ProjectActualMetric) {
        await ports.currentAuthority(actor);
        const permission = metric === "project-posted-cost" ? "finance.read" : "records.read";
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, permission) || !hasModuleAccess(actor.modules, "projects")
            || !catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "projects");
    }
    async function searchProjectActualEvidence(actor: WorkContext, raw: unknown) {
        const input = projectActualQuerySchema.parse(raw);
        const metric = input.metric;
        await requireProjectActualEvidence(actor, metric);
        const cursor = projectActualCursor(input.cursor);
        const fields = metric === "project-actual-hours" ? projectActualHoursFields : projectPostedCostFields;
        const { rows } = await r.
            factsSearchProjectActualEvidence(metric, input, actor, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: projectActualProviderId, metric, filters: input.filters,
            query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId] }));
        await ports.currentAuthority(actor);
        await audit(actor, projectActualProviderId, metric, "search", items.map((row) => ({ id: row.id, version: row.version })), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: projectActualProviderId, metric, fields,
            items: items.map((row) => metric === "project-actual-hours" ? {
                id: row.id, label: `${row.work_date} · ${row.minutes} minutes`, summary: `${row.project_title} · Approved tracked time`,
                status: row.state, version: row.version, href: projectMilestoneHref(row.project_id),
            } : {
                id: row.id, label: String(row.journal_number), amount: row.amount_minor, currency: row.currency,
                summary: `${row.project_title} · ${row.entry_date} · ${row.account_code} ${row.account_name}`,
                status: row.state, version: row.version,
                href: canonicalUiHref(`/holdings/${encodeURIComponent(actor.holdingSlug)}/finance/journals/${encodeURIComponent(row.journal_id!)}`),
            }),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ date: items.at(-1)!.sort_date, id: items.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readProjectActualEvidence(actor: WorkContext, metric: ProjectActualMetric, recordId: string, filters: Filters) {
        await requireProjectActualEvidence(actor, metric);
        const id = z.string().min(1).max(200).parse(recordId);
        const fields = metric === "project-actual-hours" ? projectActualHoursFields : projectPostedCostFields;
        const { row } = await r.factsReadProjectActualEvidence(metric, actor, id, filters);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const record = metric === "project-actual-hours" ? {
            id: row.id, project_id: row.project_id, project_title: row.project_title, work_date: row.work_date,
            minutes: row.minutes, user_id: row.user_id, state: row.state, version: row.version,
            href: projectMilestoneHref(row.project_id),
        } : {
            id: row.id, project_id: row.project_id, project_title: row.project_title, journal_id: row.journal_id,
            journal_number: row.journal_number, entry_date: row.entry_date, currency: row.currency,
            account_code: row.account_code, account_name: row.account_name, amount_minor: row.amount_minor,
            state: row.state, version: row.version,
            href: canonicalUiHref(`/holdings/${encodeURIComponent(actor.holdingSlug)}/finance/journals/${encodeURIComponent(row.journal_id!)}`),
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: projectActualProviderId, metric, filters,
            scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], recordId: id }));
        await ports.currentAuthority(actor);
        await audit(actor, projectActualProviderId, metric, "open", [{ id: row.id, version: row.version }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: projectActualProviderId, metric, fields, record, queryHash };
    }
    async function requireOperationsEvidence(actor: WorkContext, metric: OperationsMetricId) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "records.read") || !catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "operations");
    }
    async function searchOperationsEvidence(actor: WorkContext, raw: unknown) {
        const input = operationsQuerySchema.parse(raw);
        const metric = input.metric;
        await requireOperationsEvidence(actor, metric);
        const referenceAt = ports.clock.now();
        const cursor = operationsEvidenceCursor(input.cursor, metric);
        const { rows } = await r.
            factsSearchOperationsEvidence(actor, referenceAt, metric, input, cursor);
        const hasMore = rows.length > input.limit;
        const items = rows.slice(0, input.limit);
        const fields = operationsEvidenceFields(metric);
        const queryHash = await ports.digest(JSON.stringify({ providerId: operationsProviderId, metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], referenceAt }));
        await ports.currentAuthority(actor);
        await audit(actor, operationsProviderId, metric, "search", items.map((row) => ({ id: row.id, version: row.version })), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: operationsProviderId,
            metric,
            fields,
            items: items.map((row) => ({
                id: operationsRecordId(metric, row.id),
                label: row.title || "Operational task",
                summary: metric === "exception-resolution-time"
                    ? `Completed ${new Date(row.updated_at).toISOString().slice(0, 10)} · ${row.exception_duration_ms} ms past due`
                    : `${row.state} · ${row.due_at === null ? "No due date" : `due ${new Date(row.due_at).toISOString().slice(0, 10)}`}`,
                status: row.state,
                href: operationsTaskHref(row.id),
            })),
            hasMore,
            nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ metric, sortAt: items.at(-1)!.sort_at, id: items.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readOperationsEvidence(actor: WorkContext, metric: OperationsMetricId, recordId: string, filters: Filters) {
        await requireOperationsEvidence(actor, metric);
        const taskId = decodeOperationsRecordId(recordId, metric);
        const referenceAt = ports.clock.now();
        const { row } = await r.factsReadOperationsEvidence(actor, referenceAt, metric, filters, taskId);
        if (!row)
            throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
        const fields = operationsEvidenceFields(metric);
        const record = {
            id: row.id, title: row.title, state: row.state, due_at: row.due_at, created_at: row.created_at,
            updated_at: row.updated_at, version: row.version, branch_id: row.branch_id, department_id: row.department_id,
            ...(metric === "exception-resolution-time" ? { exception_duration_ms: row.exception_duration_ms } : {}),
            ...(metric === "operations-escalations" ? { alert_created_at: row.alert_created_at } : {}),
            href: operationsTaskHref(row.id),
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: operationsProviderId, metric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], recordId, referenceAt }));
        await ports.currentAuthority(actor);
        await audit(actor, operationsProviderId, metric, "open", [{ id: row.id, version: row.version }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: operationsProviderId, metric, fields, record, queryHash };
    }
    async function searchExecutiveEvidence(actor: WorkContext, raw: unknown) {
        const common = z.object({ metric: z.enum([...financeMetrics, payrollSummaryMetric, approvedLeaveMetric, authorizedOvertimeMetric, hotelOccupancyMetric, ...hotelRoomRevenueMetrics, hotelGuestAccountBalanceMetric, hotelNightAuditBlockersMetric, "invoice-match-exceptions", ...salesMetrics, salesFulfillmentMetric, fieldVisitMetric, manufacturingCostMetric, customerActivityMetric, socialActivityMetric, ...qualityMetrics, ...legalMetrics, closeBlockersMetric, identityChangeMetric, organizationNodeMetric, pendingStructureMetric, activeAssignmentsMetric, pendingAccessReviewsMetric, activeSubscriptionsMetric, overdueInvoicesMetric, maintenanceDowntimeMetric, duplicateReviewMetric, supportTicketMetrics[0], supportTicketMetrics[1], supportIncidentMetric, stockedLocationsMetric, availableStockMetric, inventoryShortagesMetric, inventoryMovementsMetric, projectMilestoneMetric, projectDependencyMetric, ...projectActualMetrics, ...operationsMetrics, ...agentEvidenceMetrics, ...crmEvidenceMetrics, ...brandEvidenceMetrics, ...learningEvidenceMetrics, ...talentEvidenceMetrics, ...posEvidenceMetrics, ...marketingEvidenceMetrics, ...planningEvidenceMetrics, ...benefitEvidenceMetrics]), ...commonFilters }).strict().parse(raw);
        if (crmEvidenceMetrics.includes(common.metric as typeof crmEvidenceMetrics[number]))
            return ports.searchCrmEvidence(actor, common);
        if (agentEvidenceMetrics.includes(common.metric as typeof agentEvidenceMetrics[number]))
            return ports.searchAgentEvidence(actor, common);
        if (brandEvidenceMetrics.includes(common.metric as typeof brandEvidenceMetrics[number]))
            return ports.searchBrandEvidence(actor, common);
        if (learningEvidenceMetrics.includes(common.metric as typeof learningEvidenceMetrics[number]))
            return ports.searchLearningEvidence(actor, common);
        if (talentEvidenceMetrics.includes(common.metric as typeof talentEvidenceMetrics[number]))
            return ports.searchTalentEvidence(actor, common);
        if (posEvidenceMetrics.includes(common.metric as typeof posEvidenceMetrics[number]))
            return ports.searchPosEvidence(actor, common);
        if (marketingEvidenceMetrics.includes(common.metric as typeof marketingEvidenceMetrics[number]))
            return ports.searchMarketingEvidence(actor, common);
        if (benefitEvidenceMetrics.includes(common.metric as typeof benefitEvidenceMetrics[number]))
            return ports.searchBenefitEvidence(actor, common);
        if (planningEvidenceMetrics.includes(common.metric as typeof planningEvidenceMetrics[number]))
            return ports.searchPlanningEvidence(actor, common);
        if (common.metric === projectMilestoneMetric)
            return searchProjectMilestoneEvidence(actor, projectMilestoneQuerySchema.parse(common));
        if (common.metric === projectDependencyMetric)
            return searchProjectDependencyEvidence(actor, projectDependencyQuerySchema.parse(common));
        if (projectActualMetrics.includes(common.metric as ProjectActualMetric))
            return searchProjectActualEvidence(actor, projectActualQuerySchema.parse(common));
        if (common.metric === payrollSummaryMetric)
            return searchPayrollSummaryEvidence(actor, payrollSummaryQuerySchema.parse(common));
        if (common.metric === approvedLeaveMetric || common.metric === authorizedOvertimeMetric)
            return searchPeopleLeaveOvertimeEvidence(actor, peopleLeaveOvertimeQuerySchema.parse(common));
        if (common.metric === hotelOccupancyMetric || hotelRoomRevenueMetrics.includes(common.metric as HotelRoomRevenueMetric))
            return searchHotelOccupancyEvidence(actor, hotelOccupancyQuerySchema.parse(common));
        if (common.metric === hotelGuestAccountBalanceMetric)
            return searchHotelGuestAccountBalanceEvidence(actor, hotelGuestAccountBalanceQuerySchema.parse(common));
        if (common.metric === hotelNightAuditBlockersMetric)
            return searchHotelNightAuditBlockerEvidence(actor, hotelNightAuditBlockersQuerySchema.parse(common));
        if (operationsMetrics.includes(common.metric as OperationsMetricId))
            return searchOperationsEvidence(actor, operationsQuerySchema.parse(common));
        if (legalMetrics.includes(common.metric as LegalMetricId))
            return searchLegalEvidence(actor, legalQuerySchema.parse(common));
        if (common.metric === closeBlockersMetric)
            return searchCloseBlockerEvidence(actor, closeBlockersQuerySchema.parse(common));
        if (common.metric === identityChangeMetric)
            return searchIdentityChangeEvidence(actor, identityChangeQuerySchema.parse(common));
        if (common.metric === organizationNodeMetric)
            return searchOrganizationNodeEvidence(actor, organizationNodeQuerySchema.parse(common));
        if (common.metric === pendingStructureMetric)
            return searchPendingStructureEvidence(actor, pendingStructureQuerySchema.parse(common));
        if (common.metric === activeAssignmentsMetric)
            return searchActiveAssignmentEvidence(actor, activeAssignmentsQuerySchema.parse(common));
        if (common.metric === pendingAccessReviewsMetric)
            return searchPendingAccessReviewEvidence(actor, pendingAccessReviewsQuerySchema.parse(common));
        if (common.metric === activeSubscriptionsMetric)
            return searchCommercialSubscriptions(actor, activeSubscriptionsQuerySchema.parse(common));
        if (common.metric === overdueInvoicesMetric)
            return searchOverdueInvoiceEvidence(actor, overdueInvoicesQuerySchema.parse(common));
        if (common.metric === duplicateReviewMetric)
            return searchDuplicateReviewEvidence(actor, duplicateReviewQuerySchema.parse(common));
        if (common.metric === maintenanceDowntimeMetric)
            return searchMaintenanceDowntimeEvidence(actor, maintenanceQuerySchema.parse(common));
        if (supportTicketMetrics.includes(common.metric as SupportTicketMetricId))
            return searchSupportTicketEvidence(actor, supportTicketQuerySchema.parse(common));
        if (common.metric === supportIncidentMetric)
            return searchSupportIncidentEvidence(actor, supportIncidentQuerySchema.parse(common));
        if (common.metric === stockedLocationsMetric)
            return searchStockedLocationEvidence(actor, stockedLocationsQuerySchema.parse(common));
        if (common.metric === availableStockMetric)
            return searchAvailableStockEvidence(actor, availableStockQuerySchema.parse(common));
        if (common.metric === inventoryShortagesMetric)
            return searchInventoryShortageEvidence(actor, inventoryShortagesQuerySchema.parse(common));
        if (common.metric === inventoryMovementsMetric)
            return searchInventoryMovementEvidence(actor, inventoryMovementsQuerySchema.parse(common));
        if (common.metric === "invoice-match-exceptions")
            return searchSupplierBillEvidence(actor, procurementQuerySchema.parse(common));
        if (common.metric === salesFulfillmentMetric)
            return searchSalesFulfillmentEvidence(actor, salesFulfillmentQuerySchema.parse(common));
        if (common.metric === fieldVisitMetric)
            return searchFieldVisitEvidence(actor, fieldVisitQuerySchema.parse(common));
        if (common.metric === manufacturingCostMetric)
            return searchManufacturingCostEvidence(actor, manufacturingCostQuerySchema.parse(common));
        if (common.metric === customerActivityMetric)
            return searchCustomerActivityEvidence(actor, customerActivityQuerySchema.parse(common));
        if (common.metric === socialActivityMetric)
            return searchSocialActivityEvidence(actor, socialActivityQuerySchema.parse(common));
        if (salesMetrics.includes(common.metric as SalesMetricId))
            return searchSalesOrderEvidence(actor, salesQuerySchema.parse(common));
        if (qualityMetrics.includes(common.metric as typeof qualityMetrics[number]))
            return searchQualityInspectionEvidence(actor, qualityQuerySchema.parse(common));
        return searchFinanceEvidence(actor, querySchema.parse(common));
    }
    async function readExecutiveEvidence(actor: WorkContext, metric: MetricId, recordId: string, filters: Filters) {
        if (crmEvidenceMetrics.includes(metric as typeof crmEvidenceMetrics[number]))
            return ports.readCrmEvidence(actor, metric, recordId, filters);
        if (agentEvidenceMetrics.includes(metric as typeof agentEvidenceMetrics[number]))
            return ports.readAgentEvidence(actor, metric, recordId, filters);
        if (brandEvidenceMetrics.includes(metric as typeof brandEvidenceMetrics[number]))
            return ports.readBrandEvidence(actor, metric, recordId, filters);
        if (learningEvidenceMetrics.includes(metric as typeof learningEvidenceMetrics[number]))
            return ports.readLearningEvidence(actor, metric, recordId, filters);
        if (talentEvidenceMetrics.includes(metric as typeof talentEvidenceMetrics[number]))
            return ports.readTalentEvidence(actor, metric, recordId, filters);
        if (posEvidenceMetrics.includes(metric as typeof posEvidenceMetrics[number]))
            return ports.readPosEvidence(actor, metric, recordId, filters);
        if (marketingEvidenceMetrics.includes(metric as typeof marketingEvidenceMetrics[number]))
            return ports.readMarketingEvidence(actor, metric, recordId, filters);
        if (benefitEvidenceMetrics.includes(metric as typeof benefitEvidenceMetrics[number]))
            return ports.readBenefitEvidence(actor, metric, recordId, filters);
        if (planningEvidenceMetrics.includes(metric as typeof planningEvidenceMetrics[number]))
            return ports.readPlanningEvidence(actor, metric, recordId, filters);
        if (metric === projectMilestoneMetric)
            return readProjectMilestoneEvidence(actor, recordId, filters);
        if (metric === projectDependencyMetric)
            return readProjectDependencyEvidence(actor, recordId, filters);
        if (projectActualMetrics.includes(metric as ProjectActualMetric))
            return readProjectActualEvidence(actor, metric as ProjectActualMetric, recordId, filters);
        if (metric === payrollSummaryMetric)
            return readPayrollSummaryEvidence(actor, recordId, filters);
        if (metric === approvedLeaveMetric || metric === authorizedOvertimeMetric)
            return readPeopleLeaveOvertimeEvidence(actor, metric, recordId, filters);
        if (metric === hotelOccupancyMetric || hotelRoomRevenueMetrics.includes(metric as HotelRoomRevenueMetric))
            return readHotelOccupancyEvidence(actor, metric as typeof hotelOccupancyMetric | HotelRoomRevenueMetric, recordId, filters);
        if (metric === hotelGuestAccountBalanceMetric)
            return readHotelGuestAccountBalanceEvidence(actor, recordId, filters);
        if (metric === hotelNightAuditBlockersMetric)
            return readHotelNightAuditBlockerEvidence(actor, recordId, filters);
        if (operationsMetrics.includes(metric as OperationsMetricId))
            return readOperationsEvidence(actor, metric as OperationsMetricId, recordId, filters);
        if (legalMetrics.includes(metric as LegalMetricId))
            return readLegalEvidence(actor, metric as LegalMetricId, recordId, filters);
        if (metric === closeBlockersMetric)
            return readCloseBlockerEvidence(actor, recordId, filters);
        if (metric === identityChangeMetric)
            return readIdentityChangeEvidence(actor, recordId, filters);
        if (metric === organizationNodeMetric)
            return readOrganizationNodeEvidence(actor, recordId, filters);
        if (metric === pendingStructureMetric)
            return readPendingStructureEvidence(actor, recordId, filters);
        if (metric === activeAssignmentsMetric)
            return readActiveAssignmentEvidence(actor, recordId, filters);
        if (metric === pendingAccessReviewsMetric)
            return readPendingAccessReviewEvidence(actor, recordId, filters);
        if (metric === activeSubscriptionsMetric)
            return readCommercialSubscription(actor, recordId, filters);
        if (metric === overdueInvoicesMetric)
            return readOverdueInvoiceEvidence(actor, recordId, filters);
        if (metric === duplicateReviewMetric)
            return readDuplicateReviewEvidence(actor, recordId, filters);
        if (metric === maintenanceDowntimeMetric)
            return readMaintenanceDowntimeEvidence(actor, recordId, filters);
        if (supportTicketMetrics.includes(metric as SupportTicketMetricId))
            return readSupportTicketEvidence(actor, metric as SupportTicketMetricId, recordId, filters);
        if (metric === supportIncidentMetric)
            return readSupportIncidentEvidence(actor, recordId, filters);
        if (metric === stockedLocationsMetric)
            return readStockedLocationEvidence(actor, recordId);
        if (metric === availableStockMetric)
            return readAvailableStockEvidence(actor, recordId);
        if (metric === inventoryShortagesMetric)
            return readInventoryShortageEvidence(actor, recordId);
        if (metric === inventoryMovementsMetric)
            return readInventoryMovementEvidence(actor, recordId, filters);
        if (metric === "invoice-match-exceptions")
            return readSupplierBillEvidence(actor, recordId, filters);
        if (metric === salesFulfillmentMetric)
            return readSalesFulfillmentEvidence(actor, recordId, filters);
        if (metric === fieldVisitMetric)
            return readFieldVisitEvidence(actor, recordId, filters);
        if (metric === manufacturingCostMetric)
            return readManufacturingCostEvidence(actor, recordId, filters);
        if (metric === customerActivityMetric)
            return readCustomerActivityEvidence(actor, recordId, filters);
        if (metric === socialActivityMetric)
            return readSocialActivityEvidence(actor, recordId, filters);
        if (salesMetrics.includes(metric as SalesMetricId))
            return readSalesOrderEvidence(actor, metric as SalesMetricId, recordId, filters);
        if (metric === "inspection-pass-rate")
            return readQualityInspectionEvidence(actor, recordId, filters);
        if (metric === "quality-holds" || metric === "quality-releases")
            return readQualityHoldEvidence(actor, metric, recordId, filters);
        if (metric === "receivables" || metric === "payables" || metric === "invoiced-sales")
            return readFinanceEvidence(actor, metric, recordId, filters);
        throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
    }
    return { searchFinanceEvidence, readFinanceEvidence, searchExecutiveEvidence, readExecutiveEvidence };
}
