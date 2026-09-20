"""Synthetic certificate fixtures; no real identities or legally binding acts."""
import tempfile, io, json, sys, importlib.util
from pathlib import Path
from datetime import datetime, timezone, timedelta
from cryptography import x509
from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.hazmat.primitives import serialization, hashes
from asn1crypto import x509 as asn_x509, keys
from pyhanko.pdf_utils.writer import PdfFileWriter, PageObject
from pyhanko.pdf_utils.generic import StreamObject
from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
from pyhanko.sign.signers import SimpleSigner, PdfSigner, PdfSignatureMetadata
from pyhanko.sign.timestamps import DummyTimeStamper
from pyhanko_certvalidator.registry import SimpleCertificateStore
spec=importlib.util.spec_from_file_location('validator',Path(__file__).with_name('validate_pdf.py'))
validator=importlib.util.module_from_spec(spec);spec.loader.exec_module(validator)
now=datetime.now(timezone.utc)
rootkey=rsa.generate_private_key(65537,2048)
name=x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,'Synthetic Test Root')])
def certificate(key,subject,serial,ca=False,tsa=False,expired=False):
 b=x509.CertificateBuilder().subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,subject)])).issuer_name(name).public_key(key.public_key()).serial_number(serial).not_valid_before(now-timedelta(days=10)).not_valid_after(now+timedelta(days=-1 if expired else 30)).add_extension(x509.BasicConstraints(ca=ca,path_length=None),critical=True).add_extension(x509.KeyUsage(digital_signature=not ca,content_commitment=not ca,key_encipherment=False,data_encipherment=False,key_agreement=False,key_cert_sign=ca,crl_sign=ca,encipher_only=None,decipher_only=None),critical=True)
 if tsa:b=b.add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.TIME_STAMPING]),critical=True)
 return b.sign(rootkey,hashes.SHA256())
root=certificate(rootkey,'Synthetic Test Root',1,ca=True)
key=rsa.generate_private_key(65537,2048);cert=certificate(key,'Fictional Signer',2)
tsakey=rsa.generate_private_key(65537,2048);tsacert=certificate(tsakey,'Fictional Timestamp Authority',3,tsa=True)
def asncert(c):return asn_x509.Certificate.load(c.public_bytes(serialization.Encoding.DER))
def asnkey(k):return keys.PrivateKeyInfo.load(k.private_bytes(serialization.Encoding.DER,serialization.PrivateFormat.PKCS8,serialization.NoEncryption()))
def signed(cert):
 writer=PdfFileWriter();content=writer.add_object(StreamObject(stream_data=b''));writer.insert_page(PageObject(content,[0,0,300,300]));base=io.BytesIO();writer.write(base);base.seek(0)
 store=SimpleCertificateStore();store.register(asncert(root))
 signer=SimpleSigner(asncert(cert),asnkey(key),store)
 output=io.BytesIO();PdfSigner(PdfSignatureMetadata(field_name='Signature'),signer,timestamper=DummyTimeStamper(asncert(tsacert),asnkey(tsakey),certs_to_embed=store)).sign_pdf(IncrementalPdfFileWriter(base),output=output)
 return output.getvalue()
def crl(revoked=False):
 b=x509.CertificateRevocationListBuilder().issuer_name(name).last_update(now-timedelta(minutes=1)).next_update(now+timedelta(days=1))
 if revoked:b=b.add_revoked_certificate(x509.RevokedCertificateBuilder().serial_number(2).revocation_date(now-timedelta(hours=1)).build())
 return b.sign(rootkey,hashes.SHA256()).public_bytes(serialization.Encoding.DER)
with tempfile.TemporaryDirectory() as d:
 p=Path(d);trust=p/'trust'
 for sub in ['roots','intermediates','revocation']:(trust/sub).mkdir(parents=True)
 (trust/'roots/root.pem').write_bytes(root.public_bytes(serialization.Encoding.PEM));(trust/'revocation/root.crl').write_bytes(crl())
 pdf=p/'signed.pdf';pdf.write_bytes(signed(cert));r=validator.run(pdf,trust);print('valid',r['status']);assert r['status']=='valid',r
 (trust/'revocation/root.crl').unlink();r=validator.run(pdf,trust);print('unknown revocation',r['status']);assert r['status']=='indeterminate',r
 (trust/'revocation/root.crl').write_bytes(crl(True));r=validator.run(pdf,trust);print('revoked',r['status']);assert r['status']=='invalid',r
 (trust/'revocation/root.crl').write_bytes(crl());pdf.write_bytes(signed(certificate(key,'Expired fictional signer',2,expired=True)));r=validator.run(pdf,trust);print('expired',r['status']);assert r['status']=='invalid',r
 pdf.write_bytes(signed(cert));(trust/'roots/root.pem').unlink();r=validator.run(pdf,trust);print('untrusted',r['status']);assert r['status']=='indeterminate',r
print('5 signature validation scenarios passed')
