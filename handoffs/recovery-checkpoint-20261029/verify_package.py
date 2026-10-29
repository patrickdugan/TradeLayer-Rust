from pathlib import Path
import hashlib,json,sys,zipfile

archive=Path(sys.argv[1])
with zipfile.ZipFile(archive)as z:
    assert z.testzip()is None
    manifest=json.loads(z.read('PACKAGE_MANIFEST.json'))
    for e in manifest['files']:
        data=z.read(e['path'])
        assert len(data)==e['bytes']and hashlib.sha256(data).hexdigest()==e['sha256'],e['path']
print(f"PASS: {len(manifest['files'])} source/evidence files verified; archive SHA-256 {hashlib.sha256(archive.read_bytes()).hexdigest()}")
