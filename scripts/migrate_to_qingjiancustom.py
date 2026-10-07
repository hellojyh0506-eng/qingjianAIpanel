import os
import shutil
import hashlib
from pathlib import Path

DEST_ROOT = Path(r"E:\qingjiancustom")
SOURCE_APP = Path(r"E:\Qingjian")
APPDATA_DIR = Path(os.environ["APPDATA"]) / "Qingjian"
SCRATCH_DIR = Path(r"C:\Users\Admin\.gemini\antigravity\scratch\qingjian")

print(f"Starting migration to {DEST_ROOT}...")

# 1. Ensure DEST_ROOT exists
DEST_ROOT.mkdir(parents=True, exist_ok=True)

# 2. Copy installed application files, excluding backups and temp logs
IGNORED_SUFFIXES = {".bak", ".tmp"}
IGNORED_NAMES = {
    "server_err.txt", "server_out.txt", "dict.qj.bak_20261007_160050",
    "qingjian-server.exe.bak", "qingjian-server.exe.bak_20261007_160050",
    "qingjian-server.exe.bak_pre_deploy", "qingjian_tsf-0.1.4.dll.bak",
    "qingjian_tsf-0.1.4.dll.bak_20261007_160050", "qingjian_tsf-0.1.4.dll.bak_pre_deploy"
}

def should_copy_app_file(src_path: Path) -> bool:
    name = src_path.name
    if name in IGNORED_NAMES:
        return False
    if any(name.endswith(suffix) for suffix in IGNORED_SUFFIXES):
        return False
    if ".bak" in name or ".tmp" in name:
        return False
    return True

print("Copying base application files...")
for item in SOURCE_APP.iterdir():
    if not should_copy_app_file(item):
        continue
    target = DEST_ROOT / item.name
    if item.is_dir():
        if item.name == "data":
            # Handle data directory with clean filtering
            target.mkdir(parents=True, exist_ok=True)
            for sub_root, dirs, files in os.walk(item):
                rel_dir = Path(sub_root).relative_to(item)
                (target / rel_dir).mkdir(parents=True, exist_ok=True)
                for file_name in files:
                    file_src = Path(sub_root) / file_name
                    if should_copy_app_file(file_src):
                        shutil.copy2(file_src, target / rel_dir / file_name)
        else:
            if not target.exists():
                shutil.copytree(item, target)
    else:
        shutil.copy2(item, target)

# 3. Copy user personalization and lexicon data to user_data/
user_data_dest = DEST_ROOT / "user_data"
user_data_dest.mkdir(parents=True, exist_ok=True)

print("Copying user lexicons and configuration from %APPDATA%\\Qingjian...")
if APPDATA_DIR.exists():
    for item in APPDATA_DIR.iterdir():
        if item.name.startswith(".user.tsv.tmp") or item.name.startswith(".usage.tsv.tmp"):
            continue
        target = user_data_dest / item.name
        if item.is_dir():
            if not target.exists():
                shutil.copytree(item, target)
        else:
            shutil.copy2(item, target)

# 4. Copy source lexicons to source_lexicon/
src_lexicon_dest = DEST_ROOT / "source_lexicon"
src_lexicon_dest.mkdir(parents=True, exist_ok=True)

print("Copying source lexicon and TSVs...")
dict_tsv = SCRATCH_DIR / "assets" / "lexicon" / "dict.tsv"
if dict_tsv.exists():
    shutil.copy2(dict_tsv, src_lexicon_dest / "dict.tsv")

sample_dict_tsv = SCRATCH_DIR / "assets" / "sample" / "dict.tsv"
if sample_dict_tsv.exists():
    shutil.copy2(sample_dict_tsv, src_lexicon_dest / "sample_dict.tsv")

glossary_en_tsv = SCRATCH_DIR / "assets" / "sample" / "glossary-en.tsv"
if glossary_en_tsv.exists():
    shutil.copy2(glossary_en_tsv, src_lexicon_dest / "glossary-en.tsv")

# 5. Create launcher scripts in E:\qingjiancustom
start_bat = DEST_ROOT / "start_server.bat"
start_bat.write_text(
    "@echo off\r\n"
    "echo Starting Qingjian Custom Server daemon...\r\n"
    "cd /d \"%~dp0\"\r\n"
    "start \"\" \"%~dp0qingjian-server.exe\"\r\n"
    "echo Done.\r\n",
    encoding="utf-8"
)

stop_bat = DEST_ROOT / "stop_server.bat"
stop_bat.write_text(
    "@echo off\r\n"
    "echo Stopping Qingjian Server...\r\n"
    "taskkill /F /IM qingjian-server.exe\r\n"
    "echo Done.\r\n",
    encoding="utf-8"
)

# 6. Create register_tsf_custom.ps1 script
register_ps1 = DEST_ROOT / "register_tsf_custom.ps1"
register_ps1.write_text(
    "# Qingjian Custom TSF Registration Script\r\n"
    "# Run this script as Administrator ONLY if you wish to point the system TSF registration to E:\\qingjiancustom\r\n"
    "# Default installation remains at E:\\Qingjian\r\n"
    "$CustomDir = Split-Path -Parent $MyInvocation.MyCommand.Path\r\n"
    "$DllPath = Join-Path $CustomDir 'qingjian_tsf-0.1.4.dll'\r\n"
    "Write-Host \"Registering Custom TSF DLL: $DllPath\" -ForegroundColor Cyan\r\n"
    "if (Test-Path $DllPath) {\r\n"
    "    regsvr32.exe /s \"$DllPath\"\r\n"
    "    Write-Host \"Successfully registered $DllPath\" -ForegroundColor Green\r\n"
    "} else {\r\n"
    "    Write-Host \"Error: $DllPath not found!\" -ForegroundColor Red\r\n"
    "}\r\n",
    encoding="utf-8"
)

# 7. Generate SHA256 checksum manifest
print("Calculating SHA256 manifest...")
manifest_lines = []
for root, _, files in os.walk(DEST_ROOT):
    for f in sorted(files):
        if f == "SHA256SUMS.txt":
            continue
        full_p = Path(root) / f
        rel_p = full_p.relative_to(DEST_ROOT)
        h = hashlib.sha256()
        with open(full_p, "rb") as fp:
            while chunk := fp.read(65536):
                h.update(chunk)
        manifest_lines.append(f"{h.hexdigest()}  {rel_p.as_posix()}")

sha_file = DEST_ROOT / "SHA256SUMS.txt"
sha_file.write_text("\n".join(manifest_lines) + "\n", encoding="utf-8")

print(f"Migration completed successfully! Total files indexed: {len(manifest_lines)}")
