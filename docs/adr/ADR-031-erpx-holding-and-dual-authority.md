# ERPX Holding and dual authority for ERPX Management

Status: Accepted on 2026-09-24.

ERPX operates as an internal Holding with a dedicated business database pair and private files, while customer billing, provisioning, Platform IAM and service state remain in the Control Plane. ERPX Management appears as a main workspace module alongside Operations and requires both permission from the selected ERPX Holding role and an active Platform assignment. The selected Platform role continues to limit reads and commands, including auditor read-only behavior.

This boundary keeps ERPX's Finance, People and everyday work isolated like any other Holding without treating ERPX as a billable customer or consuming a customer resource slot. Granting only one side of the authority cannot open the module or its APIs. Older Platform page URLs redirect into ERPX Management; the control-plane API remains available only through the same two-sided check.
