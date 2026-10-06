"""Collect locked dependency license notices for the native binary package."""
import json
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=root,
        text=True,
    )
)
sections = [
    "Third-party dependency notices for Glicol VST (VST3)\n"
    "The original plugin's MIT license is provided separately as LICENSE.\n"
    "This inventory includes build dependencies and platform-specific dependencies.\n"
]
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if package["name"] == "glicol_vst3":
        continue
    directory = Path(package["manifest_path"]).parent
    sections.append(
        "\n" + "=" * 72 + "\n"
        f'{package["name"]} {package["version"]}\n'
        f'License: {package.get("license") or "see supplied license files"}\n'
        f'Repository: {package.get("repository") or "not specified"}\n'
    )
    candidates = []
    for parent in [directory, *list(directory.parents)[:2]]:
        for file in parent.iterdir():
            name = file.name.lower()
            if file.is_file() and name.startswith(("license", "licence", "copying", "notice")):
                candidates.append(file)
        if candidates:
            break
    license_file = package.get("license_file")
    if license_file:
        candidates.append(directory / license_file)
    for file in sorted(set(candidates)):
        if file.exists():
            sections.append(f"\n--- {file.name} ---\n{file.read_text(errors='replace')}\n")
(root / "THIRD-PARTY-LICENSES.txt").write_text("\n".join(sections), encoding="utf-8")
