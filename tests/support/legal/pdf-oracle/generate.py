"""Regenerate synthetic signed-PDF fixtures using the development-only pyHanko oracle.

All signing keys are generated in memory and discarded. Certificates refer to
fictional identities. No legally binding act or production trust is represented.
"""
import io
import json
import sys
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path

from asn1crypto import keys, x509 as asn_x509
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, rsa
from cryptography.x509 import ocsp
from cryptography.x509.oid import AuthorityInformationAccessOID, ExtendedKeyUsageOID, NameOID
from pyhanko.pdf_utils import generic
from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
from pyhanko.pdf_utils.reader import PdfFileReader
from pyhanko.pdf_utils.writer import PageObject, PdfFileWriter
from pyhanko.sign.fields import FieldMDPAction, FieldMDPSpec, MDPPerm, SigFieldSpec
from pyhanko.sign.signers import PdfSignatureMetadata, PdfSigner, SimpleSigner
from pyhanko.sign.timestamps import DummyTimeStamper
from pyhanko_certvalidator.registry import SimpleCertificateStore

from validate_pdf import run

MOMENT = datetime(2026, 10, 27, 6, 0, tzinfo=timezone.utc)
ROOT_NAME = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "ERPX Synthetic Root")])
ROOT_KEY = rsa.generate_private_key(65537, 2048)
SIGNER_KEY = rsa.generate_private_key(65537, 2048)
TSA_KEY = rsa.generate_private_key(65537, 2048)


def certificate(key, name, serial, ca=False, tsa=False, expired=False, aia=True, future=False, ocsp_signer=False, no_check=False, issuer_name=ROOT_NAME, issuer_key=ROOT_KEY):
    builder = (
        x509.CertificateBuilder()
        .subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)]))
        .issuer_name(issuer_name).public_key(key.public_key()).serial_number(serial)
        .not_valid_before(MOMENT + timedelta(days=1) if future else MOMENT - timedelta(days=365))
        .not_valid_after(MOMENT + timedelta(days=-1 if expired else 3650))
        .add_extension(x509.BasicConstraints(ca=ca, path_length=None), critical=True)
        .add_extension(x509.KeyUsage(digital_signature=not ca, content_commitment=not ca,
            key_encipherment=False, data_encipherment=False, key_agreement=False,
            key_cert_sign=ca, crl_sign=ca, encipher_only=None, decipher_only=None), critical=True)
        .add_extension(x509.SubjectKeyIdentifier.from_public_key(key.public_key()), critical=False)
    )
    if tsa:
        builder = builder.add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.TIME_STAMPING]), critical=True)
    if ocsp_signer:
        builder = builder.add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.OCSP_SIGNING]), critical=False)
    if no_check:
        builder = builder.add_extension(x509.OCSPNoCheck(), critical=False)
    if not ca and aia:
        builder = builder.add_extension(x509.AuthorityInformationAccess([
            x509.AccessDescription(AuthorityInformationAccessOID.OCSP,
                x509.UniformResourceIdentifier("http://unreachable.invalid/never-fetch-ocsp"))
        ]), critical=False)
    return builder.sign(issuer_key, hashes.SHA256())


ROOT = certificate(ROOT_KEY, "ERPX Synthetic Root", 1, ca=True)
SIGNER = certificate(SIGNER_KEY, "Fictional Signer", 2)
TSA = certificate(TSA_KEY, "Fictional Timestamp Authority", 3, tsa=True)


def asn_certificate(value):
    return asn_x509.Certificate.load(value.public_bytes(serialization.Encoding.DER))


def asn_key(value):
    return keys.PrivateKeyInfo.load(value.private_bytes(serialization.Encoding.DER,
        serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))


def unsigned():
    writer = PdfFileWriter()
    content = writer.add_object(generic.StreamObject(stream_data=b"BT (Synthetic fixture) Tj ET"))
    writer.insert_page(PageObject(content, [0, 0, 300, 300]))
    field = writer.add_object(generic.DictionaryObject({
        generic.pdf_name("/FT"): generic.pdf_name("/Tx"),
        generic.pdf_name("/T"): generic.pdf_string("Input"),
        generic.pdf_name("/V"): generic.pdf_string("Before"),
    }))
    other = writer.add_object(generic.DictionaryObject({
        generic.pdf_name("/FT"): generic.pdf_name("/Tx"),
        generic.pdf_name("/T"): generic.pdf_string("Other"),
        generic.pdf_name("/V"): generic.pdf_string("Before"),
    }))
    writer.root[generic.pdf_name("/AcroForm")] = generic.DictionaryObject({
        generic.pdf_name("/Fields"): generic.ArrayObject([field, other])
    })
    output = io.BytesIO()
    writer.write(output)
    return output.getvalue()


def signed(cert=SIGNER, timestamp=True, permission=None, lock=None, source=None, field_name="Signature", signer_key=SIGNER_KEY, prefer_pss=False, md_algorithm="sha256", tsa_cert=TSA, ignore_seed=False, reason=None):
    store = SimpleCertificateStore()
    store.register(asn_certificate(ROOT))
    signer = SimpleSigner(asn_certificate(cert), asn_key(signer_key), store, prefer_pss=prefer_pss)
    output = io.BytesIO()
    metadata = PdfSignatureMetadata(field_name=field_name, md_algorithm=md_algorithm,
        certify=permission is not None, docmdp_permissions=permission or MDPPerm.FILL_FORMS, reason=reason)
    timestamper = DummyTimeStamper(asn_certificate(tsa_cert), asn_key(TSA_KEY),
        certs_to_embed=store, fixed_dt=MOMENT - timedelta(minutes=5)) if timestamp else None
    field_spec = SigFieldSpec(sig_field_name=field_name, field_mdp_spec=lock) if lock else None
    pdf_signer = PdfSigner(metadata, signer, timestamper=timestamper, new_field_spec=field_spec)
    pdf_signer._ignore_sv = ignore_seed  # Development adversarial fixture only.
    pdf_signer.sign_pdf(
        IncrementalPdfFileWriter(io.BytesIO(source or unsigned())), output=output)
    return output.getvalue()


def crl(revoked=(), stale=False, forged=False):
    builder = x509.CertificateRevocationListBuilder().issuer_name(ROOT_NAME)
    builder = builder.last_update(MOMENT - timedelta(days=2 if stale else 1))
    builder = builder.next_update(MOMENT + timedelta(days=-1 if stale else 2))
    for serial in revoked:
        builder = builder.add_revoked_certificate(x509.RevokedCertificateBuilder()
            .serial_number(serial).revocation_date(MOMENT - timedelta(hours=1)).build())
    key = rsa.generate_private_key(65537, 2048) if forged else ROOT_KEY
    return builder.sign(key, hashes.SHA256()).public_bytes(serialization.Encoding.DER)


def ocsp_response(cert, revoked=False, stale=False, responder=ROOT, responder_key=ROOT_KEY, nonce=False, this_offset=None, next_offset=None, issuer=ROOT, no_next=False):
    builder = ocsp.OCSPResponseBuilder().add_response(cert=cert, issuer=issuer,
        algorithm=hashes.SHA1(), cert_status=ocsp.OCSPCertStatus.REVOKED if revoked else ocsp.OCSPCertStatus.GOOD,
        this_update=MOMENT + (this_offset if this_offset is not None else timedelta(days=-2 if stale else -1)),
        next_update=None if no_next else MOMENT + (next_offset if next_offset is not None else timedelta(days=-1 if stale else 2)),
        revocation_time=MOMENT - timedelta(hours=1) if revoked else None,
        revocation_reason=x509.ReasonFlags.key_compromise if revoked else None)
    if nonce:
        builder = builder.add_extension(x509.OCSPNonce(b"synthetic-offline-nonce"), critical=False)
    if responder != ROOT:
        builder = builder.certificates([responder])
    return builder.responder_id(ocsp.OCSPResponderEncoding.HASH, responder).sign(responder_key, hashes.SHA256()).public_bytes(serialization.Encoding.DER)


def update_field(pdf, field_name="Input"):
    writer = IncrementalPdfFileWriter(io.BytesIO(pdf))
    fields = writer.root["/AcroForm"]["/Fields"]
    for field_ref in fields:
        field = field_ref.get_object()
        if field.get("/T") == field_name:
            field[generic.pdf_name("/V")] = generic.pdf_string("After")
            writer.mark_update(field_ref)
    output = io.BytesIO()
    writer.write(output)
    return output.getvalue()


def add_dss(pdf):
    writer = IncrementalPdfFileWriter(io.BytesIO(pdf))
    writer.root[generic.pdf_name("/DSS")] = writer.add_object(generic.DictionaryObject({
        generic.pdf_name("/Certs"): generic.ArrayObject([
            writer.add_object(generic.StreamObject(stream_data=ROOT.public_bytes(serialization.Encoding.DER)))
        ])
    }))
    writer.update_root()
    output = io.BytesIO()
    writer.write(output)
    return output.getvalue()


def tamper_signature(pdf):
    reader = PdfFileReader(io.BytesIO(pdf), strict=True)
    signature = reader.embedded_signatures[0]
    ranges = list(map(int, signature.sig_object["/ByteRange"]))
    content = signature.pkcs7_content
    # Flip a bit in the main signer's signature, retaining a parseable CMS.
    from asn1crypto import cms
    parsed = cms.ContentInfo.load(content)
    value = bytearray(parsed["content"]["signer_infos"][0]["signature"].native)
    value[-1] ^= 1
    parsed["content"]["signer_infos"][0]["signature"] = bytes(value)
    altered = parsed.dump().hex().upper().encode()
    capacity = ranges[2] - ranges[1] - 2
    return pdf[:ranges[1]+1] + altered.ljust(capacity, b"0") + pdf[ranges[2]-1:]


def tamper_timestamp(pdf):
    from asn1crypto import cms
    reader = PdfFileReader(io.BytesIO(pdf), strict=True)
    signature = reader.embedded_signatures[0]
    ranges = list(map(int, signature.sig_object["/ByteRange"]))
    parsed = cms.ContentInfo.load(signature.pkcs7_content)
    for attribute in parsed["content"]["signer_infos"][0]["unsigned_attrs"]:
        if attribute["type"].native == "signature_time_stamp_token":
            info = attribute["values"][0]["content"]["signer_infos"][0]
            value = bytearray(info["signature"].native)
            value[-1] ^= 1
            info["signature"] = bytes(value)
    altered = parsed.dump().hex().upper().encode()
    capacity = ranges[2] - ranges[1] - 2
    return pdf[:ranges[1]+1] + altered.ljust(capacity, b"0") + pdf[ranges[2]-1:]


def remove_signed_permission(pdf):
    writer = IncrementalPdfFileWriter(io.BytesIO(update_field(pdf)))
    for signature in writer.prev.embedded_signatures:
        signature_object = signature.sig_object
        del signature_object["/Reference"]
        writer.mark_update(signature_object.container_ref)
    output = io.BytesIO()
    writer.write(output)
    return output.getvalue()


def bad_dss(pdf):
    writer = IncrementalPdfFileWriter(io.BytesIO(pdf))
    writer.root[generic.pdf_name("/DSS")] = writer.add_object(generic.DictionaryObject({
        generic.pdf_name("/Unexpected"): generic.pdf_string("Not signature maintenance")
    }))
    writer.update_root()
    output = io.BytesIO(); writer.write(output)
    return output.getvalue()


def encrypted():
    writer = PdfFileWriter()
    content = writer.add_object(generic.StreamObject(stream_data=b""))
    writer.insert_page(PageObject(content, [0, 0, 300, 300]))
    writer.encrypt("synthetic-owner-password")
    output = io.BytesIO(); writer.write(output)
    return output.getvalue()


def unsupported_digest(pdf):
    from asn1crypto import cms
    reader = PdfFileReader(io.BytesIO(pdf), strict=True)
    signature = reader.embedded_signatures[0]
    ranges = list(map(int, signature.sig_object["/ByteRange"]))
    parsed = cms.ContentInfo.load(signature.pkcs7_content)
    parsed["content"]["signer_infos"][0]["digest_algorithm"]["algorithm"] = "1.2.3.4.5.6.7"
    altered = parsed.dump().hex().upper().encode()
    capacity = ranges[2] - ranges[1] - 2
    return pdf[:ranges[1]+1] + altered.ljust(capacity, b"0") + pdf[ranges[2]-1:]


def main(output):
    root_material = {"roots/root.pem": ROOT.public_bytes(serialization.Encoding.PEM)}
    baseline_trust = {**root_material, "revocation/root.crl": crl()}
    baseline = signed()
    ec_key = ec.generate_private_key(ec.SECP256R1())
    ec_certificate = certificate(ec_key, "Fictional EC Signer", 4)
    weak_key = rsa.generate_private_key(65537, 1024)
    weak_certificate = certificate(weak_key, "Fictional Weak RSA Signer", 5)
    responder_key = rsa.generate_private_key(65537, 2048)
    responder = certificate(responder_key, "Fictional OCSP Responder", 6, ocsp_signer=True, no_check=True)
    responder_without_revocation = certificate(responder_key, "Fictional OCSP Responder", 7, ocsp_signer=True)
    unauthorized = certificate(responder_key, "Fictional Unauthorized Responder", 8)
    unknown_issuer_key = rsa.generate_private_key(65537, 2048)
    unknown_issuer_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "Untrusted Synthetic Root")])
    unknown_root = certificate(unknown_issuer_key, "Untrusted Synthetic Root", 9, ca=True, issuer_name=unknown_issuer_name, issuer_key=unknown_issuer_key)
    untrusted_tsa = certificate(TSA_KEY, "Untrusted Fictional TSA", 3, tsa=True, issuer_name=unknown_issuer_name, issuer_key=unknown_issuer_key)
    cases = [
        ("trusted_crl", baseline, baseline_trust),
        ("missing_revocation", baseline, root_material),
        ("revoked_signer_crl", baseline, {**root_material, "revocation/root.crl": crl((2,))}),
        ("revoked_tsa_crl", baseline, {**root_material, "revocation/root.crl": crl((3,))}),
        ("expired_signer", signed(certificate(SIGNER_KEY, "Expired Fictional Signer", 2, expired=True)), baseline_trust),
        ("untrusted_root", baseline, {"revocation/root.crl": crl()}),
        ("unsigned", unsigned(), baseline_trust),
        ("modified_signed_content", baseline.replace(b"Synthetic fixture", b"Synthetic tamper!", 1), baseline_trust),
        ("forged_cms_signature", tamper_signature(baseline), baseline_trust),
        ("missing_timestamp", signed(timestamp=False), baseline_trust),
        ("stale_crl", baseline, {**root_material, "revocation/root.crl": crl(stale=True)}),
        ("forged_crl", baseline, {**root_material, "revocation/root.crl": crl(forged=True)}),
        ("trusted_ocsp", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER), "revocation/tsa.ocsp": ocsp_response(TSA)}),
        ("revoked_signer_ocsp", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, revoked=True), "revocation/tsa.ocsp": ocsp_response(TSA)}),
        ("stale_ocsp", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, stale=True), "revocation/tsa.ocsp": ocsp_response(TSA, stale=True)}),
        ("permitted_form_fill", update_field(signed(permission=MDPPerm.FILL_FORMS)), baseline_trust),
        ("forbidden_form_fill", update_field(signed(permission=MDPPerm.NO_CHANGES)), baseline_trust),
        ("permitted_dss_update_p1", add_dss(signed(permission=MDPPerm.NO_CHANGES)), baseline_trust),
        ("permitted_dss_update_p2", add_dss(signed(permission=MDPPerm.FILL_FORMS)), baseline_trust),
        ("second_signature", signed(source=baseline, field_name="Second"), baseline_trust),
        ("fieldmdp_include_forbidden", update_field(signed(lock=FieldMDPSpec(action=FieldMDPAction.INCLUDE, fields=["Input"]))), baseline_trust),
        ("fieldmdp_include_other_allowed", update_field(signed(lock=FieldMDPSpec(action=FieldMDPAction.INCLUDE, fields=["Input"])), "Other"), baseline_trust),
        ("fieldmdp_exclude_allowed", update_field(signed(lock=FieldMDPSpec(action=FieldMDPAction.EXCLUDE, fields=["Input"]))), baseline_trust),
        ("fieldmdp_exclude_other_forbidden", update_field(signed(lock=FieldMDPSpec(action=FieldMDPAction.EXCLUDE, fields=["Input"])), "Other"), baseline_trust),
        ("fieldmdp_all_forbidden", update_field(signed(lock=FieldMDPSpec(action=FieldMDPAction.ALL))), baseline_trust),
        ("rsa_pss", signed(prefer_pss=True), baseline_trust),
        ("ec_p256", signed(cert=ec_certificate, signer_key=ec_key), baseline_trust),
        ("weak_rsa1024", signed(cert=weak_certificate, signer_key=weak_key), baseline_trust),
        ("weak_sha1", signed(md_algorithm="sha1"), baseline_trust),
        ("sha224", signed(md_algorithm="sha224"), baseline_trust),
        ("future_signer", signed(certificate(SIGNER_KEY, "Future Fictional Signer", 2, future=True)), baseline_trust),
        ("expired_tsa", signed(tsa_cert=certificate(TSA_KEY, "Expired Fictional TSA", 3, tsa=True, expired=True)), baseline_trust),
        ("future_tsa", signed(tsa_cert=certificate(TSA_KEY, "Future Fictional TSA", 3, tsa=True, future=True)), baseline_trust),
        ("forged_timestamp", tamper_timestamp(baseline), baseline_trust),
        ("ocsp_nonce", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, nonce=True), "revocation/tsa.ocsp": ocsp_response(TSA, nonce=True)}),
        ("ocsp_delegated_authorized", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, responder=responder, responder_key=responder_key), "revocation/tsa.ocsp": ocsp_response(TSA, responder=responder, responder_key=responder_key)}),
        ("ocsp_delegated_missing_revocation", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, responder=responder_without_revocation, responder_key=responder_key), "revocation/tsa.ocsp": ocsp_response(TSA, responder=responder_without_revocation, responder_key=responder_key)}),
        ("ocsp_responder_unauthorized", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, responder=unauthorized, responder_key=responder_key), "revocation/tsa.ocsp": ocsp_response(TSA, responder=unauthorized, responder_key=responder_key)}),
        ("signed_docmdp_redefinition", remove_signed_permission(signed(permission=MDPPerm.NO_CHANGES)), baseline_trust),
        ("untrusted_tsa", signed(tsa_cert=untrusted_tsa), baseline_trust),
        ("ocsp_wrong_issuer", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, issuer=unknown_root), "revocation/tsa.ocsp": ocsp_response(TSA, issuer=unknown_root)}),
        ("ocsp_future", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, this_offset=timedelta(seconds=2)), "revocation/tsa.ocsp": ocsp_response(TSA, this_offset=timedelta(seconds=2))}),
        ("ocsp_one_second_clock_tolerance", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, this_offset=timedelta(seconds=1)), "revocation/tsa.ocsp": ocsp_response(TSA, this_offset=timedelta(seconds=1))}),
        ("ocsp_one_second_expiry_tolerance", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, next_offset=timedelta(seconds=-1)), "revocation/tsa.ocsp": ocsp_response(TSA, next_offset=timedelta(seconds=-1))}),
        ("ocsp_no_next_update_fresh", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, no_next=True, this_offset=timedelta(minutes=-10)), "revocation/tsa.ocsp": ocsp_response(TSA, no_next=True, this_offset=timedelta(minutes=-10))}),
        ("ocsp_no_next_update_stale", baseline, {**root_material, "revocation/signer.ocsp": ocsp_response(SIGNER, no_next=True, this_offset=timedelta(minutes=-40)), "revocation/tsa.ocsp": ocsp_response(TSA, no_next=True, this_offset=timedelta(minutes=-40))}),
        ("malformed_dss", bad_dss(baseline), baseline_trust),
        ("encrypted_pdf", encrypted(), baseline_trust),
        ("truncated_pdf", baseline[:-100], baseline_trust),
        ("unsupported_digest", unsupported_digest(baseline), baseline_trust),
    ]
    result = []
    with tempfile.TemporaryDirectory() as directory:
        base = Path(directory)
        for index, (name, pdf, material) in enumerate(cases):
            trust = base / str(index)
            for subdirectory in ("roots", "intermediates", "revocation"):
                (trust / subdirectory).mkdir(parents=True)
            for path, content in material.items():
                (trust / path).write_bytes(content)
            source = base / f"{index}.pdf"
            source.write_bytes(pdf)
            try:
                expected = run(source, trust, moment=MOMENT)
            except Exception as error:
                # B0's CLI catches parser/unsupported-algorithm failures here.
                expected = {"status": "indeterminate", "signatures": [], "reasons": ["VALIDATION_FAILED", type(error).__name__]}
            print(name, expected["status"])
            result.append({"name": name, "pdfHex": pdf.hex(),
                "trust": [{"path": path, "hex": content.hex()} for path, content in sorted(material.items())],
                "expected": {"status": expected["status"], "signatures": [
                    {key: signature[key] for key in ("field", "status", "integrity", "certificateChain", "certificateValidity", "revocation", "timestamp")}
                    for signature in expected["signatures"]]}})
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"schema": "erpx-pdf-conformance/v1", "oracle": "pyHanko 0.37.0 / B0 a5ed8737a", "validationTime": MOMENT.isoformat(), "synthetic": True, "cases": result}, indent=2) + "\n")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
