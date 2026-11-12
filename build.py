"""Builds the extension registry: each folder in packs/ with an extension.json becomes
OUT/registry/v1/ID-VERSION.sshx (a make.py beside it runs first), and OUT/registry/v1/index.json
lists them with their SHA-256. Signing and upload are the app repository's deploy script's.

    python build.py OUT
"""
import hashlib
import json
import subprocess
import sys
import zipfile
from datetime import date
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


def kind(m: dict) -> str:
    if m.get("programs"):
        return "resident" if m.get("activation") == "startup" else "program"
    return "script" if m.get("main") else "style pack"


def updated(src: Path) -> str:
    # The day the pack's folder last changed in git, so the same commit builds the same index.
    try:
        day = subprocess.run(["git", "log", "-1", "--format=%cs", "--", "."], cwd=src, capture_output=True, text=True).stdout.strip()
    except OSError:
        day = ""
    return day or date.today().isoformat()


def details(src: Path, m: dict) -> dict:
    # Extra keys for the site; the app reads only the ones it knows.
    c = m.get("contributes", {})
    d = {"kind": kind(m), "permissions": m.get("permissions", []),
         "commands": [x.get("label", x.get("id", "")) for x in c.get("commands", [])], "updated": updated(src)}
    if c.get("layers"):
        d["layers"] = [l.get("label", l["file"]) for l in c["layers"]]
    for k in ("icon", "homepage"):
        if k in m:
            d[k] = m[k]
    return d


def pack(src: Path, dest: Path) -> dict:
    if (src / "make.py").exists():
        subprocess.run([sys.executable, "-I", str(src / "make.py"), str(src)], check=True)
    m = json.loads((src / "extension.json").read_text(encoding="utf-8"))
    offer = {"id": m["id"], "name": m.get("name", m["id"]), "version": m["version"], "description": m.get("description", "")}
    offer |= details(src, m)
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
    (reg / "index.json").write_text(json.dumps({"extensions": offers}, indent=1))
    for o in offers:
        print(f"{o['id']} {o['version']} {o['size']} bytes")


if __name__ == "__main__":
    main()
