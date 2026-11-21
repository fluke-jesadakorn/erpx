"""Independent B0 receipts for temporary fail-closed CRL guards.

These are unresolved conformance gaps, not an expansion of ADR-053. All keys are
synthetic/in-memory and discarded; existing immutable B0 receipts are untouched.
"""
import json
import sys
import tempfile
from datetime import timedelta
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization

import generate as g
from generate_policy import scoped_crl


def scoped(*, named=False, reasons=False, revoked=False, ca_only=False, unknown=False):
    builder=x509.CertificateRevocationListBuilder().issuer_name(g.ROOT_NAME)
    builder=builder.last_update(g.MOMENT-timedelta(hours=1)).next_update(g.MOMENT+timedelta(days=1))
    builder=builder.add_extension(x509.CRLNumber(20),critical=False)
    if named or reasons or ca_only:
        point=x509.IssuingDistributionPoint(
            full_name=[x509.UniformResourceIdentifier("http://unreachable.invalid/scoped-crl")] if named else None,
            relative_name=None,only_contains_user_certs=False,only_contains_ca_certs=ca_only,
            only_some_reasons=frozenset([x509.ReasonFlags.key_compromise]) if reasons else None,
            indirect_crl=False,only_contains_attribute_certs=False)
        builder=builder.add_extension(point,critical=True)
    if unknown:
        builder=builder.add_extension(x509.UnrecognizedExtension(x509.ObjectIdentifier("1.2.3.4.5.6"),b"\x05\x00"),critical=True)
    if revoked:
        entry=x509.RevokedCertificateBuilder().serial_number(2).revocation_date(g.MOMENT-timedelta(hours=3))
        entry=entry.add_extension(x509.CRLReason(x509.ReasonFlags.key_compromise),critical=False)
        builder=builder.add_revoked_certificate(entry.build())
    return builder.sign(g.ROOT_KEY,hashes.SHA256()).public_bytes(serialization.Encoding.DER)


def named_signer_pdf():
    builder=x509.CertificateBuilder().subject_name(g.SIGNER.subject).issuer_name(g.SIGNER.issuer)
    builder=builder.public_key(g.SIGNER_KEY.public_key()).serial_number(2)
    builder=builder.not_valid_before(g.MOMENT-timedelta(days=365)).not_valid_after(g.MOMENT+timedelta(days=3650))
    for extension in g.SIGNER.extensions:
        builder=builder.add_extension(extension.value,critical=extension.critical)
    builder=builder.add_extension(x509.CRLDistributionPoints([x509.DistributionPoint(
        full_name=[x509.UniformResourceIdentifier("http://unreachable.invalid/scoped-crl")],relative_name=None,reasons=None,crl_issuer=None)]),critical=False)
    return g.signed(cert=builder.sign(g.ROOT_KEY,hashes.SHA256()))


def main(output):
    root={"roots/root.pem":g.ROOT.public_bytes(serialization.Encoding.PEM)}
    good={**root,"revocation/base.crl":scoped_crl(10)}
    delta_base={**root,"revocation/base.crl":scoped_crl(10,freshest=True)}
    baseline=g.signed();duplicate=scoped_crl(11,delta=10)
    # Exact B0 expectations remain in `expected`. `guard` is the separately
    # required safe result while full scope/delta reconciliation remains open.
    cases=[
        ("competing_delta_good",baseline,{**delta_base,"revocation/a-delta.crl":scoped_crl(11,delta=10),"revocation/b-delta.crl":scoped_crl(12,delta=10)},"indeterminate","competing deltas"),
        ("competing_delta_revoked_first",baseline,{**delta_base,"revocation/a-delta.crl":scoped_crl(11,delta=10,revoked=(2,)),"revocation/b-delta.crl":scoped_crl(12,delta=10)},"invalid","competing deltas with independently verified revocation"),
        ("competing_delta_revoked_second",baseline,{**delta_base,"revocation/a-delta.crl":scoped_crl(11,delta=10),"revocation/b-delta.crl":scoped_crl(12,delta=10,revoked=(2,))},"invalid","competing deltas with independently verified revocation"),
        ("named_scope_good",baseline,{**good,"revocation/scoped.crl":scoped(named=True)},"indeterminate","named scope applicability"),
        ("named_scope_matching_revoked",named_signer_pdf(),{**good,"revocation/scoped.crl":scoped(named=True,revoked=True)},"indeterminate","named scope applicability"),
        ("reason_scope_good",baseline,{**good,"revocation/scoped.crl":scoped(reasons=True)},"indeterminate","reason-partitioned scope"),
        ("reason_scope_revoked",baseline,{**good,"revocation/scoped.crl":scoped(reasons=True,revoked=True)},"indeterminate","reason-partitioned scope"),
        ("independent_revocation_with_unknown_scope",baseline,{**root,"revocation/base.crl":scoped_crl(10,revoked=(2,)),"revocation/scoped.crl":scoped(reasons=True)},"invalid","independently verified complete CRL revocation"),
        ("unknown_critical_scope_with_complete_good",baseline,{**good,"revocation/scoped.crl":scoped(unknown=True)},"indeterminate","unknown critical scope"),
        ("known_excluded_ca_scope",baseline,{**good,"revocation/scoped.crl":scoped(ca_only=True,revoked=True)},"valid",None),
        ("duplicate_identical_delta",baseline,{**delta_base,"revocation/a-delta.crl":duplicate,"revocation/b-delta.crl":duplicate},"valid",None),
        ("scope_unknown_with_good_ocsp",baseline,{**good,"revocation/scoped.crl":scoped(reasons=True),"revocation/signer.ocsp":g.ocsp_response(g.SIGNER),"revocation/tsa.ocsp":g.ocsp_response(g.TSA)},"indeterminate","unsupported scope cannot be hidden by a separate good response"),
    ]
    result=[]
    with tempfile.TemporaryDirectory() as directory:
        base=Path(directory)
        for index,(name,pdf,materials,guard,gap) in enumerate(cases):
            trust=base/str(index)
            for kind in ("roots","intermediates","revocation"):(trust/kind).mkdir(parents=True)
            for path,content in materials.items():(trust/path).write_bytes(content)
            source=base/f"{index}.pdf";source.write_bytes(pdf)
            try:expected=g.run(source,trust,moment=g.MOMENT)
            except Exception as error:expected={"status":"indeterminate","signatures":[],"reasons":["VALIDATION_FAILED",type(error).__name__]}
            print(name,"B0="+expected["status"],"guard="+guard)
            result.append({"name":name,"pdfHex":pdf.hex(),"trust":[{"path":path,"hex":data.hex()} for path,data in sorted(materials.items())],
                "expected":{"status":expected["status"],"signatures":[{key:signature[key] for key in ("field","status","integrity","certificateChain","certificateValidity","revocation","timestamp")} for signature in expected["signatures"]]},
                "guard":guard,"conservativeGap":gap})
    output.write_text(json.dumps({"schema":"erpx-pdf-conformance/v1","oracle":"pyHanko 0.37.0 / B0 a5ed8737a","validationTime":g.MOMENT.isoformat(),"synthetic":True,"acceptance":"unresolved pre-acceptance CRL guard corpus; not ADR-053","cases":result},indent=2)+"\n")


if __name__=="__main__":main(Path(sys.argv[1]))
