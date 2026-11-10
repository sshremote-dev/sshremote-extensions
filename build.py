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


def write(src: Path, dest: Path, name: str, m: dict, programs: list) -> dict:
    # Only what the manifest names goes in: not the sources or the build's leftovers.
    named = ([m["main"]] if m.get("main") else []) + [l["file"] for l in m.get("contributes", {}).get("layers", [])] + programs
    with zipfile.ZipFile(dest / name, "w", zipfile.ZIP_DEFLATED) as z:
        # Fixed times: the same sources make the same package and checksum.
        def put(path: str, data: bytes):
            info = zipfile.ZipInfo(path, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, data)
        put("extension.json", json.dumps(m, indent=2).encode())
        for f in sorted(named):
            put(f, (src / f).read_bytes())
    data = (dest / name).read_bytes()
    return {"url": name, "sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}


def pack(src: Path, dest: Path) -> dict:
    if (src / "make.py").exists():
        subprocess.run([sys.executable, "-I", str(src / "make.py"), str(src)], check=True)
    m = json.loads((src / "extension.json").read_text(encoding="utf-8"))
    offer = {"id": m["id"], "name": m.get("name", m["id"]), "version": m["version"], "description": m.get("description", "")}
    programs = m.get("programs", {})
    if not programs:
        return offer | write(src, dest, f"{m['id']}-{m['version']}.sshx", m, [])
    # One package for each platform, with only its program.
    offer["platforms"] = {}
    for platform, file in programs.items():
        one = m | {"programs": {platform: file}}
        offer["platforms"][platform] = write(src, dest, f"{m['id']}-{m['version']}-{platform}.sshx", one, [file])
    offer["size"] = max(p["size"] for p in offer["platforms"].values())
    return offer


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
