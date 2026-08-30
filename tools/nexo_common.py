"""Utilitários compartilhados pelas ferramentas do Nexo OS (Python 3.9+, sem dependências)."""
import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD = ROOT / "build"
LOADER_DIR = ROOT / "boot" / "loader"
KERNEL_DIR = ROOT / "kernel"
LOADER_EFI = LOADER_DIR / "target" / "x86_64-unknown-uefi" / "release" / "nexo-loader.efi"
KERNEL_ELF = KERNEL_DIR / "target" / "x86_64-unknown-none" / "release" / "nexo-kernel"

OVMF_CANDIDATES = [
    os.environ.get("NEXO_OVMF", ""),
    "/opt/homebrew/share/qemu/edk2-x86_64-code.fd",
    "/usr/local/share/qemu/edk2-x86_64-code.fd",
    "/usr/share/qemu/edk2-x86_64-code.fd",
    "/usr/share/OVMF/OVMF_CODE_4M.fd",
    "/usr/share/OVMF/OVMF_CODE.fd",
    "/usr/share/edk2/x64/OVMF_CODE.4m.fd",
    "/usr/share/edk2/ovmf/OVMF_CODE.fd",
    "/usr/share/ovmf/OVMF.fd",
]


def log(msg: str) -> None:
    print(f"[nexo] {msg}", flush=True)


def die(msg: str, code: int = 1):
    print(f"[nexo] ERRO: {msg}", file=sys.stderr, flush=True)
    sys.exit(code)


def cargo_env() -> dict:
    env = dict(os.environ)
    cargo_bin = Path.home() / ".cargo" / "bin"
    if cargo_bin.exists():
        env["PATH"] = f"{cargo_bin}{os.pathsep}{env.get('PATH', '')}"
    # Data fixa para timestamps FAT: imagem determinística.
    env.setdefault("SOURCE_DATE_EPOCH", "1756425600")
    return env


def run(cmd, cwd=None, check=True, extra_env=None, **kw) -> subprocess.CompletedProcess:
    log("$ " + " ".join(str(c) for c in cmd) + (f"  (em {cwd})" if cwd else ""))
    env = cargo_env()
    if extra_env:
        env.update(extra_env)
    return subprocess.run([str(c) for c in cmd], cwd=cwd, check=check, env=env, **kw)


def require(tool: str, hint: str) -> str:
    path = shutil.which(tool, path=cargo_env()["PATH"])
    if not path:
        die(f"'{tool}' nao encontrado. {hint}")
    return path


def find_ovmf() -> Path:
    for c in OVMF_CANDIDATES:
        if c and Path(c).is_file():
            return Path(c)
    die("firmware UEFI (OVMF/edk2) nao encontrado; defina NEXO_OVMF=/caminho/edk2-x86_64-code.fd "
        "(macOS: brew install qemu; Debian/Ubuntu: apt install ovmf)")


def llvm_tool(name: str) -> str:
    """Localiza um binário do componente llvm-tools do rustup."""
    env = cargo_env()
    sysroot = subprocess.run(["rustc", "--print", "sysroot"], env=env, capture_output=True, text=True, check=True).stdout.strip()
    host = subprocess.run(["rustc", "-vV"], env=env, capture_output=True, text=True, check=True).stdout
    triple = next(l.split(":", 1)[1].strip() for l in host.splitlines() if l.startswith("host:"))
    cand = Path(sysroot) / "lib" / "rustlib" / triple / "bin" / name
    if cand.exists():
        return str(cand)
    found = shutil.which(name)
    if found:
        return found
    die(f"{name} nao encontrado; instale o componente llvm-tools (rustup component add llvm-tools)")


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()
