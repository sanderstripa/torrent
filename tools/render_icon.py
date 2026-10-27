"""Render the approved native SVG family; requires Node and development dependency sharp."""
import subprocess
from pathlib import Path
subprocess.run(['node', str(Path(__file__).resolve().parents[1]/'scripts/render-app-icons.cjs')], check=True)
