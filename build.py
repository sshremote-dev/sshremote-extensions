"""Builds the static site for sshremote.dev: the page, the downloads and the extension registry.

    python registry/build.py OUT [--download NAME=FILE ...]

Each folder in registry/packs with an extension.json becomes OUT/registry/v1/ID-VERSION.sshx
(a make.py beside it runs first), and OUT/registry/v1/index.json lists them with their SHA-256.
"""
import hashlib
import json
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def pack(src: Path, dest: Path) -> dict:
    if (src / "make.py").exists():
        subprocess.run([sys.executable, "-I", str(src / "make.py"), str(src)], check=True)
    m = json.loads((src / "extension.json").read_text(encoding="utf-8"))
    name = f"{m['id']}-{m['version']}.sshx"
    # Only what the manifest names goes in: not the sources or the build's leftovers.
    named = ["extension.json"] + ([m["main"]] if m.get("main") else []) + [l["file"] for l in m.get("contributes", {}).get("layers", [])] + list(m.get("programs", {}).values())
    files = sorted(src / f for f in named)
    with zipfile.ZipFile(dest / name, "w", zipfile.ZIP_DEFLATED) as z:
        for p in files:
            # Fixed times: the same sources make the same package and checksum.
            info = zipfile.ZipInfo(p.relative_to(src).as_posix(), (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, p.read_bytes())
    data = (dest / name).read_bytes()
    return {"id": m["id"], "name": m.get("name", m["id"]), "version": m["version"],
            "description": m.get("description", ""), "url": name,
            "sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}


def main():
    out = Path(sys.argv[1])
    reg = out / "registry" / "v1"
    reg.mkdir(parents=True, exist_ok=True)
    offers = [pack(d, reg) for d in sorted((HERE / "packs").iterdir()) if (d / "extension.json").exists()]
    (reg / "index.json").write_text(json.dumps({"api": 1, "extensions": offers}, indent=1))
    for f in (HERE.parent / "site").iterdir():
        shutil.copy(f, out / f.name)
    args = sys.argv[2:]
    downloads = out / "download"
    downloads.mkdir(exist_ok=True)
    for i, a in enumerate(args):
        if a == "--download":
            name, file = args[i + 1].split("=", 1)
            shutil.copy(file, downloads / name)
    for o in offers:
        print(f"{o['id']} {o['version']} {o['size']} bytes")


if __name__ == "__main__":
    main()
