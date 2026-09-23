"""Age-encrypted snapshot transfer. The decryption identity stays on the recovery host."""
import hashlib, json, os, re, shutil, subprocess, tarfile
from pathlib import Path, PurePosixPath

SNAPSHOT = re.compile(r'\d{8}T\d{6}\.\d{6}Z\Z')
RECIPIENT = re.compile(r'age1[0-9a-z]{40,100}\Z')

def age_binary():
    for candidate in ('/opt/homebrew/bin/age','/usr/local/bin/age'):
        if Path(candidate).is_file(): return candidate
    found=shutil.which('age')
    if found: return found
    raise RuntimeError('AGE_BINARY_REQUIRED')

def digest(path):
    value=hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda:source.read(1024*1024),b''): value.update(chunk)
    return value.hexdigest()

def verify_snapshot(snapshot,checksums):
    snapshot=snapshot.resolve()
    if not SNAPSHOT.fullmatch(snapshot.name) or not snapshot.is_dir(): raise RuntimeError('INVALID_SNAPSHOT')
    if any(path.is_symlink() for path in snapshot.rglob('*')): raise RuntimeError('BACKUP_SYMLINK_FORBIDDEN')
    expected=json.loads((snapshot/'checksums.json').read_text())
    if checksums(snapshot)!=expected: raise RuntimeError('BACKUP_CHECKSUM_MISMATCH')
    metadata=json.loads((snapshot/'backup.json').read_text())
    if metadata.get('createdAt')!=snapshot.name: raise RuntimeError('BACKUP_IDENTITY_MISMATCH')
    return metadata

def export_snapshot(home,snapshot,destination,recipient,checksums,offline=False):
    home=home.resolve();snapshot=snapshot.resolve()
    if snapshot.parent!=home/'backups': raise RuntimeError('BACKUP_SCOPE_REQUIRED')
    metadata=verify_snapshot(snapshot,checksums)
    if not RECIPIENT.fullmatch(recipient): raise RuntimeError('AGE_RECIPIENT_REQUIRED')
    if destination.is_symlink() or not destination.is_dir(): raise RuntimeError('OFFSITE_DESTINATION_REQUIRED')
    destination=destination.resolve()
    if not offline and destination.stat().st_dev==home.stat().st_dev: raise RuntimeError('OFFSITE_SEPARATE_DEVICE_REQUIRED')
    output=destination/(snapshot.name+'.tar.age');partial=destination/('.'+snapshot.name+'.tar.age.partial')
    sidecar=destination/(snapshot.name+'.tar.age.json')
    if output.exists() or partial.exists() or sidecar.exists(): raise RuntimeError('OFFSITE_ARCHIVE_EXISTS')
    try:
        with partial.open('xb') as encrypted:
            process=subprocess.Popen([age_binary(),'-r',recipient],stdin=subprocess.PIPE,stdout=encrypted,stderr=subprocess.DEVNULL)
            try:
                with tarfile.open(fileobj=process.stdin,mode='w|') as archive:
                    for path in [snapshot,*sorted(snapshot.rglob('*'))]:
                        name=str(path.relative_to(snapshot.parent))
                        if path.is_symlink(): raise RuntimeError('BACKUP_SYMLINK_FORBIDDEN')
                        info=tarfile.TarInfo(name)
                        info.mtime=0
                        if path.is_dir():
                            info.type=tarfile.DIRTYPE;info.mode=0o700;archive.addfile(info)
                        elif path.is_file():
                            info.size=path.stat().st_size;info.mode=0o600
                            with path.open('rb') as source: archive.addfile(info,source)
                        else: raise RuntimeError('BACKUP_FILE_TYPE_FORBIDDEN')
            except Exception:
                if process.poll() is None: process.kill()
                process.wait()
                raise
            finally:
                if process.stdin and not process.stdin.closed: process.stdin.close()
            if process.wait()!=0: raise RuntimeError('AGE_ENCRYPT_FAILED')
            encrypted.flush();os.fsync(encrypted.fileno())
        # Detect a snapshot modified during the streaming export.
        verify_snapshot(snapshot,checksums)
        info={'version':1,'snapshot':snapshot.name,'createdAt':metadata['createdAt'],
              'ciphertextSha256':digest(partial),'sizeBytes':partial.stat().st_size}
        partial.rename(output)
        with sidecar.open('x') as file:
            json.dump(info,file,indent=2);file.write('\n');file.flush();os.fsync(file.fileno())
        directory_fd=os.open(destination,os.O_RDONLY)
        try: os.fsync(directory_fd)
        finally: os.close(directory_fd)
    except Exception:
        partial.unlink(missing_ok=True)
        # A published archive without its manifest is intentionally left for
        # operator inspection; the next export never overwrites it.
        raise
    print('Encrypted offsite backup:',output)
    return output

def import_archive(home,archive,identity,checksums):
    home=home.resolve();archive=archive.resolve();identity=identity.resolve()
    if not archive.is_file() or not identity.is_file(): raise RuntimeError('OFFSITE_ARCHIVE_OR_IDENTITY_REQUIRED')
    sidecar=archive.with_name(archive.name+'.json')
    info=json.loads(sidecar.read_text())
    name=info.get('snapshot')
    if info.get('version')!=1 or not isinstance(name,str) or not SNAPSHOT.fullmatch(name): raise RuntimeError('OFFSITE_MANIFEST_INVALID')
    if info.get('ciphertextSha256')!=digest(archive) or info.get('sizeBytes')!=archive.stat().st_size: raise RuntimeError('OFFSITE_CIPHERTEXT_MISMATCH')
    parent=home/'backups';parent.mkdir(mode=0o700,exist_ok=True)
    target=parent/name;stage=parent/('.'+name+'.importing')
    if target.exists() or stage.exists(): raise RuntimeError('BACKUP_IMPORT_EXISTS')
    stage.mkdir(mode=0o700)
    process=None
    try:
        process=subprocess.Popen([age_binary(),'-d','-i',str(identity),str(archive)],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
        with tarfile.open(fileobj=process.stdout,mode='r|') as source:
            for item in source:
                parts=PurePosixPath(item.name).parts
                if not parts or parts[0]!=name or any(part in ('','.','..') for part in parts): raise RuntimeError('OFFSITE_ARCHIVE_PATH_FORBIDDEN')
                path=stage.joinpath(*parts[1:])
                if item.isdir(): path.mkdir(mode=0o700,parents=True,exist_ok=True)
                elif item.isfile():
                    if path==stage: raise RuntimeError('OFFSITE_ARCHIVE_PATH_FORBIDDEN')
                    path.parent.mkdir(mode=0o700,parents=True,exist_ok=True)
                    with path.open('xb') as output:
                        member=source.extractfile(item)
                        if member is None: raise RuntimeError('OFFSITE_ARCHIVE_FILE_MISSING')
                        shutil.copyfileobj(member,output,1024*1024)
                else: raise RuntimeError('OFFSITE_ARCHIVE_FILE_TYPE_FORBIDDEN')
        if process.wait()!=0: raise RuntimeError('AGE_DECRYPT_FAILED')
        # The staging directory name differs from the published snapshot name.
        expected=json.loads((stage/'checksums.json').read_text())
        if checksums(stage)!=expected: raise RuntimeError('BACKUP_CHECKSUM_MISMATCH')
        metadata=json.loads((stage/'backup.json').read_text())
        if metadata.get('createdAt')!=name: raise RuntimeError('BACKUP_IDENTITY_MISMATCH')
        stage.rename(target)
    except Exception:
        if process and process.poll() is None: process.kill();process.wait()
        shutil.rmtree(stage,ignore_errors=True)
        raise
    print('Imported encrypted backup:',target)
    return target
