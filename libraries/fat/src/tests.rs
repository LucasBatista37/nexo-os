extern crate std;
use super::*;
use std::string::ToString;
use std::vec::Vec;

struct MemDev(Vec<u8>);
impl SectorDevice for MemDev {
    fn sector_count(&self) -> u64 {
        (self.0.len() / SECTOR) as u64
    }
    fn read_sector(&mut self, lba: u64, buf: &mut [u8; SECTOR]) -> Result<(), IoError> {
        let o = lba as usize * SECTOR;
        if o + SECTOR > self.0.len() {
            return Err(IoError);
        }
        buf.copy_from_slice(&self.0[o..o + SECTOR]);
        Ok(())
    }
}

/// Imagem FAT12 minima construida a mao: 1 setor/cluster, 1 FAT de 1 setor, raiz de 16 entradas,
/// arquivo `HELLO.TXT` (2 clusters: 3,4) e diretorio `DIR` (cluster 5) com `A.BIN` (cluster 6).
fn fat12_image() -> Vec<u8> {
    let total = 64u16;
    let mut img = std::vec![0u8; total as usize * SECTOR];
    let b = &mut img[0..SECTOR];
    b[0..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
    b[3..11].copy_from_slice(b"NEXOTEST");
    b[11..13].copy_from_slice(&512u16.to_le_bytes());
    b[13] = 1; // setores por cluster
    b[14..16].copy_from_slice(&1u16.to_le_bytes()); // reservados
    b[16] = 1; // FATs
    b[17..19].copy_from_slice(&16u16.to_le_bytes()); // entradas da raiz (1 setor)
    b[19..21].copy_from_slice(&total.to_le_bytes());
    b[21] = 0xf8;
    b[22..24].copy_from_slice(&1u16.to_le_bytes()); // setores por FAT
    b[510] = 0x55;
    b[511] = 0xaa;
    // FAT12 em LBA 1: entradas de 12 bits
    let mut fat = std::vec![0u16; 16];
    fat[0] = 0xff8;
    fat[1] = 0xfff;
    fat[3] = 4;
    fat[4] = 0xfff;
    fat[5] = 0xfff;
    fat[6] = 0xfff;
    let f = &mut img[SECTOR..2 * SECTOR];
    for (i, &v) in fat.iter().enumerate() {
        let off = i * 3 / 2;
        if i % 2 == 0 {
            f[off] = (v & 0xff) as u8;
            f[off + 1] = (f[off + 1] & 0xf0) | ((v >> 8) as u8 & 0x0f);
        } else {
            f[off] = (f[off] & 0x0f) | (((v & 0x0f) as u8) << 4);
            f[off + 1] = (v >> 4) as u8;
        }
    }
    // raiz em LBA 2; dados em LBA 3 (cluster 2)
    fn entry(dst: &mut [u8], name: &[u8; 11], attr: u8, cluster: u16, size: u32, nt: u8) {
        dst[0..11].copy_from_slice(name);
        dst[11] = attr;
        dst[12] = nt;
        dst[26..28].copy_from_slice(&cluster.to_le_bytes());
        dst[28..32].copy_from_slice(&size.to_le_bytes());
    }
    let root = 2 * SECTOR;
    entry(
        &mut img[root..root + 32],
        b"HELLO   TXT",
        0x20,
        3,
        700,
        0x18,
    ); // hello.txt (caixa NT)
    entry(
        &mut img[root + 32..root + 64],
        b"DIR        ",
        0x10,
        5,
        0,
        0,
    );
    // LFN para "LongName Example.dat" com entrada curta LONGNA~1DAT (cluster 6 reaproveitado como vazio)
    let long = "LongName Example.dat";
    let chars: Vec<u16> = long.encode_utf16().collect();
    let n_entries = chars.len().div_ceil(13);
    let mut pos = root + 64;
    for seq in (1..=n_entries).rev() {
        let e = &mut img[pos..pos + 32];
        e[0] = seq as u8 | if seq == n_entries { 0x40 } else { 0 };
        e[11] = 0x0f;
        let positions = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
        for (k, &p) in positions.iter().enumerate() {
            let idx = (seq - 1) * 13 + k;
            let ch = if idx < chars.len() {
                chars[idx]
            } else if idx == chars.len() {
                0
            } else {
                0xffff
            };
            e[p..p + 2].copy_from_slice(&ch.to_le_bytes());
        }
        pos += 32;
    }
    entry(&mut img[pos..pos + 32], b"LONGNA~1DAT", 0x20, 6, 5, 0);
    // dados: cluster 3 (LBA 4) e 4 (LBA 5) = hello.txt; cluster 5 (LBA 6) = DIR; cluster 6 (LBA 7) = A.BIN/dat
    let data = |c: usize| (c - 2 + 3) * SECTOR;
    for i in 0..700usize {
        let (c, off) = if i < 512 { (3, i) } else { (4, i - 512) };
        img[data(c) + off] = (i % 251) as u8;
    }
    let d = data(5);
    entry(&mut img[d..d + 32], b".          ", 0x10, 5, 0, 0);
    entry(&mut img[d + 32..d + 64], b"..         ", 0x10, 0, 0, 0);
    entry(&mut img[d + 64..d + 96], b"A       BIN", 0x20, 6, 5, 0);
    img[data(6)..data(6) + 5].copy_from_slice(b"abcde");
    img
}

#[test]
fn fat12_hand_built() {
    let mut fs = Fat::mount(MemDev(fat12_image()), 0).unwrap();
    assert_eq!(fs.kind(), FatKind::Fat12);
    let mut names = Vec::new();
    fs.for_each_entry(0, |e| {
        names.push((
            std::string::String::from_utf8_lossy(e.name()).into_owned(),
            e.is_dir(),
            e.size,
        ));
        true
    })
    .unwrap();
    assert_eq!(
        names,
        std::vec![
            ("hello.txt".into(), false, 700),
            ("DIR".into(), true, 0),
            ("LongName Example.dat".into(), false, 5)
        ]
    );
    let f = fs.lookup(b"/HELLO.TXT").unwrap();
    let mut buf = std::vec![0u8; 1000];
    assert_eq!(fs.read(&f, 0, &mut buf).unwrap(), 700);
    assert!(
        buf[..700]
            .iter()
            .enumerate()
            .all(|(i, &b)| b == (i % 251) as u8)
    );
    let mut part = [0u8; 100];
    assert_eq!(fs.read(&f, 480, &mut part).unwrap(), 100);
    assert!(
        part.iter()
            .enumerate()
            .all(|(i, &b)| b == ((i + 480) % 251) as u8)
    );
    assert_eq!(fs.read(&f, 700, &mut part).unwrap(), 0);
    let a = fs.lookup(b"dir/a.bin").unwrap();
    let mut s = [0u8; 8];
    assert_eq!(fs.read(&a, 0, &mut s).unwrap(), 5);
    assert_eq!(&s[..5], b"abcde");
    let l = fs.lookup(b"longname example.DAT").unwrap();
    assert_eq!(fs.read(&l, 0, &mut s).unwrap(), 5);
    assert_eq!(fs.lookup(b"nada").map(|_| ()), Err(FatError::NotFound));
    assert_eq!(fs.lookup(b"hello.txt/x").map(|_| ()), Err(FatError::NotDir));
    let dir = fs.lookup(b"dir").unwrap();
    assert_eq!(fs.read(&dir, 0, &mut s), Err(FatError::IsDir));
}

#[test]
fn corrupted_images_never_panic() {
    let image = fat12_image();
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..500 {
        let mut d = image.clone();
        for _ in 0..1 + next() % 6 {
            let i = (next() % (8 * SECTOR) as u64) as usize;
            d[i] ^= (next() % 255 + 1) as u8;
        }
        if let Ok(mut fs) = Fat::mount(MemDev(d), 0) {
            let _ = fs.for_each_entry(0, |_| true);
            if let Ok(f) = fs.lookup(b"hello.txt") {
                let mut buf = [0u8; 1024];
                let _ = fs.read(&f, 0, &mut buf);
            }
            let _ = fs.lookup(b"dir/a.bin");
        }
    }
}

/// GPT sintética: cabeçalho em LBA 1, entradas em LBA 2, ESP em [2048, 4095].
#[test]
fn gpt_esp_lookup() {
    let mut img = std::vec![0u8; 4096 * SECTOR];
    let h = &mut img[SECTOR..2 * SECTOR];
    h[0..8].copy_from_slice(b"EFI PART");
    h[72..80].copy_from_slice(&2u64.to_le_bytes());
    h[80..84].copy_from_slice(&128u32.to_le_bytes());
    h[84..88].copy_from_slice(&128u32.to_le_bytes());
    let e = &mut img[2 * SECTOR + 128..2 * SECTOR + 256]; // 2a entrada
    e[0..16].copy_from_slice(&ESP_GUID);
    e[32..40].copy_from_slice(&2048u64.to_le_bytes());
    e[40..48].copy_from_slice(&4095u64.to_le_bytes());
    let mut dev = MemDev(img);
    assert_eq!(
        find_esp(&mut dev),
        Ok(Partition {
            first_lba: 2048,
            last_lba: 4095
        })
    );
    dev.0[SECTOR] = b'X';
    assert!(matches!(find_esp(&mut dev), Err(FatError::Corrupted(_))));
}

/// Se o mtools estiver instalado, monta a imagem FAT32 gerada como o ESP real (mformat/mcopy).
#[test]
fn fat32_from_mtools_if_available() {
    use std::process::Command;
    if Command::new("mformat").arg("-h").output().is_err() {
        std::eprintln!("mformat ausente: teste FAT32 real pulado");
        return;
    }
    let dir = std::env::temp_dir().join(std::format!("nexo-fat-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let img = dir.join("esp.img");
    let sectors: u64 = 66 * 1024 * 1024 / 512; // 66 MiB: mformat -F exige >= ~33 MiB para FAT32
    std::fs::File::create(&img)
        .unwrap()
        .set_len(sectors * 512)
        .unwrap();
    let ok = Command::new("mformat")
        .args([
            "-i",
            img.to_str().unwrap(),
            "-C",
            "-F",
            "-T",
            &sectors.to_string(),
            "-h",
            "64",
            "-s",
            "32",
            "::",
        ])
        .status()
        .unwrap()
        .success();
    assert!(ok, "mformat falhou");
    assert!(
        Command::new("mmd")
            .args([
                "-i",
                img.to_str().unwrap(),
                "::/EFI",
                "::/EFI/BOOT",
                "::/nexo"
            ])
            .status()
            .unwrap()
            .success()
    );
    let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 253) as u8).collect();
    let src = dir.join("kernel.elf");
    std::fs::write(&src, &payload).unwrap();
    assert!(
        Command::new("mcopy")
            .args([
                "-i",
                img.to_str().unwrap(),
                src.to_str().unwrap(),
                "::/nexo/kernel.elf"
            ])
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(dir.join("BOOTX64.EFI"), b"MZ-fake").unwrap();
    assert!(
        Command::new("mcopy")
            .args([
                "-i",
                img.to_str().unwrap(),
                dir.join("BOOTX64.EFI").to_str().unwrap(),
                "::/EFI/BOOT/BOOTX64.EFI"
            ])
            .status()
            .unwrap()
            .success()
    );
    let data = std::fs::read(&img).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let mut fs = Fat::mount(MemDev(data), 0).unwrap();
    assert_eq!(fs.kind(), FatKind::Fat32);
    let mut names = Vec::new();
    fs.for_each_entry(0, |e| {
        names.push(std::string::String::from_utf8_lossy(e.name()).to_ascii_lowercase());
        true
    })
    .unwrap();
    names.sort();
    assert_eq!(names, std::vec!["efi", "nexo"]);
    let k = fs.lookup(b"/nexo/kernel.elf").unwrap();
    assert_eq!(k.size as usize, payload.len());
    let mut out = std::vec![0u8; payload.len()];
    assert_eq!(fs.read(&k, 0, &mut out).unwrap(), payload.len());
    assert_eq!(out, payload);
    let b = fs.lookup(b"EFI\\BOOT\\bootx64.efi").unwrap();
    let mut m = [0u8; 16];
    assert_eq!(fs.read(&b, 0, &mut m).unwrap(), 7);
    assert_eq!(&m[..7], b"MZ-fake");
}
