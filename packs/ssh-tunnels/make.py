"""Puts this pack's programs beside it, from $HELPERS (built there for each platform as
tunnels-PLATFORM, with .exe on Windows)."""
import json
import os
import shutil
import sys
from pathlib import Path

here = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent
m = json.loads((here / "extension.json").read_text(encoding="utf-8"))
helpers = Path(os.environ["HELPERS"])
for platform, name in m["programs"].items():
    ext = ".exe" if platform.startswith("windows") else ""
    shutil.copy(helpers / f"tunnels-{platform}{ext}", here / name)
