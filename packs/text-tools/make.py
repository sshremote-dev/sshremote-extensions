"""Builds the Text tools script into text_tools.wasm beside this file (needs the
wasm32-unknown-unknown Rust target)."""
import shutil
import subprocess
import sys
from pathlib import Path

here = Path(__file__).resolve().parent
out = Path(sys.argv[1]) if len(sys.argv) > 1 else here
subprocess.run(["cargo", "build", "--release", "--target", "wasm32-unknown-unknown",
                "--manifest-path", str(here / "Cargo.toml")], check=True)
shutil.copy(here / "target" / "wasm32-unknown-unknown" / "release" / "text_tools.wasm", out / "text_tools.wasm")
