"""Puts this pack's programs beside it (see registry/helper_pack.py)."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
import helper_pack  # noqa: E402

helper_pack.main(Path(__file__).resolve().parent)
