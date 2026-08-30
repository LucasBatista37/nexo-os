"""Gera uma imagem de disco GPT determinística contendo uma única partição EFI System.

Sem dependências externas: escreve MBR protetor, cabeçalhos GPT primário e
secundário, tabela de partições e copia a imagem FAT fornecida.
"""
import struct
import uuid
import zlib
from pathlib import Path

SECTOR = 512
ESP_TYPE_GUID = uuid.UUID("C12A7328-F81F-11D2-BA4B-00A0C93EC93B")
# GUIDs fixos para reprodutibilidade (derivados de nomes no namespace DNS).
DISK_GUID = uuid.uuid5(uuid.NAMESPACE_DNS, "disk.nexo-os.local")
PART_GUID = uuid.uuid5(uuid.NAMESPACE_DNS, "esp.nexo-os.local")


def _crc(b: bytes) -> int:
    return zlib.crc32(b) & 0xFFFFFFFF


def build_gpt_image(esp_image: Path, out: Path, first_lba: int = 2048) -> None:
    esp = esp_image.read_bytes()
    esp_sectors = (len(esp) + SECTOR - 1) // SECTOR
    last_part_lba = first_lba + esp_sectors - 1
    total_sectors = last_part_lba + 1 + 33  # 32 setores de tabela + 1 cabeçalho secundário
    backup_lba = total_sectors - 1

    entry = struct.pack(
        "<16s16sQQQ72s",
        ESP_TYPE_GUID.bytes_le,
        PART_GUID.bytes_le,
        first_lba,
        last_part_lba,
        0,
        "NEXO-ESP".encode("utf-16-le").ljust(72, b"\0"),
    )
    entries = entry + b"\0" * (128 * 128 - len(entry))
    entries_crc = _crc(entries)

    def header(my_lba: int, alt_lba: int, entries_lba: int) -> bytes:
        h = struct.pack(
            "<8sIIIIQQQQ16sQIII",
            b"EFI PART", 0x00010000, 92, 0, 0,
            my_lba, alt_lba, 34, backup_lba - 33,
            DISK_GUID.bytes_le, entries_lba, 128, 128, entries_crc,
        )
        h = h[:16] + struct.pack("<I", _crc(h)) + h[20:]
        return h.ljust(SECTOR, b"\0")

    mbr = bytearray(SECTOR)
    mbr[446:462] = struct.pack("<BBBBBBBBII", 0x00, 0x00, 0x02, 0x00, 0xEE, 0xFF, 0xFF, 0xFF, 1,
                               min(total_sectors - 1, 0xFFFFFFFF))
    mbr[510:512] = b"\x55\xAA"

    with open(out, "wb") as f:
        f.write(mbr)                                   # LBA 0
        f.write(header(1, backup_lba, 2))              # LBA 1
        f.write(entries)                               # LBA 2..33
        f.write(b"\0" * ((first_lba - 34) * SECTOR))   # até LBA 2048
        f.write(esp)
        f.write(b"\0" * (esp_sectors * SECTOR - len(esp)))
        f.write(entries)                               # tabela secundária (backup_lba-32..backup_lba-1)
        f.write(header(backup_lba, 1, backup_lba - 32))
    assert out.stat().st_size == total_sectors * SECTOR, "tamanho da imagem inconsistente"
