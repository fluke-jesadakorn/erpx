"""Additional independent policy corpus, with explicit conservative Rust gaps.

All cryptographic evidence is synthetic. This script never supplies expected
results from Rust, and retains no private keys. Python remains development-only.
"""
import hashlib
import io
import json
import sys
import tempfile
from datetime import timedelta
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from pyhanko.pdf_utils import generic
from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
from pyhanko.pdf_utils.reader import PdfFileReader
from pyhanko.sign.fields import MDPPerm, SigFieldSpec, append_signature_field
from pyhanko.sign.signers.pdf_signer import PdfTimeStamper

import generate as g


def dictionary(values):
    return generic.DictionaryObject({generic.pdf_name(key): value for key, value in values.items()})


def seeded(values, **kwargs):
    writer = IncrementalPdfFileWriter(io.BytesIO(g.unsigned()))
    append_signature_field(writer, SigFieldSpec(sig_field_name="Signature"))
    field = writer.root["/AcroForm"]["/Fields"][-1].get_object()
    field[generic.pdf_name("/SV")] = dictionary(values)
    output = io.BytesIO(); writer.write(output)
    return g.signed(source=output.getvalue(), ignore_seed=True, **kwargs)


def dss_vri(pdf, extend=False):
    writer = IncrementalPdfFileWriter(io.BytesIO(pdf))
    if extend:
        dss = writer.root["/DSS"]
        certificates = dss["/Certs"]
        certificates.append(writer.add_object(generic.StreamObject(stream_data=g.SIGNER.public_bytes(serialization.Encoding.DER))))
        writer.update_container(certificates)
        writer.update_container(dss)
    else:
        signature = PdfFileReader(io.BytesIO(pdf)).embedded_signatures[0]
        key = generic.pdf_name("/" + hashlib.sha1(signature.pkcs7_content).hexdigest().upper())
        certificate = writer.add_object(generic.StreamObject(stream_data=g.ROOT.public_bytes(serialization.Encoding.DER)))
        revocation = writer.add_object(generic.StreamObject(stream_data=g.crl()))
        dss = dictionary({"/Certs": generic.ArrayObject([certificate]),
            "/CRLs": generic.ArrayObject([revocation]),
            "/VRI": writer.add_object(generic.DictionaryObject({key: writer.add_object(dictionary({
                "/Cert": generic.ArrayObject([certificate]), "/CRL": generic.ArrayObject([revocation]),
                "/TU": generic.pdf_string("D:20261027060000Z")
            }))}))})
        writer.root[generic.pdf_name("/DSS")] = writer.add_object(dss)
        writer.update_root()
    output=io.BytesIO(); writer.write(output)
    return output.getvalue()


def metadata(pdf, xml=None, reuse=False):
    writer=IncrementalPdfFileWriter(io.BytesIO(pdf))
    # The fixture explicitly controls XMP; the writer normally regenerates it.
    writer._update_meta=lambda:None
    xml=xml or b'''<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title><rdf:Alt><rdf:li xml:lang="x-default">Synthetic metadata</rdf:li></rdf:Alt></dc:title></rdf:Description>
</rdf:RDF></x:xmpmeta><?xpacket end="w"?>'''
    if reuse:
        reference=writer.root.get_value_as_reference("/Metadata")
        reference.get_object()._data=xml
        reference.get_object()._encoded_data=None
        writer.mark_update(reference)
    else:
        writer.root[generic.pdf_name("/Metadata")]=writer.add_object(generic.StreamObject(
            dict_data={generic.pdf_name("/Type"):generic.pdf_name("/Metadata"),generic.pdf_name("/Subtype"):generic.pdf_name("/XML")},stream_data=xml))
        writer.update_root()
    output=io.BytesIO(); writer.write(output)
    return output.getvalue()


def dss_drawing_alias():
    writer=IncrementalPdfFileWriter(io.BytesIO(g.unsigned()))
    page,_=writer.find_page_for_modification(0)
    content=page.get_object().get_value_as_reference("/Contents")
    writer.root[generic.pdf_name("/DSS")]=writer.add_object(dictionary({"/Certs":generic.ArrayObject([
        generic.IndirectObject(content.idnum,content.generation,writer)])}))
    writer.update_root()
    unsigned=io.BytesIO();writer.write(unsigned)
    writer=IncrementalPdfFileWriter(io.BytesIO(g.signed(source=unsigned.getvalue())))
    page,_=writer.find_page_for_modification(0)
    content=page.get_object().get_value_as_reference("/Contents")
    content.get_object()._data=b"BT (Malicious drawing update through DSS alias) Tj ET"
    content.get_object()._encoded_data=None
    writer.mark_update(content)
    output=io.BytesIO();writer.write(output)
    return output.getvalue()


def annotation(pdf):
    writer=IncrementalPdfFileWriter(io.BytesIO(pdf))
    page_ref,_=writer.find_page_for_modification(0)
    page=page_ref.get_object()
    annotations=page.get("/Annots",generic.ArrayObject())
    annotations.append(writer.add_object(dictionary({"/Type":generic.pdf_name("/Annot"),
        "/Subtype":generic.pdf_name("/Text"), "/Rect":generic.ArrayObject([generic.NumberObject(n) for n in (10,10,20,20)]),
        "/Contents":generic.pdf_string("Synthetic annotation")})))
    page[generic.pdf_name("/Annots")]=annotations
    writer.mark_update(page_ref)
    output=io.BytesIO(); writer.write(output)
    return output.getvalue()


def document_timestamp(pdf):
    output=io.BytesIO()
    PdfTimeStamper(timestamper=g.DummyTimeStamper(g.asn_certificate(g.TSA),g.asn_key(g.TSA_KEY),fixed_dt=g.MOMENT-timedelta(minutes=1))).timestamp_pdf(
        IncrementalPdfFileWriter(io.BytesIO(pdf)),"sha256",output=output)
    return output.getvalue()


def scoped_crl(number, delta=None, revoked=(), indirect=False, only_ca=False, freshest=False, aki=None, removed=(), entry_issuer=g.ROOT_NAME, issuer_on_first_only=False):
    builder=x509.CertificateRevocationListBuilder().issuer_name(g.ROOT_NAME)
    builder=builder.last_update(g.MOMENT-timedelta(hours=2 if delta is None else 1))
    builder=builder.next_update(g.MOMENT+timedelta(days=1))
    builder=builder.add_extension(x509.CRLNumber(number),critical=False)
    if aki is not None:
        identifier=x509.AuthorityKeyIdentifier.from_issuer_public_key(g.ROOT_KEY.public_key()) if aki else x509.AuthorityKeyIdentifier(b"\x00"*20,None,None)
        builder=builder.add_extension(identifier,critical=False)
    if delta is not None:
        builder=builder.add_extension(x509.DeltaCRLIndicator(delta),critical=True)
    if freshest:
        builder=builder.add_extension(x509.FreshestCRL([x509.DistributionPoint(
            full_name=[x509.UniformResourceIdentifier("http://unreachable.invalid/never-fetch-delta")],
            relative_name=None,reasons=None,crl_issuer=None)]),critical=False)
    if indirect or only_ca:
        builder=builder.add_extension(x509.IssuingDistributionPoint(full_name=None,relative_name=None,
            only_contains_user_certs=False,only_contains_ca_certs=only_ca,only_some_reasons=None,
            indirect_crl=indirect,only_contains_attribute_certs=False),critical=True)
    for index,serial in enumerate(revoked):
        entry=x509.RevokedCertificateBuilder().serial_number(serial).revocation_date(g.MOMENT-timedelta(hours=3))
        if serial in removed:
            entry=entry.add_extension(x509.CRLReason(x509.ReasonFlags.remove_from_crl),critical=False)
        if indirect and (not issuer_on_first_only or index==0):
            entry=entry.add_extension(x509.CertificateIssuer([x509.DirectoryName(entry_issuer)]),critical=True)
        builder=builder.add_revoked_certificate(entry.build())
    return builder.sign(g.ROOT_KEY,hashes.SHA256()).public_bytes(serialization.Encoding.DER)


def main(output):
    name=generic.pdf_name; number=generic.NumberObject; text=generic.pdf_string; array=generic.ArrayObject
    root={"roots/root.pem":g.ROOT.public_bytes(serialization.Encoding.PEM)}
    trust={**root,"revocation/root.crl":g.crl()}
    baseline=g.signed()
    # gap=None means exact per-component parity is required. A named gap keeps
    # the independent oracle result intact and requires a conservative result.
    cases=[
        ("seed_empty",seeded({}),trust,None),
        ("seed_advisory_digest",seeded({"/DigestMethod":array([text("sha512")])}),trust,None),
        ("seed_digest_allowed",seeded({"/Ff":number(64),"/DigestMethod":array([text("sha256")])}),trust,None),
        ("seed_digest_forbidden",seeded({"/Ff":number(64),"/DigestMethod":array([text("sha512")])}),trust,None),
        ("seed_reason_allowed",seeded({"/Ff":number(8),"/Reasons":array([text("Approved")])},reason="Approved"),trust,None),
        ("seed_reason_forbidden",seeded({"/Ff":number(8),"/Reasons":array([text("Approved")])},reason="Other"),trust,None),
        ("seed_reason_omitted",seeded({"/Ff":number(8)}),trust,None),
        ("seed_timestamp_required",seeded({"/TimeStamp":dictionary({"/Ff":number(1),"/URL":text("http://unreachable.invalid/never-fetch-timestamp")})}),trust,None),
        ("seed_subfilter_allowed",seeded({"/Ff":number(2),"/SubFilter":array([name("/adbe.pkcs7.detached")])}),trust,None),
        ("seed_subfilter_forbidden",seeded({"/Ff":number(2),"/SubFilter":array([name("/ETSI.CAdES.detached")])}),trust,None),
        ("seed_approval_required",seeded({"/MDP":dictionary({"/P":number(0)})}),trust,None),
        ("seed_certification_required",seeded({"/MDP":dictionary({"/P":number(1)})}),trust,None),
        ("seed_matching_certification",seeded({"/MDP":dictionary({"/P":number(1)})},permission=MDPPerm.NO_CHANGES),trust,None),
        ("seed_subject_certificate",seeded({"/Cert":dictionary({"/Ff":number(1),"/Subject":array([generic.ByteStringObject(g.SIGNER.public_bytes(serialization.Encoding.DER))])})}),trust,None),
        ("seed_wrong_subject_certificate",seeded({"/Cert":dictionary({"/Ff":number(1),"/Subject":array([generic.ByteStringObject(g.TSA.public_bytes(serialization.Encoding.DER))])})}),trust,None),
        ("seed_issuer_chain_allowed",seeded({"/Cert":dictionary({"/Ff":number(2),"/Issuer":array([generic.ByteStringObject(g.ROOT.public_bytes(serialization.Encoding.DER))])})}),trust,None),
        ("seed_issuer_chain_forbidden",seeded({"/Cert":dictionary({"/Ff":number(2),"/Issuer":array([generic.ByteStringObject(g.TSA.public_bytes(serialization.Encoding.DER))])})}),trust,None),
        ("seed_subject_dn_allowed",seeded({"/Cert":dictionary({"/Ff":number(8),"/SubjectDN":array([dictionary({"/CN":text("Fictional Signer")})])})}),trust,None),
        ("seed_subject_dn_forbidden",seeded({"/Cert":dictionary({"/Ff":number(8),"/SubjectDN":array([dictionary({"/CN":text("Other Signer")})])})}),trust,None),
        ("seed_key_usage_allowed",seeded({"/Cert":dictionary({"/Ff":number(32),"/KeyUsage":array([text("11XXXXXXX")])})}),trust,None),
        ("seed_key_usage_forbidden",seeded({"/Cert":dictionary({"/Ff":number(32),"/KeyUsage":array([text("01XXXXXXX")])})}),trust,None),
        ("seed_key_usage_forbidden_only_legacy_gap",seeded({"/Cert":dictionary({"/Ff":number(32),"/KeyUsage":array([text("0XXXXXXXX")])})}),trust,"SEED_VALUE_CONSTRAINTS_FAILURE"),
        ("seed_key_usage_alternatives",seeded({"/Cert":dictionary({"/Ff":number(32),"/KeyUsage":array([text("0XXXXXXXX"),text("11XXXXXXX")])})}),trust,None),
        ("dss_new_vri",dss_vri(baseline),trust,None),
        ("dss_new_vri_append",dss_vri(dss_vri(baseline),extend=True),trust,None),
        ("dss_existing_vri_append",dss_vri(g.signed(source=dss_vri(baseline),field_name="Second"),extend=True),trust,None),
        ("xmp_update_p2",metadata(g.signed(permission=MDPPerm.FILL_FORMS)),trust,None),
        ("xmp_update_p1",metadata(g.signed(permission=MDPPerm.NO_CHANGES)),trust,None),
        ("xmp_existing_stream_update",metadata(g.signed(source=metadata(g.unsigned())),reuse=True),trust,None),
        ("xmp_malformed",metadata(baseline,xml=b"<x:xmpmeta xmlns:x='adobe:ns:meta/'><unclosed>"),trust,None),
        ("xmp_wrong_root",metadata(baseline,xml=b"<other/>"),trust,None),
        ("xmp_entity",metadata(baseline,xml=b"<!DOCTYPE xmpmeta [<!ENTITY injected 'synthetic'>]><x:xmpmeta xmlns:x='adobe:ns:meta/'>&injected;</x:xmpmeta>"),trust,None),
        ("dss_drawing_alias",dss_drawing_alias(),trust,None),
        ("annotation_update_p3",annotation(g.signed(permission=MDPPerm.ANNOTATE)),trust,None),
        ("document_timestamp",document_timestamp(baseline),trust,None),
        ("delta_good",baseline,{**root,"revocation/base.crl":scoped_crl(10,freshest=True),"revocation/delta.crl":scoped_crl(11,delta=10)},None),
        ("crl_authority_key_binding",baseline,{**root,"revocation/root.crl":scoped_crl(10,aki=True)},None),
        ("crl_wrong_authority_key",baseline,{**root,"revocation/root.crl":scoped_crl(10,aki=False)},None),
        ("delta_revoked",baseline,{**root,"revocation/base.crl":scoped_crl(10,freshest=True),"revocation/delta.crl":scoped_crl(11,delta=10,revoked=(2,))},None),
        ("delta_removes_revocation",baseline,{**root,"revocation/base.crl":scoped_crl(10,freshest=True,revoked=(2,)),"revocation/delta.crl":scoped_crl(11,delta=10,revoked=(2,),removed=(2,))},None),
        ("delta_without_base",baseline,{**root,"revocation/delta.crl":scoped_crl(11,delta=10)},None),
        ("delta_present_without_advertisement",baseline,{**root,"revocation/base.crl":scoped_crl(10),"revocation/delta.crl":scoped_crl(11,delta=10,revoked=(2,))},None),
        ("partitioned_ca_only_crl",baseline,{**root,"revocation/root.crl":scoped_crl(10,only_ca=True)},None),
        ("indirect_crl",baseline,{**root,"revocation/root.crl":scoped_crl(10,indirect=True)},None),
        ("indirect_revoked_crl",baseline,{**root,"revocation/root.crl":scoped_crl(10,indirect=True,revoked=(2,))},None),
        ("indirect_other_issuer_context",baseline,{**root,"revocation/root.crl":scoped_crl(10,indirect=True,revoked=(2,3),entry_issuer=x509.Name([x509.NameAttribute(g.NameOID.COMMON_NAME,"Other Certificate Issuer")]),issuer_on_first_only=True)},None),
    ]
    result=[]
    with tempfile.TemporaryDirectory() as directory:
        base=Path(directory)
        for index,(case,pdf,materials,gap) in enumerate(cases):
            material_directory=base/str(index)
            for kind in ("roots","intermediates","revocation"):
                (material_directory/kind).mkdir(parents=True)
            for path,content in materials.items():
                (material_directory/path).write_bytes(content)
            source=base/f"{index}.pdf"; source.write_bytes(pdf)
            try:
                expected=g.run(source,material_directory,moment=g.MOMENT)
            except Exception as error:
                expected={"status":"indeterminate","signatures":[],"reasons":["VALIDATION_FAILED",type(error).__name__]}
            print(case,expected["status"],gap or "exact parity")
            result.append({"name":case,"pdfHex":pdf.hex(),
                "trust":[{"path":path,"hex":content.hex()} for path,content in sorted(materials.items())],
                "expected":{"status":expected["status"],"signatures":[
                    {key:signature[key] for key in ("field","status","integrity","certificateChain","certificateValidity","revocation","timestamp")}
                    for signature in expected["signatures"]]},"conservativeGap":gap})
    output.write_text(json.dumps({"schema":"erpx-pdf-conformance/v1","oracle":"pyHanko 0.37.0 / B0 a5ed8737a", "validationTime":g.MOMENT.isoformat(),"synthetic":True,"cases":result},indent=2)+"\n")


if __name__=="__main__":
    main(Path(sys.argv[1]))
