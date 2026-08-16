#!/usr/bin/env python3
"""Validate the ERPX Markdown requirements library without third-party packages."""

from __future__ import annotations

import re
import sys
from collections import Counter, defaultdict
from pathlib import Path
from urllib.parse import unquote


PLAN = Path(__file__).resolve().parents[1]
REPO = PLAN.parent
REQ_RE = re.compile(r"REQ-[A-Z0-9]+-[0-9]{3}[A-Z]*")
HEADING_RE = re.compile(r"^### (REQ-[A-Z0-9]+-[0-9]{3}[A-Z]*)\s+—\s+(.+)$", re.M)
TABLE_RE = re.compile(r"^\| (REQ-[A-Z0-9]+-[0-9]{3}[A-Z]*) \|", re.M)
LINK_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
E2E_RE = re.compile(r"E2E-([0-9]{2})")

ORIGINAL_CONTEXTS = {
    "assets",
    "brand",
    "commercial",
    "crm",
    "customer-360",
    "customer-authority",
    "finance",
    "identity-access",
    "inventory",
    "legal",
    "maintenance",
    "marketing",
    "operations",
    "organization",
    "people-payroll",
    "platform-administration",
    "pos",
    "procurement",
    "quality",
    "sales",
    "support",
}
ENTERPRISE_CONTEXTS = {
    "benefits",
    "ecommerce",
    "field-service",
    "learning",
    "manufacturing",
    "planning",
    "projects",
    "talent",
}
COLLABORATION_CONTEXTS = {"collaboration", "agent-orchestration"}
EXPECTED_CONTEXTS = ORIGINAL_CONTEXTS | ENTERPRISE_CONTEXTS | COLLABORATION_CONTEXTS

REQUIRED_FILES = {
    "README.md",
    "01-product-foundation.md",
    "02-stakeholders-and-access.md",
    "03-shared-policies-and-user-experience.md",
    "04-approval-governance.md",
    "05-enterprise-work-and-shared-services.md",
    "06-requirements-governance.md",
    "portfolios/07-manufacturing.md",
    "portfolios/08-supply-and-demand-planning.md",
    "portfolios/09-projects.md",
    "portfolios/10-e-commerce.md",
    "portfolios/11-recruiting-and-talent.md",
    "portfolios/12-learning.md",
    "portfolios/13-benefits.md",
    "portfolios/14-field-service.md",
    "portfolios/15-ai-and-collaboration.md",
    "platform/master-reference-data-import-reporting-and-search.md",
    "platform/interface-catalog.md",
    "platform/ai-assistance-and-controlled-extensions.md",
    "platform/reference-routing-and-collaboration-data.md",
    "platform/jurisdiction-and-sector-packs.md",
    "platform/customer-exit-and-portability.md",
    "decisions/README.md",
    "decisions/FSD-001-baseline-and-governance.md",
    "decisions/FSD-002-shared-platform-capabilities.md",
    "decisions/FSD-003-enterprise-suite-expansion.md",
    "decisions/FSD-004-thailand-and-sector-overlays.md",
    "decisions/FSD-005-collaboration-references-and-agents.md",
    "decisions/FSD-006-clean-rebuild-architecture-and-cutover.md",
    "references/regulatory-source-register.md",
    "acceptance/end-to-end-scenarios.md",
    "acceptance/verification-catalog.md",
    "acceptance/traceability-matrix.md",
    "glossary.md",
}

NON_DEFINITION_FILES = {
    PLAN / "README.md",
    PLAN / "06-requirements-governance.md",
    PLAN / "glossary.md",
    PLAN / "acceptance" / "end-to-end-scenarios.md",
    PLAN / "acceptance" / "verification-catalog.md",
    PLAN / "acceptance" / "traceability-matrix.md",
    PLAN / "decisions" / "README.md",
    PLAN / "references" / "regulatory-source-register.md",
}


def add_error(errors: list[str], message: str) -> None:
    errors.append(message)


def markdown_files() -> list[Path]:
    return sorted(PLAN.rglob("*.md"))


def requirement_files() -> list[Path]:
    return [
        path
        for path in markdown_files()
        if path not in NON_DEFINITION_FILES and "decisions" not in path.parts
    ]


def table_cells(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def heading_blocks(text: str) -> list[tuple[str, str]]:
    matches = list(HEADING_RE.finditer(text))
    blocks: list[tuple[str, str]] = []
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        blocks.append((match.group(1), text[match.start() : end]))
    return blocks


def collect_requirements(errors: list[str]) -> tuple[dict[str, Path], dict[str, set[str]]]:
    definitions: dict[str, Path] = {}
    scenarios: dict[str, set[str]] = defaultdict(set)
    for path in requirement_files():
        text = path.read_text(encoding="utf-8")
        for req_id, block in heading_blocks(text):
            if req_id in definitions:
                add_error(errors, f"duplicate requirement {req_id}: {path} and {definitions[req_id]}")
                continue
            definitions[req_id] = path
            for label in ("**Requirement.**", "**Rationale.**", "**Acceptance criteria.**", "**Sources.**"):
                if label not in block:
                    add_error(errors, f"{req_id} is missing {label} in {path.relative_to(REPO)}")
            if "ERPX shall" not in block:
                add_error(errors, f"{req_id} has no ERPX shall obligation")
        for line_number, line in enumerate(text.splitlines(), 1):
            match = TABLE_RE.match(line)
            if not match:
                continue
            req_id = match.group(1)
            cells = table_cells(line)
            if req_id in definitions:
                add_error(errors, f"duplicate requirement {req_id}: {path} and {definitions[req_id]}")
                continue
            definitions[req_id] = path
            if len(cells) != 7:
                add_error(errors, f"{path.relative_to(REPO)}:{line_number} {req_id} must have 7 cells")
                continue
            _, title, obligation, rationale, acceptance, source, scenario_cell = cells
            if not title:
                add_error(errors, f"{req_id} has no title")
            if not obligation.startswith("ERPX shall"):
                add_error(errors, f"{req_id} obligation must start with ERPX shall")
            if not rationale:
                add_error(errors, f"{req_id} has no rationale")
            for label in ("Success:", "Denial:", "Failure:"):
                if label not in acceptance:
                    add_error(errors, f"{req_id} acceptance is missing {label}")
            if not source or ("[" not in source and "FSD-" not in source):
                add_error(errors, f"{req_id} has no source authority")
            found_scenarios = {f"E2E-{value}" for value in E2E_RE.findall(scenario_cell)}
            if not found_scenarios:
                add_error(errors, f"{req_id} has no acceptance scenario")
            scenarios[req_id].update(found_scenarios)
    return definitions, scenarios


def expand_matrix(errors: list[str]) -> tuple[set[str], dict[str, set[str]]]:
    path = PLAN / "acceptance" / "traceability-matrix.md"
    coverage: set[str] = set()
    scenarios: dict[str, set[str]] = defaultdict(set)
    token_re = re.compile(r"REQ-([A-Z0-9]+)-([0-9]{3})(?:\.\.([0-9]{3}))?")
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.startswith("| `REQ-"):
            continue
        cells = table_cells(line)
        if len(cells) != 4:
            add_error(errors, f"traceability row {line_number} must have 4 cells")
            continue
        row_ids: set[str] = set()
        for prefix, start_text, end_text in token_re.findall(cells[0]):
            start = int(start_text)
            end = int(end_text) if end_text else start
            if end < start:
                add_error(errors, f"traceability row {line_number} has reversed range")
                continue
            row_ids.update(f"REQ-{prefix}-{number:03d}" for number in range(start, end + 1))
        if not row_ids:
            add_error(errors, f"traceability row {line_number} has no requirement range")
        duplicate = coverage.intersection(row_ids)
        if duplicate:
            add_error(errors, f"traceability duplicates {sorted(duplicate)}")
        coverage.update(row_ids)
        row_scenarios = {f"E2E-{value}" for value in E2E_RE.findall(cells[3])}
        if not row_scenarios:
            add_error(errors, f"traceability row {line_number} has no scenario")
        for req_id in row_ids:
            scenarios[req_id].update(row_scenarios)
    return coverage, scenarios


def validate_evidence(definitions: dict[str, Path], errors: list[str]) -> None:
    evidence_ids = [req_id.replace("REQ-", "EVD-", 1) + "-01" for req_id in definitions]
    if len(evidence_ids) != len(set(evidence_ids)):
        add_error(errors, "deterministic evidence IDs are not unique")
    catalog = (PLAN / "acceptance" / "verification-catalog.md").read_text(encoding="utf-8")
    prefixes = {req_id.split("-")[1] for req_id in definitions}
    for prefix in sorted(prefixes):
        if f"`{prefix}`" not in catalog:
            add_error(errors, f"verification catalog has no sign-off mapping for {prefix}")


def validate_scenarios(errors: list[str]) -> set[str]:
    path = PLAN / "acceptance" / "end-to-end-scenarios.md"
    text = path.read_text(encoding="utf-8")
    defined = re.findall(r"^## (E2E-[0-9]{2})\s+—", text, re.M)
    defined += re.findall(r"^\| (E2E-[0-9]{2})\s+—", text, re.M)
    counts = Counter(defined)
    for scenario, count in sorted(counts.items()):
        if count != 1:
            add_error(errors, f"scenario {scenario} is defined {count} times")
    expected = {f"E2E-{number:02d}" for number in range(1, 69)}
    actual = set(defined)
    if actual != expected:
        add_error(errors, f"scenario catalog mismatch; missing={sorted(expected-actual)} extra={sorted(actual-expected)}")
    return actual


def validate_links_and_markdown(errors: list[str]) -> None:
    for path in markdown_files():
        text = path.read_text(encoding="utf-8")
        headings: list[int] = []
        for line_number, line in enumerate(text.splitlines(), 1):
            if line.rstrip() != line:
                add_error(errors, f"trailing whitespace: {path.relative_to(REPO)}:{line_number}")
            if line.startswith("#"):
                match = re.match(r"^(#{1,6})\s+\S", line)
                if not match:
                    add_error(errors, f"malformed Markdown heading: {path.relative_to(REPO)}:{line_number}")
                else:
                    headings.append(len(match.group(1)))
        if headings and headings[0] != 1:
            add_error(errors, f"first heading is not H1: {path.relative_to(REPO)}")
        if headings.count(1) != 1:
            add_error(errors, f"document must have exactly one H1: {path.relative_to(REPO)}")
        if text.count("```") % 2:
            add_error(errors, f"unbalanced code fence: {path.relative_to(REPO)}")
        for raw_target in LINK_RE.findall(text):
            target = raw_target.strip().strip("<>")
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            target = unquote(target.split("#", 1)[0])
            if not target:
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                add_error(errors, f"broken link in {path.relative_to(REPO)}: {raw_target}")


def validate_catalogs(errors: list[str]) -> None:
    readme = (PLAN / "README.md").read_text(encoding="utf-8")
    context_root = REPO / "crates" / "contexts"
    installed_contexts = {
        path.name for path in context_root.iterdir() if path.is_dir() and (path / "Cargo.toml").is_file()
    }
    if installed_contexts != EXPECTED_CONTEXTS:
        add_error(
            errors,
            f"repository context set mismatch; missing={sorted(EXPECTED_CONTEXTS-installed_contexts)} "
            f"extra={sorted(installed_contexts-EXPECTED_CONTEXTS)}",
        )
    if not (
        len(ORIGINAL_CONTEXTS) == 21
        and len(ENTERPRISE_CONTEXTS) == 8
        and len(COLLABORATION_CONTEXTS) == 2
        and len(EXPECTED_CONTEXTS) == 31
    ):
        add_error(errors, "context lifecycle taxonomy must remain 21 original + 8 enterprise + 2 collaboration")

    workspace_manifest = (REPO / "Cargo.toml").read_text(encoding="utf-8")
    members_match = re.search(r"(?ms)^members\s*=\s*\[(.*?)^\]", workspace_manifest)
    registered_members = (
        set(re.findall(r'"([^"]+)"', members_match.group(1))) if members_match else set()
    )
    expected_members = {f"crates/contexts/{context}" for context in EXPECTED_CONTEXTS}
    if not members_match or not expected_members.issubset(registered_members):
        add_error(
            errors,
            f"root workspace is missing context members {sorted(expected_members-registered_members)}",
        )
    registered_dependencies = set(
        re.findall(
            r'^erpx-[a-z0-9-]+\s*=\s*\{\s*path\s*=\s*"(crates/contexts/[a-z0-9-]+)"',
            workspace_manifest,
            re.M,
        )
    )
    if not expected_members.issubset(registered_dependencies):
        add_error(
            errors,
            "root workspace dependencies are missing contexts "
            f"{sorted(expected_members-registered_dependencies)}",
        )
    for expected_text in ("Current 21", "Enterprise 8", "Collaboration 2", "31 bounded contexts"):
        if expected_text not in readme:
            add_error(errors, f"README is missing context marker {expected_text}")
    if "| Baseline version | `2.0.0` |" not in readme:
        add_error(errors, "README baseline must be 2.0.0")
    workspace_rows = re.findall(
        r"^\| ([0-9]+) \| ([^|]+) \| (`/[^`]+`) \| ([^|]+) \|$", readme, re.M
    )
    if len(workspace_rows) != 21:
        add_error(errors, f"README has {len(workspace_rows)} workspace rows, expected 21")
    web_only = [row for row in workspace_rows if "Web only" in row[3]]
    full_parity = [row for row in workspace_rows if "Web, Desktop, iOS, Android" in row[3]]
    if len(web_only) != 2 or len(full_parity) != 19:
        add_error(errors, f"channel matrix must have 2 Web-only and 19 four-channel workspaces")
    expected_workspaces = [
        "Operations", "Finance", "Sales", "Procurement", "Inventory", "Assets", "Maintenance",
        "Quality", "Marketing & CRM", "HR & Payroll", "Point of Sale", "Legal", "Manufacturing",
        "Supply & Demand Planning", "Projects", "E-commerce", "Recruiting & Talent", "Learning",
        "Benefits", "Field Service", "AI & Collaboration",
    ]
    actual_workspaces = [row[1].strip() for row in workspace_rows]
    if actual_workspaces != expected_workspaces:
        add_error(errors, "canonical workspace names/order do not match the controlled 21-workspace catalog")
    if {row[1].strip() for row in web_only} != {"E-commerce", "Recruiting & Talent"}:
        add_error(errors, "only E-commerce and Recruiting & Talent may be Web-only")
    expected_routes = [
        "/operations", "/finance", "/sales", "/procurement", "/inventory", "/asset",
        "/maintenance", "/quality", "/marketing", "/people", "/pos", "/legal",
        "/manufacturing", "/planning", "/project", "/commerce", "/talent", "/learning",
        "/benefits", "/field-service", "/ai",
    ]
    actual_routes = [row[2].strip("`") for row in workspace_rows]
    if actual_routes != expected_routes:
        add_error(errors, "canonical workspace routes do not match the 2.0.0 short-slug catalog")
    if "Offline behavior is read-only" not in readme or "`/ai`" not in readme:
        add_error(errors, "README is missing offline or AI & Collaboration workspace boundary")

    proto_root = REPO / "crates" / "proto" / "proto" / "erpx"
    retired_proto_root = proto_root / "platform" / "v1"
    if retired_proto_root.exists():
        add_error(errors, "retired erpx.platform.v1 protobuf source tree must be absent")
    proto_services: set[str] = set()
    for path in proto_root.rglob("*.proto"):
        if "data_plane_agent" in path.parts:
            continue
        proto_services.update(
            re.findall(r"^service\s+(\w+)\s*\{", path.read_text(encoding="utf-8"), re.M)
        )
    interface_text = (PLAN / "platform" / "interface-catalog.md").read_text(encoding="utf-8")
    legacy_rows = re.findall(
        r"^\| (IFC-SVC-0[0-9]{2}) \| (\w+) \|", interface_text, re.M
    )
    if len(proto_services) != 65:
        add_error(errors, f"repository exposes {len(proto_services)} protobuf services, expected 65")
    catalog_ids = re.findall(r"^\| (IFC-[A-Z]+-[0-9]{3}) \|", interface_text, re.M)
    duplicates = [item for item, count in Counter(catalog_ids).items() if count > 1]
    if duplicates:
        add_error(errors, f"duplicate interface IDs: {duplicates}")
    expected_legacy_ids = {f"IFC-SVC-{number:03d}" for number in range(1, 66)}
    actual_legacy_ids = {catalog_id for catalog_id, _service in legacy_rows}
    if actual_legacy_ids != expected_legacy_ids:
        add_error(errors, "retired service catalog IDs must be exactly IFC-SVC-001..065")
    replacement_rows = re.findall(
        r"^\| (IFC-SVC-1[0-9]{2}) \| (\w+) \| ([^|]+?) \| (Command|Query|Stream) \|",
        interface_text,
        re.M,
    )
    expected_replacement_ids = {f"IFC-SVC-{number:03d}" for number in range(101, 166)}
    actual_replacement_ids = {row[0] for row in replacement_rows}
    if actual_replacement_ids != expected_replacement_ids or len(replacement_rows) != 65:
        add_error(errors, "replacement service catalog IDs must be exactly IFC-SVC-101..165")
    replacement_services = {service for _catalog_id, service, _owner, _kind in replacement_rows}
    if replacement_services != proto_services:
        add_error(
            errors,
            "replacement protobuf snapshot mismatch; "
            f"missing={sorted(proto_services-replacement_services)} "
            f"extra={sorted(replacement_services-proto_services)}",
        )
    context_names = {
        "Identity & Access", "Platform Administration", "Organization", "Commercial", "Brand",
        "Customer Authority", "Customer 360", "Support", "Finance", "Sales", "Point of Sale",
        "Procurement", "Inventory", "Assets", "Maintenance", "Quality", "Operations",
        "People & Payroll", "Legal", "Marketing", "CRM", "Manufacturing",
        "Supply & Demand Planning", "Projects", "E-commerce", "Recruiting & Talent", "Learning",
        "Benefits", "Field Service", "Collaboration", "Agent Orchestration",
    }
    per_context: dict[str, set[str]] = defaultdict(set)
    for catalog_id, service, owner, kind in replacement_rows:
        if catalog_id in {"IFC-SVC-163", "IFC-SVC-164", "IFC-SVC-165"}:
            continue
        owner = owner.strip()
        per_context[owner].add(kind)
        if not service.endswith(f"{kind}Service"):
            add_error(errors, f"{catalog_id} service {service} does not match {kind} kind")
    if set(per_context) != context_names:
        add_error(
            errors,
            f"replacement service ownership mismatch; missing={sorted(context_names-set(per_context))} "
            f"extra={sorted(set(per_context)-context_names)}",
        )
    for owner in sorted(context_names):
        if per_context.get(owner) != {"Command", "Query"}:
            add_error(errors, f"{owner} must own exactly one Command and one Query service")
    cross_cutting = {
        ("IFC-SVC-163", "CollaborationStreamService", "Collaboration", "Stream"),
        ("IFC-SVC-164", "ReferenceQueryService", "Item Directory projection", "Query"),
        ("IFC-SVC-165", "DataPlaneStreamService", "Platform Administration", "Stream"),
    }
    normalized_rows = {(a, b, c.strip(), d) for a, b, c, d in replacement_rows}
    if not cross_cutting.issubset(normalized_rows):
        add_error(errors, "replacement catalog is missing the three approved cross-cutting services")
    for marker in ("retires `IFC-SVC-001..065`", "No retired ID"):
        if marker not in interface_text:
            add_error(errors, f"legacy-service retirement boundary is missing {marker}")
    for metadata in (
        "Caller/consumer",
        "Scope",
        "Authentication/authorization",
        "Idempotency",
        "Version",
        "Classification",
        "Failure semantics",
        "Lifecycle and source",
    ):
        if interface_text.count(f"| {metadata} |") < 2:
            add_error(errors, f"legacy and replacement interface metadata are missing {metadata}")
    for target in (
        "CollaborationCommandService",
        "CollaborationQueryService",
        "CollaborationStreamService",
        "ReferenceQueryService",
        "AgentOrchestrationCommandService",
        "AgentOrchestrationQueryService",
        "DataPlaneStreamService",
        "/reference/item/:id",
        "POST /api/v1/<module>/<record>/:id/commands/<command>",
    ):
        if target not in interface_text:
            add_error(errors, f"interface catalog is missing target {target}")

    required_concepts = {
        "PrincipalRef",
        "RecordRef",
        "ItemDescriptor",
        "ThreadAnchor",
        "CommentDocument",
        "CommentRevision",
        "MentionEvent",
        "ReferenceRelation",
        "ActionCard",
        "AgentVersionRef",
        "AgentAssignment",
        "DelegationEdge",
        "AgentSession",
        "BudgetDecision",
    }
    actual_concepts = set(
        re.findall(r"^\| IFC-TYP-[0-9]{3} \| `([A-Za-z]+)` \|", interface_text, re.M)
    )
    if actual_concepts != required_concepts:
        add_error(
            errors,
            f"target contract concept mismatch; missing={sorted(required_concepts-actual_concepts)} "
            f"extra={sorted(actual_concepts-required_concepts)}",
        )

    reference_path = PLAN / "platform" / "reference-routing-and-collaboration-data.md"
    reference_text = reference_path.read_text(encoding="utf-8")
    record_rows: list[list[str]] = []
    in_catalog = False
    for line_number, line in enumerate(reference_text.splitlines(), 1):
        if line == "<!-- first-class-record-catalog:start -->":
            in_catalog = True
            continue
        if line == "<!-- first-class-record-catalog:end -->":
            in_catalog = False
            continue
        if in_catalog and re.match(r"^\| REC-[0-9]{3} \|", line):
            cells = table_cells(line)
            if len(cells) != 6:
                add_error(errors, f"record catalog row {line_number} must have 6 cells")
            else:
                record_rows.append(cells)
    expected_record_ids = {f"REC-{number:03d}" for number in range(1, 32)}
    actual_record_ids = {row[0] for row in record_rows}
    if actual_record_ids != expected_record_ids or len(record_rows) != 31:
        add_error(errors, "first-class record catalog IDs must be exactly REC-001..031")
    expected_contexts = {
        "Identity & Access", "Platform Administration", "Organization", "Commercial", "Brand",
        "Customer Authority", "Customer 360", "Support", "Finance", "Sales", "Point of Sale",
        "Procurement", "Inventory", "Assets", "Maintenance", "Quality", "Operations",
        "People & Payroll", "Legal", "Marketing", "CRM", "Manufacturing",
        "Supply & Demand Planning", "Projects", "E-commerce", "Recruiting & Talent", "Learning",
        "Benefits", "Field Service", "Collaboration", "Agent Orchestration",
    }
    actual_contexts = {row[1] for row in record_rows}
    if actual_contexts != expected_contexts:
        add_error(
            errors,
            f"record catalog context mismatch; missing={sorted(expected_contexts-actual_contexts)} "
            f"extra={sorted(actual_contexts-expected_contexts)}",
        )
    for catalog_id, owner, records, opaque, routes, authority in record_rows:
        if not records or "RecordRef.id" not in opaque or ":id" not in routes or "[" not in authority:
            add_error(errors, f"{catalog_id} ({owner}) lacks records, opaque ref, route, or authority")


def validate_future_context_authority(errors: list[str]) -> None:
    expected = {
        "portfolios/07-manufacturing.md": ("MFG", "FSD-003"),
        "portfolios/08-supply-and-demand-planning.md": ("PLN", "FSD-003"),
        "portfolios/09-projects.md": ("PRJ", "FSD-003"),
        "portfolios/10-e-commerce.md": ("ECM", "FSD-003"),
        "portfolios/11-recruiting-and-talent.md": ("TAL", "FSD-003"),
        "portfolios/12-learning.md": ("LRN", "FSD-003"),
        "portfolios/13-benefits.md": ("BEN", "FSD-003"),
        "portfolios/14-field-service.md": ("FSV", "FSD-003"),
    }
    for relative, (_prefix, authority) in expected.items():
        path = PLAN / relative
        if authority not in path.read_text(encoding="utf-8"):
            add_error(errors, f"{relative} lacks accepted {authority} context authority")
    collaboration = (PLAN / "portfolios" / "15-ai-and-collaboration.md").read_text(encoding="utf-8")
    if "FSD-005" not in collaboration:
        add_error(errors, "portfolios/15-ai-and-collaboration.md lacks accepted FSD-005 authority")


def validate_decisions_and_invariants(errors: list[str]) -> None:
    for number in range(1, 7):
        matches = list((PLAN / "decisions").glob(f"FSD-{number:03d}-*.md"))
        if len(matches) != 1:
            add_error(errors, f"expected one FSD-{number:03d} file")
            continue
        if "- Status: Accepted" not in matches[0].read_text(encoding="utf-8"):
            add_error(errors, f"{matches[0].name} is not Accepted")
    fsd_006 = (PLAN / "decisions" / "FSD-006-clean-rebuild-architecture-and-cutover.md").read_text(
        encoding="utf-8"
    )
    for marker in (
        "Effective baseline: 2.0.0",
        "Domain-Driven Design",
        "Clean Architecture",
        "Hexagonal Architecture",
        "CQRS",
        "schema generation v2",
        "atomic hard cutover",
        "IFC-SVC-101..165",
    ):
        if marker not in fsd_006:
            add_error(errors, f"FSD-006 is missing architecture/cutover marker {marker}")
    data_text = (PLAN / "platform" / "data-integrations-and-cqrs.md").read_text(encoding="utf-8")
    for forbidden_dependency in ("SQLx", "tonic", "prost", "Dioxus", "HTTP", "provider SDK"):
        if forbidden_dependency not in data_text:
            add_error(errors, f"data architecture boundary is missing {forbidden_dependency}")
    collaboration = (PLAN / "portfolios" / "15-ai-and-collaboration.md").read_text(encoding="utf-8")
    if "REQ-COL-001" not in collaboration or "standalone room/DM creation is rejected" not in collaboration:
        add_error(errors, "anchored-thread/no-standalone invariant is missing")
    if "REQ-COL-003" not in collaboration or "permit only narrower thread audiences" not in collaboration:
        add_error(errors, "comment visibility may not be broader than the anchor record")
    for marker in (
        "REQ-COL-031",
        "REQ-COL-032",
        "REQ-COL-033",
        "comment text/reaction cannot mutate",
    ):
        if marker not in collaboration:
            add_error(errors, f"collaboration boundary is missing {marker}")
    if "REQ-AGN-005" not in collaboration or "schedule/event trigger creation is rejected" not in collaboration:
        add_error(errors, "explicit-agent-invocation invariant is missing")
    if "REQ-AGN-016" not in collaboration or "agent cannot execute mutation directly" not in collaboration:
        add_error(errors, "assistive-only agent mutation boundary is missing")
    if "bottom-right docked/resizable panel" not in collaboration or "floating action button with bottom sheet" not in collaboration:
        add_error(errors, "platform-specific floating launcher target is missing")
    reference_text = (PLAN / "platform" / "reference-routing-and-collaboration-data.md").read_text(encoding="utf-8")
    for marker in ("REQ-REF-001", "REQ-REF-002", "REQ-REF-004", "REQ-REF-005", "REQ-REF-006"):
        if marker not in reference_text:
            add_error(errors, f"first-class reference catalog invariant {marker} is missing")
    record_catalog = reference_text.split("<!-- first-class-record-catalog:start -->", 1)[1].split(
        "<!-- first-class-record-catalog:end -->", 1
    )[0]
    if "/workspaces/" in record_catalog or "/references/:id" in record_catalog:
        add_error(errors, "first-class record catalog contains a legacy route")
    if "/reference/item/:id" not in reference_text:
        add_error(errors, "stable copied-link route is missing")
    project_text = (PLAN / "portfolios" / "09-projects.md").read_text(encoding="utf-8")
    for state in ("`proposed`", "`open`", "`in-progress`", "`waiting`", "`resolved`", "`closed`", "`cancelled`"):
        if state not in project_text:
            add_error(errors, f"project core work-state catalog is missing {state}")

    continuity = (PLAN / "platform" / "deployment-continuity-and-migration.md").read_text(encoding="utf-8")
    for gate in ("60 seconds", "24 hours", "four hours", "25 hours", "31-day"):
        if gate not in continuity:
            add_error(errors, f"service-quality hard gate is missing {gate}")

    source_register = (PLAN / "references" / "regulatory-source-register.md").read_text(encoding="utf-8")
    for authority in (
        "rd.go.th/65244.html",
        "catalog.sso.go.th",
        "dpo.pdpc.or.th/registration",
        "bot.or.th/th/our-roles/payment-systems/information-technology-risk-supervision.html",
        "oiceservice.oic.or.th/document/Law/file/11389/11389_3956dee0c06add1ae53b1e3ce8a3e3c4.pdf",
        "ict.moph.go.th/th/extension/1083",
    ):
        if authority not in source_register:
            add_error(errors, f"Thailand official-source register is missing {authority}")


def validate_required_files(errors: list[str]) -> None:
    actual = {str(path.relative_to(PLAN)) for path in markdown_files()}
    for required in sorted(REQUIRED_FILES):
        if required not in actual:
            add_error(errors, f"missing required document Plan/{required}")


def main() -> int:
    errors: list[str] = []
    validate_required_files(errors)
    definitions, direct_scenarios = collect_requirements(errors)
    matrix_ids, matrix_scenarios = expand_matrix(errors)
    definition_ids = set(definitions)
    if len(definition_ids) != 644:
        add_error(errors, f"leaf requirement count is {len(definition_ids)}, expected exactly 644")
    if definition_ids != matrix_ids:
        add_error(errors, f"traceability mismatch; missing={sorted(definition_ids-matrix_ids)} orphan={sorted(matrix_ids-definition_ids)}")
    scenarios = validate_scenarios(errors)
    for req_id in sorted(definition_ids):
        mapped = direct_scenarios.get(req_id, set()) or matrix_scenarios.get(req_id, set())
        if not mapped:
            add_error(errors, f"{req_id} has no scenario mapping")
        unknown = mapped - scenarios
        if unknown:
            add_error(errors, f"{req_id} maps unknown scenarios {sorted(unknown)}")
    validate_evidence(definitions, errors)
    validate_links_and_markdown(errors)
    validate_catalogs(errors)
    validate_future_context_authority(errors)
    validate_decisions_and_invariants(errors)
    if errors:
        print(f"ERPX requirements validation FAILED with {len(errors)} issue(s):")
        for error in errors:
            print(f"- {error}")
        return 1
    prefixes = Counter(req_id.split("-")[1] for req_id in definition_ids)
    print("ERPX requirements validation PASSED")
    print(f"- Markdown documents: {len(markdown_files())}")
    print(f"- Atomic leaf requirements: {len(definition_ids)}")
    print(f"- Requirement prefixes: {len(prefixes)}")
    print(f"- Acceptance scenarios: {len(scenarios)}")
    print("- Bounded contexts: 31 (21 original + 8 enterprise + Collaboration + Agent Orchestration)")
    print("- Canonical workspaces: 21")
    print("- Replacement protobuf services cataloged: 65 (legacy 65 retired)")
    print(f"- Deterministic evidence packages: {len(definition_ids)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
