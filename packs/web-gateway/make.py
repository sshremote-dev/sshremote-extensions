"""Puts this pack's programs beside it: sshremote built with only this part, from
$HELPERS (a folder holding FEATURE-windows-x86_64.exe and FEATURE-linux-x86_64)."""
import json
import os
import shutil
import sys
from pathlib import Path

here = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent
m = json.loads((here / "extension.json").read_text(encoding="utf-8"))
feature = m["contributes"]["provides"][0]
helpers = Path(os.environ["HELPERS"])
for platform, name in m["programs"].items():
    ext = ".exe" if platform.startswith("windows") else ""
    shutil.copy(helpers / f"{feature}-{platform}{ext}", here / name)
