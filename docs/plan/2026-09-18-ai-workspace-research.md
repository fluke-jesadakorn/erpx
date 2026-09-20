# ERPX architecture and AI Workspace: design research

Date: 2026-09-18. English edition of the original research, preserving its 44-pattern comparison and source references. This is design research, not evidence that every capability has shipped. See [Executive implementation](2026-09-18-executive-live-canvas.md) and the linked acceptance record for current implementation status.

## Evidence and boundaries

ERPX uses Next.js/TypeScript, Node.js 24.20.0, Bun 1.2.17 and PostgreSQL 18.6. Rust/Dioxus specifications describe archived intent, not current runtime behavior. The public development origin serves port 3100; production on 3101 remains separate. Request, expense and social AI have specialized preparation paths. Generic field suggestions in `src/server/records.ts` still include a deterministic development provider: an AI button alone is not evidence of live inference.

The user selected OpenCode Go and `muse-spark-1.3-contributor` at `https://opencode.ai/zen/go/v1`. Text and image requests were verified through ERPX during provider integration. This demonstrates the tested connection, not universal extraction accuracy. Page-scoped assistant work and Executive work have their own acceptance records.

These patterns address different dimensions and can be combined. Recommendations below are design judgments for ERPX, not vendor guarantees or benchmark results.

## Operation, timeline and event-driven approaches

| Approach | Meaning | Advantages | Costs | ERPX examples |
| --- | --- | --- | --- | --- |
| Operation / command / transaction | A business intention is authorized, validated and committed through its owner. | Clear intent, invariant protection and a definite result. | Large commands become slow; cross-system consequences need separate coordination. | Save draft, receive goods, approve request, post journal. |
| Timeline / temporal | Effective time, recorded time, validity and scheduling are modeled explicitly. | Historical answers and explainable changes. | Backdating, timezones, overlapping intervals and corrections need precise rules. | Effective rates, assignment history, schedules, retroactive changes. |
| Event-driven | Producers announce committed facts and consumers react. | Independent notifications, projections and background integrations. | Delivery, duplication, ordering, retries, tracing and lag require design. | Receipt triggers inspection work and read-model refresh. |

Commands express intent; events record facts; temporal models explain when facts apply and when they became known. A visible timeline does not imply event sourcing. Current state plus an audit log can provide a timeline, whereas event sourcing makes event history authoritative for rebuilding state.

## Architecture comparison: 44 complementary patterns

| # | Pattern | Suitable work / advantage | Main tradeoff |
| --- | --- | --- | --- |
| 1 | CRUD / resource-oriented | Straightforward current-state master data and directories. | Field edits can hide business intent and transition rules. |
| 2 | Transaction Script | Short use cases with centralized procedural steps. | Repeated rules and large procedures accumulate. |
| 3 | Domain Model / DDD | Clear business language, invariants and ownership boundaries. | Requires domain expertise; excessive abstraction adds cost. |
| 4 | Command / use-case-oriented | Explicit ReceiveGoods or SubmitRequest operations with one validation boundary. | Commands and results require design; a message bus is optional. |
| 5 | State machine / statechart | Allowed lifecycle transitions are explicit and testable. | Parallel states, cancellation and reopening can become complex. |
| 6 | Workflow / BPMN | Human/system handoffs and waiting states are visible. | In-flight work needs a policy when definitions change. |
| 7 | Case management / CMMN | Flexible incident, dispute and investigation work. | Sequence and duration are less predictable. |
| 8 | Rules / decision tables / DMN | Reviewable classification, approval and discount criteria. | Conflicts, gaps and rule versions need governance. |
| 9 | Constraint / optimization-driven | Allocate shifts, vehicles or machines under explicit constraints. | Models may be infeasible; output quality depends on inputs. |
| 10 | Actor model | Identity, state and behavior for concurrent sessions or devices. | Cross-actor transactions and reporting require extra design. |
| 11 | Scheduled / time-triggered | Deadlines, periodic work and close-of-period tasks. | Handle missed runs, timezone changes and duplicates. |
| 12 | Effective-dated / valid-time | Prices, rates and grants valid during business intervals. | Every query needs a reference date and overlap rules. |
| 13 | Bitemporal | Separates effective time from the time information became known. | Corrections and historical queries become more complex. |
| 14 | Ledger / immutable journal | Reconciliable balances and durable movements. | Posting, reversal and reconciliation must be correct. |
| 15 | Event sourcing | Reconstruct state and rebuild projections from authoritative events. | Evolution, replay, correction and migration are expensive. |
| 16 | Current state + audit log | Practical ERP state with change evidence and a timeline. | An incomplete audit cannot reconstruct every historical state. |
| 17 | Batch / DAG pipeline | Dependency-based import, validation and reconciliation. | Reruns must be idempotent; cycles do not fit a finite DAG. |
| 18 | Synchronous request/response | Immediate validation and user-visible results. | Long call chains couple latency and availability. |
| 19 | Event-driven / pub-sub | Multiple independent reactions to committed facts. | Ordering, duplicate delivery and tracing remain necessary. |
| 20 | Queue / competing consumers | Workers absorb bursts and process durable work. | Leases, acknowledgement and idempotency are required. |
| 21 | Stream processing | Continuously updated telemetry and aggregates. | Windows, late arrivals, state and recovery add complexity. |
| 22 | Complex event processing | Detect patterns across multiple events. | Time semantics and false positives require careful modeling. |
| 23 | Orchestration | A coordinator exposes progress, retries and compensation. | It must survive failures without owning every domain. |
| 24 | Choreography | Participants react independently to one another's events. | End-to-end behavior becomes harder to trace. |
| 25 | Saga | Coordinate local transactions across system boundaries. | Compensation is not a perfect rollback of every effect. |
| 26 | Transactional outbox | Commit business state and delivery intent atomically. | Consumers still need duplicate-safe processing. |
| 27 | Change Data Capture | Feed analytics from database changes with less table polling. | Row changes may not express business intent; schemas remain coupled. |
| 28 | CQRS | Optimize read and write models for different purposes. | More models and potential synchronization lag. |
| 29 | Materialized read views | Fast prepared results for expensive joins and summaries. | Freshness, refresh and rebuild policies are necessary. |
| 30 | Layered / N-tier | Familiar separation of UI, application/domain and persistence. | Layers without real responsibilities add forwarding and coupling. |
| 31 | Hexagonal / ports and adapters | Replace providers and test rules through interfaces. | Too many adapters can overcomplicate a small system. |
| 32 | Clean architecture | Keep use cases independent from framework details. | Mapping and abstraction can outweigh simple use cases. |
| 33 | Modular monolith | Domain ownership with a single deployment and local transactions. | Import boundaries and data ownership need enforcement. |
| 34 | Microservices | Independent deployment and scaling by domain. | Network failures, consistency, operations and tracing cost more. |
| 35 | Service-oriented architecture | Enterprise capabilities exposed through contracts. | Central governance or integration can become bottlenecks. |
| 36 | Plugin / extension architecture | Add connectors behind defined extension points. | Compatibility, permissions and isolation require contracts. |
| 37 | Serverless / functions | Invocation-based execution for bursty or isolated work. | Runtime restrictions, latency and pricing depend on the platform. |
| 38 | Backend for frontend | Tailored API composition for web, mobile or artifact hosts. | Adds endpoints and hops when clients have similar needs. |
| 39 | Contract / API-first | Shared schemas for clients, AI tools and integrations. | Specifications need owners, versions and behavioral tests. |
| 40 | Analytical warehouse / read platform | Separate heavy historical analysis from transactions. | ETL, freshness, semantics and authorization must align. |
| 41 | Graph-oriented | Relationships, dependencies and traceability are first-class. | Edge semantics and access rules need design; SQL still has a role. |
| 42 | Local-first / offline sync | Field work continues through unreliable connectivity. | Merging records does not automatically preserve stock or credit invariants. |
| 43 | Data mesh | Domain-owned data products under shared governance. | Requires organizational maturity and can overwhelm small teams. |
| 44 | Cell-based architecture | Isolated service units reduce the failure blast radius. | Routing, capacity and tenant movement become more complex. |

CQRS does not require two databases or a broker. Event-driven systems do not require microservices. Stream processing and complex event processing overlap but are not identical. A notification event need not be suitable for reconstructing authoritative state. A financial ledger does not require event sourcing throughout the ERP, and a scheduler is a trigger rather than the business-state owner.

## Recommended ERPX combination

| Area | Starting approach | Reason |
| --- | --- | --- |
| Application structure | Domain-oriented modular monolith with adapters | Builds on the current runtime with manageable operating cost. |
| Save / submit / approve / post | Typed commands, transactions and state transitions | Preserves business intent and authority. |
| Notifications and AI preparation | Durable jobs, outbox and idempotent workers | Supports retries and visible progress. |
| History | Current state, audit timelines and targeted effective dating | Avoids unnecessary full event-sourcing complexity. |
| Accounting and movements | Domain-owned ledgers and journals | Enables controlled posting, reversal and reconciliation. |
| Work summaries and analytics | Read models and semantic metrics | Provides task-oriented queries with explainable totals. |
| Approval conditions | Versioned rules or decision tables | Keeps policy changes reviewable. |
| Scheduling and resource allocation | A solver subsystem when a real optimization problem exists | A workflow engine alone is not an optimizer. |

This research does not justify a wholesale migration to microservices, actors, full event sourcing or data mesh. Adopt them only for a measured need, such as independent teams, scale or failure isolation.

## AI across workflows

Page assistance and authorized cross-context analysis should share identity, retrieval, provider selection, budgets, provenance and command validation.

| Dimension | Page-scoped assistant | Authorized cross-context assistant |
| --- | --- | --- |
| Context | Current route, record/version, field and selection. | The question and an authorized capability/data catalog. |
| Suitable help | Explain fields, prepare drafts, summarize evidence and identify omissions. | Compare authorized contexts and assemble executive analysis. |
| Retrieval | Reauthorize each referenced record. | Reauthorize each search and aggregate on the server. |
| Visible evidence | Current page, sources and proposed edits. | Company/branch scope, period, metric definition and sources. |
| Mutations | Reviewed proposals applied to an ordinary draft. | User-selected proposals enter normal domain commands. |
| Boundary | Being on a page does not grant access to other records. | Asking for “everything” does not enlarge authority. |

Do not send the whole database to every prompt. Retrieve only necessary evidence. Restricted record names, autocomplete results, errors, counts and citations must also obey policy. Suggestions should include field, proposed value, source location, uncertainty and source/record revision. Users must be able to accept individual fields, edit them or discard the proposal without losing their own work. Saving still invokes domain validation, authorization and current-state checks. Documents and model output are untrusted evidence, not permission to approve, pay or change routing.

## Knowledge, model training and user training

| Capability | Proposed meaning | Important boundary |
| --- | --- | --- |
| Company knowledge | Reviewed SOPs, policies, examples and glossary with owners, revisions and effective dates. | Uploading does not grant universal access or guarantee accurate answers. |
| Retrieval / RAG | Find relevant authorized passages and cite them. | Retrieval does not train model weights or replace live numeric queries. |
| Reviewed corrections | Retain human corrections for evaluation and improvements. | One correction does not automatically become company-wide policy. |
| Fine-tuning | A separate project with permitted training data, objectives and evaluation. | Not a prerequisite for the initial AI Workspace and not a side effect of chat. |
| User onboarding | Teach a person how to complete actual work and review AI drafts. | Different from training the AI; avoid interrupting every workflow. |

Start with owned knowledge and a realistic evaluation set. Business metrics need shared server-side definitions, not just prose descriptions.

## Saved interactive analytics: original proposal

The original research considered isolated HTML/CSS/JavaScript artifacts. The later [AI Live Canvas plan](2026-09-18-executive-live-canvas.md) narrows the initial runtime to validated components and application-owned JavaScript. The following remains a research boundary, not a claim that arbitrary generated code is deployed.

The proposed flow is question → authorized semantic query → bounded result and provenance → artifact revision → isolated viewer. The model proposes metrics, dimensions, filters and periods; the server selects parameterized queries. Official amounts come from deterministic calculations, not model guesses. Simulations must label assumptions and separate simulated values from measured actuals.

Persist artifact identity, owner, presentation revision, bindings, parameters, approved dependencies and provenance. Track metric definitions, grain, units/currency, timezone, filters and freshness. Reopening preserves layout and reauthorizes data queries. Never combine refreshed numbers with stale narratives silently. Expose stale, partial and failed states.

Sharing must account for embedded titles, static values, captions, hidden HTML and prior prompts, not only fresh queries. Recipient access and deliberate output distribution are distinct policies in the later Executive plan. A static snapshot export is also a separate disclosure.

For any future executable artifact runtime, use a separate origin and a restrictive sandbox. Keep ERP session cookies, API credentials and provider secrets outside the artifact. Expose narrowly typed bindings or bridge operations; never a general fetch, SQL or mutation proxy. Validate sender, instance, nonce and schema. An opaque origin of `null` alone is not a trusted identity; a per-instance MessageChannel can provide a tighter bridge. Bound input/output, execution and query cost, and make it possible to stop a failed viewer without losing the primary draft.

An iframe and `connect-src 'none'` do not prove zero network egress. Self-navigation, CSS/image URLs, beacons, forms, popups, nested frames and WebRTC require explicit policies and supported-browser tests. The cited WHATWG, MDN and W3C documents describe those boundaries.

## UX and delivery criteria

| Surface | Question to verify | Evidence |
| --- | --- | --- |
| Forms | Does AI help complete a correct draft without losing edits? | Completion time, accepted/rejected values and preserved input. |
| Document extraction | Can each proposed field point to evidence? | Field accuracy, missing fields and unreadable-source cases. |
| Page chat | Does the answer match this record and scope? | Context binding, source links and stale-record tests. |
| Cross-context analysis | Are metrics correct and authority respected? | Expected calculations and a role matrix. |
| Executive reporting | Are definitions, periods and limitations usable? | Reconciliation against source queries and human review. |
| Saved reports | Do reopening, parameters and refresh preserve meaning? | Revisions, fresh data and stale/partial/error states. |
| Sharing | Is the recipient view correctly bounded? | Different-permission recipients, embedded text and revocation. |
| Training | Can the user complete the real task afterward? | Completion and remaining questions. |
| Provider failure | Can work continue without losing drafts? | Timeout, quota, invalid output and cancellation cases. |

Unavailable capabilities must be labeled honestly. Prefer task language such as “Review these values and apply them to your draft” over implementation jargon.

Sequence: verify provider text/image paths; deliver a page-scoped draft workflow; add authorized semantic metrics; persist and refresh bounded interactive reports; test sharing across different permissions; expand by each page's purpose with owned knowledge and evaluation. Minimum verification includes Holding/Company/Branch isolation, revocation, malicious documents, query budgets, malformed provider output, human approval boundaries, money/date/unit correctness, runtime containment, keyboard/mobile interaction and provider failure.

## Primary references retained from the original research

- [Microsoft CQRS](https://learn.microsoft.com/en-us/azure/architecture/patterns/cqrs)
- [Fowler: Bitemporal History](https://martinfowler.com/articles/bitemporal-history.html)
- [Microsoft Event-driven Architecture](https://learn.microsoft.com/en-us/azure/architecture/guide/architecture-styles/event-driven)
- [Microsoft Event Sourcing](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing)
- [Fowler Enterprise Patterns](https://martinfowler.com/eaaCatalog/)
- [Eric Evans DDD Reference](https://www.domainlanguage.com/wp-content/uploads/2016/05/DDD_Reference_2015-03.pdf)
- [W3C SCXML](https://www.w3.org/TR/scxml/)
- [OMG BPMN/CMMN/DMN](https://www.omg.org/intro/TripleCrown.pdf)
- [Google OR-Tools Job Shop](https://developers.google.com/optimization/scheduling/job_shop)
- [Microsoft Orleans](https://dotnet.github.io/orleans/docs/overview/)
- [TigerBeetle Immutable Transfer](https://docs.tigerbeetle.com/reference/transfer/)
- [Airflow DAGs](https://airflow.apache.org/docs/apache-airflow/stable/core-concepts/dags.html)
- [Microsoft Cloud Patterns](https://learn.microsoft.com/en-us/azure/architecture/patterns/)
- [Apache Flink Use Cases](https://flink.apache.org/what-is-flink/use-cases/)
- [AWS Saga Patterns](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/saga-patterns.html)
- [AWS Transactional Outbox](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/transactional-outbox.html)
- [Debezium Architecture](https://debezium.io/documentation/reference/stable/architecture.html)
- [Microsoft Common Web Architectures](https://learn.microsoft.com/en-us/dotnet/architecture/modern-web-apps-azure/common-web-application-architectures)
- [AWS Hexagonal Architecture](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/hexagonal-architecture.html)
- [Fowler Monolith First](https://martinfowler.com/bliki/MonolithFirst.html)
- [Microsoft Architecture Styles](https://learn.microsoft.com/en-gb/azure/architecture/guide/architecture-styles/)
- [AWS SOA](https://aws.amazon.com/what-is/service-oriented-architecture/)
- [VS Code Extension Host](https://code.visualstudio.com/api/advanced-topics/extension-host)
- [Microsoft BFF](https://learn.microsoft.com/en-us/azure/architecture/patterns/backends-for-frontends)
- [OpenAPI Specification](https://spec.openapis.org/oas/latest.html)
- [Microsoft Database Architecture](https://learn.microsoft.com/en-us/azure/architecture/databases/database-get-started)
- [Neo4j Graph Concepts](https://neo4j.com/docs/getting-started/appendix/graphdb-concepts/)
- [Ink & Switch Local-first](https://www.inkandswitch.com/essay/local-first/)
- [AWS Data Mesh](https://aws.amazon.com/what-is/data-mesh/)
- [AWS Cell-based Architecture](https://docs.aws.amazon.com/solutions/cell-based-architecture-on-aws/)
- [Cube Access Control](https://docs.cube.dev/docs/data-modeling/access-control)
- [dbt Semantic Layer](https://docs.getdbt.com/docs/use-dbt-semantic-layer/dbt-sl)
- [dbt Sources JSON](https://docs.getdbt.com/reference/artifacts/sources-json)
- [MDN iframe](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe)
- [MDN postMessage](https://developer.mozilla.org/en-US/docs/Web/API/Window/postMessage)
- [MDN Channel Messaging](https://developer.mozilla.org/en-US/docs/Web/API/Channel_Messaging_API/Using_channel_messaging)
- [WHATWG Sandboxing](https://html.spec.whatwg.org/multipage/browsers.html#sandboxing)
- [MDN connect-src](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/connect-src)
- [W3C CSP Exfiltration](https://w3c.github.io/webappsec-csp/#security-exfiltration)
