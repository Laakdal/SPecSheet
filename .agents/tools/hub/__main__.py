"""Run as: python .agents/tools/hub <command>   (Python 3.10+, standard library only)"""
import sys
from pathlib import Path

if sys.version_info < (3, 10):
    sys.exit("hub needs Python 3.10 or newer")

sys.dont_write_bytecode = True  # keep the project tree clean of __pycache__
_here = str(Path(__file__).resolve().parent)
if _here not in sys.path:
    sys.path.insert(0, _here)

import cli  # noqa: E402

sys.exit(cli.main())
