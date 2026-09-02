# Traceability — Requirement → Code → Spec

> Every row links a requirement surface to its contract, implementation, and business-plan spec. Docs = code.

## RPCs (≈552)

| RPC | Context | Contract | Implementation | Spec |
|---|---|---|---|---|

| RegisterAgent | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| AddAgentVersion | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| ApproveAgentVersion | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| SuspendAgent | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| RetireAgent | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| StartAssignment | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| DelegateAssignment | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| RetryAssignment | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| CancelAssignment | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| RegisterPool | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| ReplacePoolMembers | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| RoutePool | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| StartPrivateSession | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| AppendPrivateSessionMessage | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| PublishPrivateSessionMessage | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| SetPrivateSessionLegalHold | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| ExpirePrivateSession | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| RequestFieldSuggestion | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| StartFieldSuggestion | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| CompleteFieldSuggestion | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| ApplyFieldSuggestion | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| RejectFieldSuggestion | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| GetAgentDefinition | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| GetAssignment | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| GetPool | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| GetPrivateSession | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| GetRoutingDecision | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| GetInvocationEvidence | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| GetFieldSuggestion | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| ListFieldSuggestionsByRecord | agent | `crates/proto/proto/erpx/agent/service.proto` | `crates/contexts/agent/src/application.rs` | `Plan/02-capabilities/*/agent.md` |
| Create | assets | `crates/proto/proto/erpx/assets/service.proto` | `crates/contexts/assets/src/application.rs` | `Plan/02-capabilities/*/assets.md` |
| PlaceInService | assets | `crates/proto/proto/erpx/assets/service.proto` | `crates/contexts/assets/src/application.rs` | `Plan/02-capabilities/*/assets.md` |
| MarkDegraded | assets | `crates/proto/proto/erpx/assets/service.proto` | `crates/contexts/assets/src/application.rs` | `Plan/02-capabilities/*/assets.md` |
| BeginMaintenance | assets | `crates/proto/proto/erpx/assets/service.proto` | `crates/contexts/assets/src/application.rs` | `Plan/02-capabilities/*/assets.md` |
| ReturnToService | assets | `crates/proto/proto/erpx/assets/service.proto` | `crates/contexts/assets/src/application.rs` | `Plan/02-capabilities/*/assets.md` |
| Retire | assets | `crates/proto/proto/erpx/assets/service.proto` | `crates/contexts/assets/src/application.rs` | `Plan/02-capabilities/*/assets.md` |
| Get | assets | `crates/proto/proto/erpx/assets/service.proto` | `crates/contexts/assets/src/application.rs` | `Plan/02-capabilities/*/assets.md` |
| List | assets | `crates/proto/proto/erpx/assets/service.proto` | `crates/contexts/assets/src/application.rs` | `Plan/02-capabilities/*/assets.md` |
| PublishPlan | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| OpenEnrollment | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| AddDependent | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| ElectCoverage | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| SubmitProvider | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| AcknowledgeProvider | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| ReleasePayroll | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| GetEnrollment | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| GetRecord | benefits | `crates/proto/proto/erpx/benefits/service.proto` | `crates/contexts/benefits/src/application.rs` | `Plan/02-capabilities/*/benefits.md` |
| CreateDraft | brand | `crates/proto/proto/erpx/brand/service.proto` | `crates/contexts/brand/src/application.rs` | `Plan/02-capabilities/*/brand.md` |
| SaveDraft | brand | `crates/proto/proto/erpx/brand/service.proto` | `crates/contexts/brand/src/application.rs` | `Plan/02-capabilities/*/brand.md` |
| PublishDraft | brand | `crates/proto/proto/erpx/brand/service.proto` | `crates/contexts/brand/src/application.rs` | `Plan/02-capabilities/*/brand.md` |
| Rollback | brand | `crates/proto/proto/erpx/brand/service.proto` | `crates/contexts/brand/src/application.rs` | `Plan/02-capabilities/*/brand.md` |
| SetCategoryLocks | brand | `crates/proto/proto/erpx/brand/service.proto` | `crates/contexts/brand/src/application.rs` | `Plan/02-capabilities/*/brand.md` |
| GetDraft | brand | `crates/proto/proto/erpx/brand/service.proto` | `crates/contexts/brand/src/application.rs` | `Plan/02-capabilities/*/brand.md` |
| GetResolved | brand | `crates/proto/proto/erpx/brand/service.proto` | `crates/contexts/brand/src/application.rs` | `Plan/02-capabilities/*/brand.md` |
| ListHistory | brand | `crates/proto/proto/erpx/brand/service.proto` | `crates/contexts/brand/src/application.rs` | `Plan/02-capabilities/*/brand.md` |
| CreateThread | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| AddComment | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| EditComment | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| TombstoneComment | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| TransitionThread | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| SetReaction | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| SetFollowing | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| WithdrawMention | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| ModerateComment | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| ProposeActionCard | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| ConfirmActionCard | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| AddRecordRelation | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| RevokeRecordRelation | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| StartRecordThreadWithComment | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| PrepareAttachmentUpload | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| FinalizeAttachmentUpload | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| GetThread | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| SearchThreads | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| ListThreadsByRecord | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| ListMentionInbox | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| ListRecordRelations | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| GetThreadV2 | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| ListThreadsByRecordV2 | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| GetAttachmentDownload | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| AutocompleteTargets | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| Subscribe | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/*/collaboration.md` |
| ListFeed | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` + `crates/contexts/collaboration/src/feed.rs` (`FeedQueryService`) + `crates/adapters/collaboration-sqlx/src/feed.rs` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` + `Plan/03-cross-cutting/@-and-ai/collaboration.md#social-feed` |
| GetFeedItem | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` + `crates/contexts/collaboration/src/feed.rs` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` |
| SearchFeed | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` |
| ListMyDay | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` |
| ShareFeedItem | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` |
| CreateHashtagTopic | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` |
| FollowHashtag | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` |
| ListHashtagTopics | collaboration | `crates/proto/proto/erpx/collaboration/service.proto` | `crates/contexts/collaboration/src/application.rs` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` |
| AllocateCompanyEntitlement | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ActivateCompanyEntitlement | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ActivateBranchEntitlement | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| CreateCheckoutSession | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| CreateCustomerPortalSession | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| GrantGraceExtension | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| CreateCatalogPlan | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| PublishCatalogPlan | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| CreateSubscription | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ApplyBillingEvent | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| CreateEntitlement | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ApplyEntitlementChange | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| CheckEntitlement | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ListSaasCatalog | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| GetHoldingSubscription | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ListHoldingInvoices | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| GetCatalogPlan | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ListCatalogPlans | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| GetSubscription | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ListSubscriptions | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| GetEntitlement | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| ListEntitlements | commercial | `crates/proto/proto/erpx/commercial/service.proto` | `crates/contexts/commercial/src/application.rs` | `Plan/02-capabilities/*/commercial.md` |
| SaveCrmHandoff | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| SaveCrmOpportunity | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| RecordCrmAttribution | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| AcceptMarketingLead | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| CreateOpportunity | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| TransitionOpportunity | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| ListCrmReadRows | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| GetCrmLead | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| GetOpportunity | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| ListOpportunities | crm | `crates/proto/proto/erpx/crm/service.proto` | `crates/contexts/crm/src/application.rs` | `Plan/02-capabilities/*/crm.md` |
| AddCustomerNote | customer_360 | `crates/proto/proto/erpx/customer_360/service.proto` | `crates/contexts/customer_360/src/application.rs` | `Plan/02-capabilities/*/customer_360.md` |
| GetCustomer360 | customer_360 | `crates/proto/proto/erpx/customer_360/service.proto` | `crates/contexts/customer_360/src/application.rs` | `Plan/02-capabilities/*/customer_360.md` |
| GetCustomer360Detail | customer_360 | `crates/proto/proto/erpx/customer_360/service.proto` | `crates/contexts/customer_360/src/application.rs` | `Plan/02-capabilities/*/customer_360.md` |
| Create | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| UpdateIdentity | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| ChangeStatus | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| LinkAuthority | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| UnlinkAuthority | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| Merge | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| Get | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| List | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| GetCustomerAccessAudit | customer_authority | `crates/proto/proto/erpx/customer_authority/service.proto` | `crates/contexts/customer_authority/src/application.rs` | `Plan/02-capabilities/*/customer_authority.md` |
| CreateChannel | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ApproveChannel | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| TransitionChannel | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| CreateListing | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ApproveListing | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| PublishListing | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| WithdrawListing | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| CreateCart | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| PutCartLine | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| LockCart | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| StartCheckout | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ReconcileCheckoutRevalidation | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ReconcileCheckoutReservation | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ReconcileCheckoutPayment | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ReconcileCheckoutOrder | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| RequestReturn | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ReconcileReturnDisposition | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ReconcileReturnRefund | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ProposeFulfillment | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| TransitionFulfillment | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| GetChannel | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| GetListing | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| GetCart | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| GetCheckout | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| GetReturn | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| GetFulfillment | ecommerce | `crates/proto/proto/erpx/ecommerce/service.proto` | `crates/contexts/ecommerce/src/application.rs` | `Plan/02-capabilities/*/ecommerce.md` |
| ActivateAgreement | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| CreateOrder | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| ScheduleOrder | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| ReservePart | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| DispatchOrder | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| StartVisit | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| RecordException | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| IssuePart | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| CompleteVisit | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| ReleaseBilling | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| GetOrder | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| GetRecord | field_service | `crates/proto/proto/erpx/field_service/service.proto` | `crates/contexts/field_service/src/application.rs` | `Plan/02-capabilities/*/field_service.md` |
| CreateVendorInvoice | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| PostVendorInvoice | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| CreatePayment | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ReleasePayment | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetVendorInvoice | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetPayment | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| CreateChart | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| AddAccount | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ChangeAccountStatus | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| CreatePeriod | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| BeginPeriodClose | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ClosePeriod | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ReopenPeriod | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| CreateJournal | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| PostJournal | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ApplySubledgerSettlement | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetChart | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ListCharts | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetPeriod | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetJournal | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ListJournals | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetSubledger | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ListSubledger | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetTrialBalance | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ListBankTransactions | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetStatutoryTaxReport | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| RevalueSubledgers | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ImportChartCsv | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ImportOpeningTrialBalance | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| GetFinancialStatements | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| ListAging | finance | `crates/proto/proto/erpx/finance/service.proto` | `crates/contexts/finance/src/application.rs` | `Plan/02-capabilities/*/finance.md` |
| SaveDirectoryConnector | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| StartDirectoryImport | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| SaveDirectoryRoleMapping | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| PreflightDirectoryConnector | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| TransitionDirectoryConnectorLifecycle | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| LinkDirectoryPrincipal | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| UnlinkDirectoryPrincipal | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ReconcileDirectoryConnector | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ProposeDirectoryIdentityReplacement | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ApproveDirectoryIdentityReplacement | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| RejectDirectoryIdentityReplacement | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| RenewRole | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| RecoverFinalRole | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| CreatePrincipal | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ActivatePrincipal | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| SuspendPrincipal | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ReactivatePrincipal | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| DisablePrincipal | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| AssignRole | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| RevokeRole | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| DefineRole | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| SetRolePermissions | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ListDirectoryConnectors | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| GetDirectoryImportStatus | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ListDirectoryRoleMappings | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ListDirectoryIdentityLinks | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| GetDirectoryConnectorReadiness | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ListDirectoryConflicts | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ListExpiringRoles | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ListRoleReview | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| GetPrincipal | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ListPrincipals | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| GetRole | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| ListRoles | identity_access | `crates/proto/proto/erpx/identity_access/service.proto` | `crates/contexts/identity_access/src/application.rs` | `Plan/02-capabilities/*/identity_access.md` |
| Register | inventory | `crates/proto/proto/erpx/inventory/service.proto` | `crates/contexts/inventory/src/application.rs` | `Plan/02-capabilities/*/inventory.md` |
| Receive | inventory | `crates/proto/proto/erpx/inventory/service.proto` | `crates/contexts/inventory/src/application.rs` | `Plan/02-capabilities/*/inventory.md` |
| Issue | inventory | `crates/proto/proto/erpx/inventory/service.proto` | `crates/contexts/inventory/src/application.rs` | `Plan/02-capabilities/*/inventory.md` |
| Reserve | inventory | `crates/proto/proto/erpx/inventory/service.proto` | `crates/contexts/inventory/src/application.rs` | `Plan/02-capabilities/*/inventory.md` |
| ReleaseReservation | inventory | `crates/proto/proto/erpx/inventory/service.proto` | `crates/contexts/inventory/src/application.rs` | `Plan/02-capabilities/*/inventory.md` |
| Get | inventory | `crates/proto/proto/erpx/inventory/service.proto` | `crates/contexts/inventory/src/application.rs` | `Plan/02-capabilities/*/inventory.md` |
| List | inventory | `crates/proto/proto/erpx/inventory/service.proto` | `crates/contexts/inventory/src/application.rs` | `Plan/02-capabilities/*/inventory.md` |
| PublishCourse | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| RetireCourse | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| AssignCourse | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| RecordAttendance | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| RecordProviderResult | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| WaiveEnrollment | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| CompleteEnrollment | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| GetEnrollment | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| GetRecord | learning | `crates/proto/proto/erpx/learning/service.proto` | `crates/contexts/learning/src/application.rs` | `Plan/02-capabilities/*/learning.md` |
| CreateContract | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| AddContractParty | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| TransitionContract | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| CreateDocument | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| AddDocumentVersion | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| ArchiveDocument | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| RecordDocumentAccess | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| GetContract | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| ListContracts | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| GetDocument | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| ListDocuments | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| ListAccessHistory | legal | `crates/proto/proto/erpx/legal/service.proto` | `crates/contexts/legal/src/application.rs` | `Plan/02-capabilities/*/legal.md` |
| Open | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| Schedule | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| Start | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| Block | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| Resume | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| Complete | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| Cancel | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| RecordDowntime | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| Get | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| List | maintenance | `crates/proto/proto/erpx/maintenance/service.proto` | `crates/contexts/maintenance/src/application.rs` | `Plan/02-capabilities/*/maintenance.md` |
| PublishSpecification | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| CreateOrder | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| ReleaseOrder | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| ReserveMaterial | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| StartOrder | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| IssueMaterial | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| ReportLabor | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| ReportResult | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| ApplyQualityDisposition | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| RecordCost | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| CompleteOrder | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| CloseOrder | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| GetSpecification | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| GetOrder | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| ListOrders | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| GetGenealogy | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| GetWipCost | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| GetRecord | manufacturing | `crates/proto/proto/erpx/manufacturing/service.proto` | `crates/contexts/manufacturing/src/application.rs` | `Plan/02-capabilities/*/manufacturing.md` |
| SubmitMarketingForm | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| EraseMarketingProfile | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| IssueMarketingPreview | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| RevokeMarketingPreview | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| IssueMarketingDomainVerification | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| VerifyMarketingDomain | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| BindMarketingDomain | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| ConfigureMarketingProviderAccount | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| AcknowledgeMarketingProviderVerification | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| SetMarketingProviderAccountState | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| CreateLeadCandidate | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| CaptureEngagement | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| QualifyLeadCandidate | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| RejectLeadCandidate | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| GetMarketingDeliveryRead | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| ListMarketingContentReadRows | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| GetLeadCandidate | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| ListLeadCandidates | marketing | `crates/proto/proto/erpx/marketing/service.proto` | `crates/contexts/marketing/src/application.rs` | `Plan/02-capabilities/*/marketing.md` |
| ReassignException | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| AcknowledgeException | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| EscalateException | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| DecideApproval | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| ResolveException | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| CloseException | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| PublishWorkQueuePolicy | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| GetException | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| ListExceptionQueue | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| ListWorkQueuePolicies | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| GetSummary | operations | `crates/proto/proto/erpx/operations/service.proto` | `crates/contexts/operations/src/application.rs` | `Plan/02-capabilities/*/operations.md` |
| CreateNode | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| RenameNode | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| ChangeNodeCode | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| ReparentNode | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| SuspendNode | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| ActivateNode | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| ArchiveNode | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| GetNode | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| ListNodes | organization | `crates/proto/proto/erpx/organization/service.proto` | `crates/contexts/organization/src/application.rs` | `Plan/02-capabilities/*/organization.md` |
| HireEmployee | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| TransitionEmployee | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| CreateLeaveRequest | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| TransitionLeaveRequest | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| CreateExpenseClaim | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| TransitionExpenseClaim | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| CreatePayrollRun | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| TransitionPayrollRun | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| GetEmployee | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| ListEmployees | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| GetLeaveRequest | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| ListLeaveRequests | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| GetExpenseClaim | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| ListExpenseClaims | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| GetPayrollRun | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| ListPayrollRuns | people_payroll | `crates/proto/proto/erpx/people_payroll/service.proto` | `crates/contexts/people_payroll/src/application.rs` | `Plan/02-capabilities/*/people_payroll.md` |
| CreateScenario | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| FreezeScenario | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| RunScenario | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| OverrideRecommendation | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| ApproveRecommendation | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| RejectRecommendation | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| ReleaseRecommendation | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| GetScenario | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| GetRecommendation | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| GetRun | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| ListRecommendations | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| ListShortages | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| ListCapacityOverloads | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| GetPlan | planning | `crates/proto/proto/erpx/planning/service.proto` | `crates/contexts/planning/src/application.rs` | `Plan/02-capabilities/*/planning.md` |
| RegisterHolding | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| TransitionHolding | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| RegisterConnector | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| TransitionConnector | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| IssueAdminContext | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| RevokeAdminContext | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ProvisionHoldingBranch | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| RetryHoldingBranchProvisioning | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| SuspendHoldingBranchRoute | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| BeginHoldingAdminContext | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| EndHoldingAdminContext | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| BeginHoldingProvisioning | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ProvisionHoldingResources | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ActivateHolding | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| SuspendHolding | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| EnterHoldingRecovery | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ScheduleHoldingBackup | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ScheduleClusterBaseBackup | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| StartClusterRestoreRehearsal | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| StartProjectionRebuild | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ActivateProjectionRebuild | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ReplayDeadLetterOutbox | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| QueueMigrationWork | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| BeginMigrationRun | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| AdvanceMigrationPhase | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| SaveHoldingInfrastructureProfile | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| SaveBusinessUnitConfiguration | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| IssueDataPlaneEnrollment | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ProposeDesiredConfiguration | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| DecideDesiredConfiguration | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| RequestRootAccess | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| DecideRootAccess | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| RevokeRootAccess | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| SaveCorporateIdentityDraft | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| PublishCorporateIdentityDraft | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| RollbackCorporateIdentity | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetHolding | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListHoldings | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetConnector | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListConnectors | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetAdminContext | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetHoldingBranchProvisioning | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListHoldingBranchProvisionings | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListOperationalHoldings | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetOperationalHolding | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ResolveActiveHoldingRoute | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListHoldingControlHistory | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetHoldingAdminContext | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| DiscoverHoldingOrganizationUnits | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListHoldingBackupJobs | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListClusterRecoveryPoints | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetClusterRestoreRehearsal | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListProjectionStatuses | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListPlatformAudit | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListPlatformIntegrations | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetPlatformSettings | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListMigrationStatuses | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetHoldingInfrastructure | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetBusinessUnitConfiguration | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListRootAccessRequests | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetResolvedCorporateIdentity | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetCorporateIdentityDraft | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| ListCorporateIdentityHistory | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| GetNavigationManifest | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| Subscribe | platform_administration | `crates/proto/proto/erpx/platform_administration/service.proto` | `crates/contexts/platform_administration/src/application.rs` | `Plan/02-capabilities/*/platform_administration.md` |
| CreateSale | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| OpenSale | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| HoldSale | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| ResumeSale | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| RecordPayment | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| CompleteSale | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| VoidSale | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| RequestRefund | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| ApproveRefund | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| RejectRefund | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| PostRefund | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| CancelRefund | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| GetSale | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| ListSales | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| GetRefund | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| ListRefunds | pos | `crates/proto/proto/erpx/pos/service.proto` | `crates/contexts/pos/src/application.rs` | `Plan/02-capabilities/*/pos.md` |
| Create | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| Release | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| Cancel | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| CreateRequisition | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| SubmitRequisition | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| ApproveRequisition | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| RejectRequisition | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| CancelRequisition | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| CreateGoodsReceipt | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| AcceptGoodsReceipt | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| RejectGoodsReceipt | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| CancelGoodsReceipt | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| Get | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| List | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| GetRequisition | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| GetGoodsReceipt | procurement | `crates/proto/proto/erpx/procurement/service.proto` | `crates/contexts/procurement/src/application.rs` | `Plan/02-capabilities/*/procurement.md` |
| CreateProject | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| TransitionProject | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| AddNode | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| MoveNode | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| TransitionNode | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| AttachNodeReference | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| AddNodeAttachment | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| RecordNodeTime | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| RecordNodeCost | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| ProposeDependency | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| DecideDependency | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| CancelDependency | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| RevokeDependency | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| ProposeHandoff | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| ApproveHandoff | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| MarkHandoffReady | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| MarkHandoffUncertain | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| CancelHandoff | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| ReconcileHandoff | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| GetProject | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| GetProjectGraph | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| ListProjects | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| GetRecord | projects | `crates/proto/proto/erpx/projects/service.proto` | `crates/contexts/projects/src/application.rs` | `Plan/02-capabilities/*/projects.md` |
| Create | quality | `crates/proto/proto/erpx/quality/service.proto` | `crates/contexts/quality/src/application.rs` | `Plan/02-capabilities/*/quality.md` |
| Start | quality | `crates/proto/proto/erpx/quality/service.proto` | `crates/contexts/quality/src/application.rs` | `Plan/02-capabilities/*/quality.md` |
| RecordResult | quality | `crates/proto/proto/erpx/quality/service.proto` | `crates/contexts/quality/src/application.rs` | `Plan/02-capabilities/*/quality.md` |
| Release | quality | `crates/proto/proto/erpx/quality/service.proto` | `crates/contexts/quality/src/application.rs` | `Plan/02-capabilities/*/quality.md` |
| Cancel | quality | `crates/proto/proto/erpx/quality/service.proto` | `crates/contexts/quality/src/application.rs` | `Plan/02-capabilities/*/quality.md` |
| Get | quality | `crates/proto/proto/erpx/quality/service.proto` | `crates/contexts/quality/src/application.rs` | `Plan/02-capabilities/*/quality.md` |
| List | quality | `crates/proto/proto/erpx/quality/service.proto` | `crates/contexts/quality/src/application.rs` | `Plan/02-capabilities/*/quality.md` |
| Resolve | reference | `crates/proto/proto/erpx/reference/service.proto` | `crates/contexts/reference/src/application.rs` | `Plan/02-capabilities/*/reference.md` |
| Search | reference | `crates/proto/proto/erpx/reference/service.proto` | `crates/contexts/reference/src/application.rs` | `Plan/02-capabilities/*/reference.md` |
| ListBacklinks | reference | `crates/proto/proto/erpx/reference/service.proto` | `crates/contexts/reference/src/application.rs` | `Plan/02-capabilities/*/reference.md` |
| CreateQuote | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| SendQuote | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| AcceptQuote | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| RejectQuote | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| ExpireQuote | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| CreateSalesOrder | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| ConfirmSalesOrder | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| MarkSalesOrderPartiallyFulfilled | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| MarkSalesOrderFulfilled | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| CancelSalesOrder | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| GetQuote | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| GetSalesOrder | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| ListSalesOrders | sales | `crates/proto/proto/erpx/sales/service.proto` | `crates/contexts/sales/src/application.rs` | `Plan/02-capabilities/*/sales.md` |
| OpenTicket | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| TransitionTicket | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| AddCustomerReply | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| AddInternalNote | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| RequestSession | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| ApproveSession | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| RevokeSession | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| IssueCapability | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| RevokeCapability | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| OpenIncident | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| TransitionIncident | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| AddIncidentTimeline | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| BroadcastIncident | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| GetCustomerTicket | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| ListCustomerTickets | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| GetInternalTicket | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| ListInternalTickets | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| GetIncident | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| ListIncidents | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| GetSession | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| GetCapability | support | `crates/proto/proto/erpx/support/service.proto` | `crates/contexts/support/src/application.rs` | `Plan/02-capabilities/*/support.md` |
| CreateRequisition | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| ApproveRequisition | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| TransitionRequisition | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| CreateCandidateProfile | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| RequestCandidateErasure | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| CompleteCandidateErasure | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| SetCandidateLegalHold | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| SubmitApplication | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| AddApplicationInterview | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| SelectApplication | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| WithdrawApplication | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| CreateOffer | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| ApproveOffer | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| PresentOffer | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| DecideOffer | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| RequestHireHandoff | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| ReconcileHireHandoff | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| CreateProposal | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| SubmitProposal | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| ApproveProposal | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| ReconcileProposal | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| GetRequisition | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| GetCandidateProfile | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| GetApplication | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| GetOffer | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| GetHireHandoff | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| GetProposal | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |
| GetRecord | talent | `crates/proto/proto/erpx/talent/service.proto` | `crates/contexts/talent/src/application.rs` | `Plan/02-capabilities/*/talent.md` |

## Reference routing — `@`-mentions

| Surface | Contract | Implementation | Spec |
|---|---|---|---|
| `@<context>/<id>` → `/reference/item/:id` | `crates/proto/proto/erpx/reference/service.proto` (`ReferenceQueryService`) + `crates/proto/proto/erpx/collaboration/service.proto` (Typed Mentions) | `crates/migration/migrations/0006_global_collaboration_typed_mentions.sql` + `apps/server/src/target_transport.rs` (`HoldingDataRouteScope`) | `Plan/01-vision-and-scope.md` + `CONTEXT.md` (Referenceable, Typed Mention) |
| Collaboration threads/comments/action cards | `CollaborationCommandService.CreateThread/AddComment/ProposeActionCard` | `crates/contexts/collaboration/src/*` + `crates/adapters/collaboration-sqlx` | `Plan/02-capabilities/13-ai-collaboration/collaboration.md` |
| Field values / AI suggestions / search (promise) | Same `ReferenceQueryService` | Collaboration path implemented; field/search expansion is declared follow-up in `01-vision-and-scope.md` | `Plan/01-vision-and-scope.md` |

## Console routes

| Route | Area | Contract | Type |
|---|---|---|---|

| `/public/marketing/form/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/public/marketing/preview/:id?:token` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/auth/sign-in?:return_to&:auth_error&:add_account` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/auth/accounts?:return_to` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/auth/security` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/auth/devices?:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/auth/recovery` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/settings/ai-provider` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/settings/billing` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/work?:view&:domain&:severity&:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/processes?:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/insights` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/modules` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/platform/holdings?:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/platform/holdings/:holding_id?:history_cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/platform/holding/:holding_id?:history_cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/platform/support` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/platform/support/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/platform/incidents` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/platform/incidents/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/principal?:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/principal/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/role?:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/role/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/grant?:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/grant/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/access-review?:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/access-review/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/break-glass-session?:cursor` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/identity/break-glass-session/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/reference/item/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/operations` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/finance` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/sales` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/procurement` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/inventory` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/asset` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/maintenance` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/quality` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/marketing` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/people` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/pos` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/legal` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/manufacturing` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/planning` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/project` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/commerce` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/talent` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/learning` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/benefits` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/field-service` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/ai` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/:workspace/:resource` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/:workspace/:resource/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/:module/:record/:id` | console | `apps/console/src/routes.rs` | `AppRoute` |
| `/:..segments` | console | `apps/console/src/routes.rs` | `AppRoute` |

## Data

| Surface | Contract |
|---|---|
| 7 migrations (105+ baseline tables) + outbox/projection checkpoint | `crates/migration/migrations/0001_baseline.sql` … `0007_ai_provider.sql` |
| ADRs (18) | `adr/ADR-*.md` |

## Verification

- `cargo xtask test-affected` checks owning package + consumers
- `cargo xtask verify` is the full gate (fmt → clippy → tests → SQLx disposable DBs → Dioxus build → live smoke)

## Deep `@` + AI — production addendum (2026-08-29)

- **Per-module deep specs (31):** `Plan/03-cross-cutting/@-and-ai/<context>.md` each trace to `crates/proto/proto/erpx/<ctx>/service.proto` + `crates/contexts/<ctx>/src/domain.rs` + `crates/adapters/<ctx>-sqlx`/`crates/migration` + `crates/integration-contracts/src/editable_fields.rs` (`RecordEditableFieldSchema`/`EditableFieldDefinition`/`supports_agent_patches`) + `crates/integration-contracts/src/catalog.rs` (`ContextId`) + `crates/contexts/agent-orchestration/src/suggestion.rs` (`SuggestionCapability::RecordFieldPatch { owner }`, `RecordFieldPatch`, `FieldSuggestionEvidence`, `Provenance`).
- **Reference routing:** every `@<context>/<id>` resolves via `crates/proto/proto/erpx/reference/service.proto` (`ReferenceQueryService.Resolve/Search/ListBacklinks`) → `apps/console/src/routes.rs` (`AppRoute::ReferenceItem`) → typed edge `record_reference_edges(host,target,kind,ordinal)` (shared, append-only, deduped, soft cap 50, backlink-indexed).
- **Editable-field coverage (31/31 OwnerPublished, 2026-08-29):** `sales.quote` (4), `customer-360.note`, `procurement.requisition`, `maintenance.request`, `marketing.content` (2), `finance.journal` (2), `projects.task` (2), `support.ticket` (1), `inventory.move`, `crm.note`, `quality.inspection`, `operations.work_order`, `people_payroll.record`, `asset.asset`, `brand.profile`, `commercial.subscription`, `commerce.order`, `field-service.dispatch`, `learning.enrollment`, `benefits.election`, `talent.candidate`, `manufacturing.order`, `planning.scenario`, `project.project`, `legal.contract`, `collaboration.thread`, `agent.assignment`, `organization.holding`, `identity.principal`, `platform.holding`, `customer.account` (31); 0 `OwnerUnsupported` — `editable_fields_spec_ahead.rs` retained as history. Bounds: `MAX_FIELD_PATCH_OPERATIONS=64`, `MAX_FIELD_EVIDENCE_REFS=16`.
- **Verification:** Q15=C per-module Chromium E2E (type `@` → picker → chip → backlinks; AI `Request→Ready→Apply/Conflicted/Reject` with ≥3 refs) + disposable-DB harnesses `cargo xtask test-affected` per owning crate + `reference`/`collaboration`/`agent-orchestration`; cross-module smoke over canonical spine above.
## ASCII UX mockups (2026-08-29)

- **Per-module mockups (31):** each `Plan/03-cross-cutting/@-and-ai/<ctx>.md` `## ASCII UX` section traces to `apps/console/src/routes.rs` (`AppRoute`) + `apps/console/src/features/<ctx>` + `crates/ui-domains/*` + `crates/proto/proto/erpx/<ctx>/service.proto` + editable-field schema; ANSI ```ansi``` + plain fallback; chips/AI rail/backlinks illustrated.
- **Journeys:** `Plan/03-cross-cutting/@-and-ai/_journeys.md` (3 journeys: Revenue `sales→finance`, Supply `projects→procurement→inventory`, People+Ops `talent→people_payroll` + `support→quality→maintenance`) cross-links above modules; spine validated via `ReferenceQueryService.Search/ListBacklinks`.
## Universal @ + Field AI code cutover (2026-08-30)

- **Shared kernel (code is truth, additive):**
  - `crates/integration-contracts/src/reference_edge.rs` — `ReferenceEdgeKind` (10 kinds: relates_to/depends_on/blocks/billed_by/fulfills/authored_with/implements/supersedes/derived_from/evidence_for), `RecordResolutionState` (Resolved/Stale/Degraded/Offline/Tombstone), `RecordReferenceEdge`/`View`, `MAX_REFERENCE_EDGES_PER_HOST=50` (Q9=C soft cap, deduped, ordered).
  - `crates/integration-contracts/src/catalog.rs` — `ContextId::supports_record_references() == true` for all 31 contexts (always true; thin `catalog` spec-ahead flagged in docs).
  - `crates/domain/src/reference_graph.rs` — pure validation `validate_add`/`validate_reorder` (append-only, deduped target, self-reference guard, cap-50).
  - `crates/proto/proto/erpx/interface/reference.proto` — `ReferenceEdgeKind` enum (10) shared by `ReferenceQueryService`/`ListBacklinks`.
  - `crates/migration/migrations_holding_write/0011_universal_record_reference_graph.sql` — broadens `suggestion_envelopes.owner_context` to all 31 contexts, broadens `record_relations.relation_kind` to include `billed_by/fulfills/authored_with`, adds `record_relations_host_ordinal` + `record_relations_target_backlinks` indexes; aliases existing `erpx_collaboration.record_relations` as canonical `record_reference_edges(host,target,kind,ordinal)` per Plan/ADR-019 addendum.
  - `crates/console-features/ai-collaboration/src/gateway.rs` — `UniversalAtTokenizer` (`@<context>/<id>` → `StableRecordRef` + `@user/@group/@agent-pool` typed) + `FieldPatchRequest`/`FieldPatchReviewDecision` (owner-scoped `RecordFieldPatch` ghost rail, human-approved).

- **Per-module vertical slices (31/31 parallel, single branch, 2026-08-29):** shared kernel covers all contexts without per-context table duplication. Each context's deep spec `Plan/03-cross-cutting/@-and-ai/<ctx>.md` traces to `crates/proto/proto/erpx/<ctx>/service.proto` (≈539 RPCs) + `crates/contexts/<ctx>/src/domain.rs` + `crates/adapters/<ctx>-sqlx` + `crates/integration-contracts/src/editable_fields.rs` (`OwnerPublished` 31/31: sales.quote/customer-360.note/procurement.requisition/maintenance.request/marketing.content + finance.journal/projects.task/support.ticket + inventory.move/crm.note/quality.inspection/operations.work_order/people_payroll.record + asset/brand/commercial/commerce/field-service/learning/benefits/talent/manufacturing/planning/project/legal/collaboration/agent/organization/identity/platform/customer) + `crates/contexts/agent-orchestration/src/suggestion.rs` (`SuggestionCapability::RecordFieldPatch { owner }`, `Queued→Ready→Applied/Conflicted` ghost+human Apply, Q5=A/Q11=A/Q12=A). Enterprise workspaces `manufacturing/planning/projects/ecommerce/talent/learning/benefits/field-service/collaboration/agent` now have full `EnterpriseWorkspaceGateway` List/Detail via `WorkspaceKind` record families (5 each) plus per-context `record.notes` editable field and `@` chip/Backlinks/AI rail binding. No atomic 31-way cutover (Q14=A) — parallel on single branch.
- **Routes & UX:** `apps/console/src/routes.rs` (`/reference/item/:id` + `/finance,/sales,/procurement,/inventory,/projects,/talent,/field-service,/work` etc.) already maps; shared tokenizer/chips (`[cyan @ctx/id]`, `[red tombstone]`, `Stale/Degraded` badges) + single `FieldSuggestionCard` ghost/diff rail consumed per slice.

