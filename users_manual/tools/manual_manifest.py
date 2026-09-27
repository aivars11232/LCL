#!/usr/bin/env python3
"""Write or check users_manual/MANIFEST.json: the Users Manual snapshot that
the desktop workspace and the Android app both package.

The snapshot is every top-level *.md file, in byte order of name. The digest
is SHA-256 over, for each file: name, NUL, length in decimal, NUL, bytes.

    manual_manifest.py          check; exit 1 when the manifest is stale
    manual_manifest.py --write  rewrite the manifest
"""
import hashlib, json, pathlib, sys

VERSION = "0.3.0"
root = pathlib.Path(__file__).resolve().parent.parent
names = sorted((p.name for p in root.iterdir() if p.is_file() and p.name.endswith(".md")),
               key=lambda n: n.encode())
whole = hashlib.sha256()
files = []
for name in names:
    data = (root / name).read_bytes()
    whole.update(name.encode() + b"\0" + str(len(data)).encode() + b"\0" + data)
    files.append({"name": name, "sha256": hashlib.sha256(data).hexdigest()})
manifest = {"format": 1, "manual": "LCL Users Manual", "version": VERSION,
            "digest": whole.hexdigest(), "files": files}
text = json.dumps(manifest, indent=2) + "\n"
path = root / "MANIFEST.json"
if sys.argv[1:] == ["--write"]:
    path.write_text(text)
    print(f"wrote {path.name}: {len(files)} files, digest {manifest['digest']}")
elif path.exists() and path.read_text() == text:
    print(f"{path.name} is current: {len(files)} files, digest {manifest['digest']}")
else:
    print(f"{path.name} is stale: run {pathlib.Path(__file__).name} --write", file=sys.stderr)
    sys.exit(1)
