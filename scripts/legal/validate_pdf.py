"""Offline, fail-closed PDF signature validation using operator-owned trust data.
No URLs embedded in certificates or documents are fetched. Trust directory:
roots/*.pem, intermediates/*.pem, revocation/*.crl, revocation/*.ocsp.
"""
import json, sys, hashlib, logging
from pathlib import Path
from datetime import datetime, timezone
from asn1crypto import pem, x509
from pyhanko.pdf_utils.reader import PdfFileReader
from pyhanko.sign.validation import validate_pdf_signature
from pyhanko_certvalidator import ValidationContext
logging.disable(logging.CRITICAL)

def certs(directory):
    result=[]
    for path in sorted(directory.glob('*.pem')):
        raw=path.read_bytes()
        for _,_,der in pem.unarmor(raw,multiple=True): result.append(x509.Certificate.load(der))
    return result

def run(path, trust):
    roots=certs(trust/'roots'); other=certs(trust/'intermediates')
    crls=[p.read_bytes() for p in sorted((trust/'revocation').glob('*.crl'))]
    ocsps=[p.read_bytes() for p in sorted((trust/'revocation').glob('*.ocsp'))]
    manifest=[(str(p.relative_to(trust)),hashlib.sha256(p.read_bytes()).hexdigest()) for p in sorted(trust.rglob('*')) if p.is_file()]
    now=datetime.now(timezone.utc)
    def context(mode): return ValidationContext(trust_roots=roots,other_certs=other,crls=crls,ocsps=ocsps,allow_fetching=False,revocation_mode=mode,moment=now)
    signatures=[]
    with open(path,'rb') as stream:
        reader=PdfFileReader(stream,strict=True)
        if reader.encrypted: raise ValueError('ENCRYPTED_PDF')
        for sig in reader.embedded_signatures:
            status=validate_pdf_signature(sig,signer_validation_context=context('require'),ts_validation_context=context('require'))
            chain=validate_pdf_signature(sig,signer_validation_context=context('soft-fail'),ts_validation_context=context('soft-fail'))
            cert=status.signing_cert
            problem=status.trust_problem_indic.name if status.trust_problem_indic else None
            integrity='valid' if status.intact and status.valid else 'invalid'
            validity='valid' if cert.not_valid_before<=now<=cert.not_valid_after else 'invalid'
            revoked=bool(status.revoked)
            revocation='invalid' if revoked else ('valid' if status.trusted else 'indeterminate')
            certificate_chain='valid' if chain.validation_path is not None and chain.trusted else 'indeterminate'
            timestamp=status.timestamp_validity
            timestamp_state=('valid' if timestamp.trusted and timestamp.intact and timestamp.valid else ('invalid' if not timestamp.intact or not timestamp.valid or timestamp.revoked else 'indeterminate')) if timestamp else 'indeterminate'
            modified=status.docmdp_ok is False
            result='invalid' if integrity=='invalid' or revoked or validity=='invalid' or modified or timestamp_state=='invalid' else ('valid' if status.bottom_line and timestamp_state=='valid' else 'indeterminate')
            reasons=[]
            if not roots: reasons.append('NO_OPERATOR_TRUST_ROOTS')
            if problem: reasons.append(problem)
            if not timestamp: reasons.append('NO_TRUSTED_TIMESTAMP')
            if modified: reasons.append('DOCUMENT_MODIFICATION_POLICY_FAILURE')
            signatures.append(dict(field=sig.field_name,status=result,integrity=integrity,certificateChain=certificate_chain,certificateValidity=validity,revocation=revocation,timestamp=timestamp_state,subject=cert.subject.human_friendly,issuer=cert.issuer.human_friendly,serial=str(cert.serial_number),certificateSha256=hashlib.sha256(cert.dump()).hexdigest(),notBefore=cert.not_valid_before.isoformat(),notAfter=cert.not_valid_after.isoformat(),timestampTime=timestamp.timestamp.isoformat() if timestamp else None,reasons=reasons))
    overall='invalid' if any(s['status']=='invalid' for s in signatures) else ('valid' if signatures and all(s['status']=='valid' for s in signatures) else 'indeterminate')
    return dict(status=overall,signatures=signatures,reasons=[] if signatures else ['NO_EMBEDDED_SIGNATURES'],validatedAt=now.isoformat(),trustStoreDigest=hashlib.sha256(json.dumps(manifest).encode()).hexdigest(),mode='offline-strict-revocation',engine='pyHanko 0.37.0')

if __name__=='__main__':
    try: print(json.dumps(run(Path(sys.argv[1]),Path(sys.argv[2])),ensure_ascii=False))
    except Exception as error:
        print(json.dumps(dict(status='indeterminate',signatures=[],reasons=['VALIDATION_FAILED',type(error).__name__])))
