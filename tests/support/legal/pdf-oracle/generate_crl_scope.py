"""Independent signed evidence for supported RFC5280 CRL scope semantics.

The B0 oracle supplies every expected state. Synthetic keys are RAM-only.
This generator never imports or invokes Rust and never overwrites R2 receipts.
"""
import json
import sys
import tempfile
from datetime import timedelta
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.x509.oid import NameOID

import generate as g
from generate_policy import scoped_crl

URI = x509.UniformResourceIdentifier("http://unreachable.invalid/scope")
RELATIVE = x509.RelativeDistinguishedName([x509.NameAttribute(NameOID.COMMON_NAME, "Partition")])
ALL_REASONS = frozenset([
    x509.ReasonFlags.key_compromise, x509.ReasonFlags.ca_compromise,
    x509.ReasonFlags.affiliation_changed, x509.ReasonFlags.superseded,
    x509.ReasonFlags.cessation_of_operation, x509.ReasonFlags.certificate_hold,
    x509.ReasonFlags.privilege_withdrawn, x509.ReasonFlags.aa_compromise,
])
KEY_REASON = frozenset([x509.ReasonFlags.key_compromise])


def distribution(names=None, relative=None, reasons=None, issuer=None):
    return x509.DistributionPoint(full_name=names, relative_name=relative, reasons=reasons, crl_issuer=issuer)


def signer(points):
    builder = x509.CertificateBuilder().subject_name(g.SIGNER.subject).issuer_name(g.SIGNER.issuer)
    builder = builder.public_key(g.SIGNER_KEY.public_key()).serial_number(2)
    builder = builder.not_valid_before(g.MOMENT-timedelta(days=365)).not_valid_after(g.MOMENT+timedelta(days=3650))
    for extension in g.SIGNER.extensions:
        builder = builder.add_extension(extension.value, critical=extension.critical)
    builder = builder.add_extension(x509.CRLDistributionPoints(points), critical=False)
    return g.signed(cert=builder.sign(g.ROOT_KEY, hashes.SHA256()))


def partition(*, names=None, relative=None, reasons=None, revoked=False, unknown=False):
    builder = x509.CertificateRevocationListBuilder().issuer_name(g.ROOT_NAME)
    builder = builder.last_update(g.MOMENT-timedelta(hours=1)).next_update(g.MOMENT+timedelta(days=1))
    builder = builder.add_extension(x509.CRLNumber(30), critical=False)
    if names or relative or reasons:
        builder = builder.add_extension(x509.IssuingDistributionPoint(
            full_name=names, relative_name=relative, only_contains_user_certs=False,
            only_contains_ca_certs=False, only_some_reasons=reasons,
            indirect_crl=False, only_contains_attribute_certs=False), critical=True)
    if unknown:
        builder = builder.add_extension(x509.UnrecognizedExtension(x509.ObjectIdentifier("1.2.3.4.5.6"), b"\x05\x00"), critical=True)
    if revoked:
        entry = x509.RevokedCertificateBuilder().serial_number(2).revocation_date(g.MOMENT-timedelta(hours=2))
        entry = entry.add_extension(x509.CRLReason(x509.ReasonFlags.key_compromise), critical=False)
        builder = builder.add_revoked_certificate(entry.build())
    return builder.sign(g.ROOT_KEY, hashes.SHA256()).public_bytes(serialization.Encoding.DER)


def main(output):
    root = {"roots/root.pem": g.ROOT.public_bytes(serialization.Encoding.PEM),
            "revocation/tsa.ocsp": g.ocsp_response(g.TSA)}
    full = {**root, "revocation/full.crl": scoped_crl(10)}
    baseline = g.signed()
    named = signer([distribution([URI])])
    relative = signer([distribution(relative=RELATIVE)])
    expanded = x509.Name([*g.ROOT_NAME, *RELATIVE])
    named_crl = {**root, "revocation/named.crl": partition(names=[URI])}
    cases = [
        ("named_partition_without_complete_good", named, named_crl),
        ("named_mismatch_without_complete_good", named, {**root, "revocation/named.crl": partition(names=[x509.UniformResourceIdentifier("http://unreachable.invalid/other")])}),
        ("named_mismatch_revocation_excluded", named, {**full, "revocation/named.crl": partition(names=[x509.UniformResourceIdentifier("http://unreachable.invalid/other")], revoked=True)}),
        ("relative_names_match", relative, {**root, "revocation/named.crl": partition(relative=RELATIVE)}),
        ("relative_name_matches_full_directory", relative, {**root, "revocation/named.crl": partition(names=[x509.DirectoryName(expanded)])}),
        ("relative_name_revocation", relative, {**root, "revocation/named.crl": partition(relative=RELATIVE, revoked=True)}),
        ("canonical_directory_name", signer([distribution([x509.DirectoryName(expanded)])]), {**root, "revocation/named.crl": partition(names=[x509.DirectoryName(x509.Name([*g.ROOT_NAME, x509.NameAttribute(NameOID.COMMON_NAME, " partition ")]))])}),
        ("uri_scheme_host_case", named, {**root, "revocation/named.crl": partition(names=[x509.UniformResourceIdentifier("HTTP://UNREACHABLE.INVALID/scope")])}),
        ("uri_path_case_mismatch", named, {**root, "revocation/named.crl": partition(names=[x509.UniformResourceIdentifier("http://unreachable.invalid/SCOPE")])}),
        ("one_of_multiple_distribution_points_matches", signer([distribution([x509.UniformResourceIdentifier("http://unreachable.invalid/other")]), distribution([URI])]), named_crl),
        ("reason_partition_incomplete", baseline, {**root, "revocation/key.crl": partition(reasons=KEY_REASON)}),
        ("reason_partition_complete_union", baseline, {**root, "revocation/key.crl": partition(reasons=KEY_REASON), "revocation/rest.crl": partition(reasons=ALL_REASONS-KEY_REASON)}),
        ("reason_partition_missing_one_reason", baseline, {**root, "revocation/key.crl": partition(reasons=KEY_REASON), "revocation/rest.crl": partition(reasons=ALL_REASONS-KEY_REASON-frozenset([x509.ReasonFlags.aa_compromise]))}),
        ("reason_partition_proves_revocation_without_complete_coverage", baseline, {**root, "revocation/key.crl": partition(reasons=KEY_REASON, revoked=True)}),
        ("unknown_critical_alone_cannot_prove_good", baseline, {**root, "revocation/unknown.crl": partition(unknown=True)}),
        ("consistent_remove_deltas", baseline, {**root, "revocation/base.crl": scoped_crl(10, freshest=True, revoked=(2,)), "revocation/a-delta.crl": scoped_crl(11, delta=10, revoked=(2,), removed=(2,)), "revocation/b-delta.crl": scoped_crl(12, delta=10, revoked=(2,), removed=(2,))}),
    ]
    results = []
    with tempfile.TemporaryDirectory() as directory:
        base = Path(directory)
        for index, (name, pdf, materials) in enumerate(cases):
            trust = base/str(index)
            for kind in ("roots", "intermediates", "revocation"):
                (trust/kind).mkdir(parents=True)
            for path, content in materials.items():
                (trust/path).write_bytes(content)
            source = base/f"{index}.pdf"
            source.write_bytes(pdf)
            expected = g.run(source, trust, moment=g.MOMENT)
            print(name, expected["status"])
            results.append({"name": name, "pdfHex": pdf.hex(),
                "trust": [{"path": path, "hex": content.hex()} for path, content in sorted(materials.items())],
                "expected": {"status": expected["status"], "signatures": [
                    {key: value[key] for key in ("field", "status", "integrity", "certificateChain", "certificateValidity", "revocation", "timestamp")}
                    for value in expected["signatures"]]}})
    output.write_text(json.dumps({"schema": "erpx-pdf-conformance/v1", "oracle": "pyHanko 0.37.0 / B0 a5ed8737a",
        "validationTime": g.MOMENT.isoformat(), "synthetic": True, "cases": results}, indent=2)+"\n")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
