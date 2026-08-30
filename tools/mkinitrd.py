"""Empacota arquivos no formato initramfs do Nexo OS (NEXOIRD1). Ver kernel/lib/initrd.

Uso: mkinitrd.py SAIDA nome=caminho [nome=caminho ...]
"""
import struct
import sys
from pathlib import Path

MAGIC = b"NEXOIRD1"
HEADER_SIZE = 16
ENTRY_SIZE = 48
NAME_MAX = 32


def pack(members: list, out: Path) -> int:
    table = b""
    blob = b""
    offset = HEADER_SIZE + len(members) * ENTRY_SIZE
    for name, path in members:
        data = Path(path).read_bytes()
        n = name.encode("utf-8")
        if len(n) >= NAME_MAX:
            raise SystemExit(f"nome longo demais: {name}")
        table += n.ljust(NAME_MAX, b"\0") + struct.pack("<QQ", offset, len(data))
        blob += data
        offset += len(data)
    out.write_bytes(MAGIC + struct.pack("<II", len(members), 0) + table + blob)
    return len(members)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    members = [tuple(a.split("=", 1)) for a in sys.argv[2:]]
    n = pack(members, Path(sys.argv[1]))
    print(f"initrd: {n} membros em {sys.argv[1]}")
