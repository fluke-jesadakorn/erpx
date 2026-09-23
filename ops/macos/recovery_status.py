"""Read-only recovery facts. A fresh encrypted archive is not an RPO/RTO certificate."""
import datetime, json
from pathlib import Path
from offsite import RECIPIENT, SNAPSHOT, digest

UTC = datetime.timezone.utc

def archive_time(name):
    if not SNAPSHOT.fullmatch(name): return None
    return datetime.datetime.strptime(name, '%Y%m%dT%H%M%S.%fZ').replace(tzinfo=UTC)

def recovery_status(home: Path, now=None):
    home = home.resolve()
    now = now or datetime.datetime.now(UTC)
    facts = {
        'rpoTargetSeconds': 3600,
        'rtoTargetSeconds': 14400,
        'offsiteConfigured': False,
        'separateDevice': False,
        'latestVerifiedArchive': None,
        'archiveAgeSeconds': None,
        'archiveFreshUnderOneHour': False,
        'rpoProven': False,
        'rtoProven': False,
        'issues': [],
    }
    config_path = home/'config/offsite.json'
    if config_path.is_symlink() or not config_path.is_file():
        facts['issues'].append('OFFSITE_NOT_CONFIGURED')
        return facts
    if config_path.stat().st_mode & 0o077:
        facts['issues'].append('OFFSITE_CONFIG_NOT_PRIVATE')
        return facts
    try:
        config = json.loads(config_path.read_text())
        destination = Path(config['destination'])
        if not isinstance(config.get('recipient'), str) or not RECIPIENT.fullmatch(config['recipient']):
            raise ValueError('Invalid recipient')
    except (KeyError, TypeError, ValueError, OSError):
        facts['issues'].append('OFFSITE_CONFIG_INVALID')
        return facts
    facts['offsiteConfigured'] = True
    if destination.is_symlink() or not destination.is_dir():
        facts['issues'].append('OFFSITE_DESTINATION_UNAVAILABLE')
        return facts
    facts['separateDevice'] = destination.stat().st_dev != home.stat().st_dev
    if not facts['separateDevice']:
        facts['issues'].append('OFFSITE_SEPARATE_DEVICE_REQUIRED')
    valid = []
    for sidecar in destination.glob('*.tar.age.json'):
        try:
            if sidecar.is_symlink(): continue
            info = json.loads(sidecar.read_text())
            name = info['snapshot']
            created = archive_time(name)
            archive = destination/(name+'.tar.age')
            if (info.get('version') != 1 or created is None or
                info.get('createdAt') != name or sidecar.name != archive.name+'.json' or
                not archive.is_file() or archive.is_symlink() or
                info.get('sizeBytes') != archive.stat().st_size or
                info.get('ciphertextSha256') != digest(archive)):
                continue
            valid.append((created, name))
        except (KeyError, TypeError, ValueError, OSError):
            continue
    if not valid:
        facts['issues'].append('OFFSITE_ARCHIVE_UNVERIFIED')
    else:
        created, name = max(valid)
        age = int((now-created).total_seconds())
        facts['latestVerifiedArchive'] = name
        facts['archiveAgeSeconds'] = age
        facts['archiveFreshUnderOneHour'] = 0 <= age <= 3600
        if not facts['archiveFreshUnderOneHour']:
            facts['issues'].append('OFFSITE_ARCHIVE_FUTURE_TIMESTAMP' if age < 0 else 'OFFSITE_ARCHIVE_TOO_OLD')
    facts['issues'].extend(['RPO_STRATEGY_NOT_PROVEN','SEPARATE_HOST_RESTORE_NOT_PROVEN'])
    return facts
