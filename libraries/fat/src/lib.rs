//! Leitor **somente leitura** de FAT12/16/32 (§Fase 3: "FAT somente para EFI") e localização da
//! partição de sistema EFI numa tabela GPT. Sem alocação: o chamador fornece o dispositivo de
//! setores e buffers; nomes longos (VFAT) são montados em um buffer de 255 bytes (ASCII; outros
//! caracteres viram `?`).
#![no_std]
#![forbid(unsafe_code)]

/// Tamanho do setor.
pub const SECTOR: usize = 512;
/// GUID da partição de sistema EFI (C12A7328-F81F-11D2-BA4B-00A0C93EC93B), em bytes mistos.
pub const ESP_GUID: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
/// Tamanho máximo de nome longo.
pub const NAME_MAX: usize = 255;
/// Atributo: diretório.
pub const ATTR_DIR: u8 = 0x10;

/// Erro de E/S.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoError;

/// Dispositivo de setores de 512 B.
pub trait SectorDevice {
    /// Número de setores.
    fn sector_count(&self) -> u64;
    /// Lê um setor.
    fn read_sector(&mut self, lba: u64, buf: &mut [u8; SECTOR]) -> Result<(), IoError>;
}

/// Erros do leitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatError {
    /// E/S.
    Io,
    /// Estrutura inválida.
    Corrupted(&'static str),
    /// Não encontrado.
    NotFound,
    /// Componente não é diretório.
    NotDir,
    /// É diretório.
    IsDir,
    /// GPT sem partição EFI.
    NoEsp,
}

impl From<IoError> for FatError {
    fn from(_: IoError) -> Self {
        FatError::Io
    }
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}

/// Partição encontrada na GPT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Partition {
    /// Primeiro LBA.
    pub first_lba: u64,
    /// Último LBA (inclusivo).
    pub last_lba: u64,
}

/// Localiza a partição de sistema EFI (primeira entrada com o GUID de tipo ESP).
pub fn find_esp(dev: &mut impl SectorDevice) -> Result<Partition, FatError> {
    let mut s = [0u8; SECTOR];
    dev.read_sector(1, &mut s)?;
    if &s[0..8] != b"EFI PART" {
        return Err(FatError::Corrupted("assinatura GPT"));
    }
    let entries_lba = u64_at(&s, 72);
    let count = u32_at(&s, 80) as u64;
    let esize = u32_at(&s, 84) as u64;
    if esize < 128
        || esize > SECTOR as u64
        || !(SECTOR as u64).is_multiple_of(esize)
        || count > 1024
    {
        return Err(FatError::Corrupted("entradas GPT"));
    }
    let per_sector = SECTOR as u64 / esize;
    let mut e = [0u8; SECTOR];
    for i in 0..count {
        if i.is_multiple_of(per_sector) {
            dev.read_sector(entries_lba + i / per_sector, &mut e)?;
        }
        let off = ((i % per_sector) * esize) as usize;
        if e[off..off + 16] == ESP_GUID {
            let (first, last) = (u64_at(&e, off + 32), u64_at(&e, off + 40));
            if first == 0 || last < first || last >= dev.sector_count() {
                return Err(FatError::Corrupted("faixa da particao"));
            }
            return Ok(Partition {
                first_lba: first,
                last_lba: last,
            });
        }
    }
    Err(FatError::NoEsp)
}

/// Tipo de FAT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatKind {
    /// FAT12.
    Fat12,
    /// FAT16.
    Fat16,
    /// FAT32.
    Fat32,
}

/// Entrada de diretório decodificada.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    /// Nome (longo se houver, senão 8.3 com ponto).
    name: [u8; NAME_MAX],
    len: u8,
    /// Atributos.
    pub attr: u8,
    /// Primeiro cluster.
    pub cluster: u32,
    /// Tamanho em bytes (0 para diretórios).
    pub size: u32,
}

impl Entry {
    /// Nome.
    pub fn name(&self) -> &[u8] {
        &self.name[..self.len as usize]
    }
    /// `true` se diretório.
    pub fn is_dir(&self) -> bool {
        self.attr & ATTR_DIR != 0
    }
}

/// Volume FAT montado (somente leitura).
pub struct Fat<D: SectorDevice> {
    dev: D,
    base: u64,
    kind: FatKind,
    sectors_per_cluster: u32,
    fat_start: u64,
    fat_sectors: u32,
    root_dir_start: u64,
    root_dir_sectors: u32,
    data_start: u64,
    total_clusters: u32,
    root_cluster: u32,
}

impl<D: SectorDevice> Fat<D> {
    /// Monta o volume que começa no setor `base`.
    pub fn mount(mut dev: D, base: u64) -> Result<Self, FatError> {
        let mut s = [0u8; SECTOR];
        dev.read_sector(base, &mut s)?;
        if s[510] != 0x55 || s[511] != 0xaa {
            return Err(FatError::Corrupted("assinatura do setor de boot"));
        }
        let bytes_per_sector = u16_at(&s, 11) as u32;
        let sectors_per_cluster = s[13] as u32;
        let reserved = u16_at(&s, 14) as u32;
        let fats = s[16] as u32;
        let root_entries = u16_at(&s, 17) as u32;
        let total16 = u16_at(&s, 19) as u32;
        let fat16_size = u16_at(&s, 22) as u32;
        let total32 = u32_at(&s, 32);
        let fat32_size = u32_at(&s, 36);
        if bytes_per_sector != SECTOR as u32
            || sectors_per_cluster == 0
            || !sectors_per_cluster.is_power_of_two()
            || reserved == 0
            || fats == 0
        {
            return Err(FatError::Corrupted("BPB"));
        }
        let fat_sectors = if fat16_size != 0 {
            fat16_size
        } else {
            fat32_size
        };
        let total = if total16 != 0 { total16 } else { total32 };
        if fat_sectors == 0 || total == 0 {
            return Err(FatError::Corrupted("tamanhos do BPB"));
        }
        let root_dir_sectors = (root_entries * 32).div_ceil(SECTOR as u32);
        let data_sectors = total
            .checked_sub(reserved + fats * fat_sectors + root_dir_sectors)
            .ok_or(FatError::Corrupted("geometria"))?;
        let total_clusters = data_sectors / sectors_per_cluster;
        let kind = if fat16_size == 0 && root_entries == 0 {
            FatKind::Fat32
        } else if total_clusters < 4085 {
            FatKind::Fat12
        } else {
            FatKind::Fat16
        };
        let root_cluster = if kind == FatKind::Fat32 {
            u32_at(&s, 44)
        } else {
            0
        };
        let fat_start = base + reserved as u64;
        let root_dir_start = fat_start + (fats * fat_sectors) as u64;
        let data_start = root_dir_start + root_dir_sectors as u64;
        if data_start + (total_clusters * sectors_per_cluster) as u64 > dev.sector_count() {
            return Err(FatError::Corrupted("volume maior que o dispositivo"));
        }
        Ok(Fat {
            dev,
            base,
            kind,
            sectors_per_cluster,
            fat_start,
            fat_sectors,
            root_dir_start,
            root_dir_sectors,
            data_start,
            total_clusters,
            root_cluster,
        })
    }

    /// Tipo do volume.
    pub fn kind(&self) -> FatKind {
        self.kind
    }
    /// Setor base.
    pub fn base(&self) -> u64 {
        self.base
    }
    /// Bytes por cluster.
    pub fn cluster_bytes(&self) -> usize {
        self.sectors_per_cluster as usize * SECTOR
    }

    /// Devolve o dispositivo.
    pub fn into_device(self) -> D {
        self.dev
    }

    fn cluster_lba(&self, cluster: u32) -> Result<u64, FatError> {
        if cluster < 2 || cluster - 2 >= self.total_clusters {
            return Err(FatError::Corrupted("numero de cluster"));
        }
        Ok(self.data_start + ((cluster - 2) * self.sectors_per_cluster) as u64)
    }

    /// Próximo cluster da cadeia (`None` = fim).
    pub fn next_cluster(&mut self, cluster: u32) -> Result<Option<u32>, FatError> {
        let mut s = [0u8; SECTOR];
        let v = match self.kind {
            FatKind::Fat32 => {
                let off = cluster as u64 * 4;
                let lba = self.fat_start + off / SECTOR as u64;
                if off / SECTOR as u64 >= self.fat_sectors as u64 {
                    return Err(FatError::Corrupted("indice na FAT"));
                }
                self.dev.read_sector(lba, &mut s)?;
                u32_at(&s, (off % SECTOR as u64) as usize) & 0x0fff_ffff
            }
            FatKind::Fat16 => {
                let off = cluster as u64 * 2;
                if off / SECTOR as u64 >= self.fat_sectors as u64 {
                    return Err(FatError::Corrupted("indice na FAT"));
                }
                self.dev
                    .read_sector(self.fat_start + off / SECTOR as u64, &mut s)?;
                u16_at(&s, (off % SECTOR as u64) as usize) as u32
            }
            FatKind::Fat12 => {
                let off = cluster as u64 * 3 / 2;
                if (off + 1) / SECTOR as u64 >= self.fat_sectors as u64 {
                    return Err(FatError::Corrupted("indice na FAT"));
                }
                let lba = self.fat_start + off / SECTOR as u64;
                self.dev.read_sector(lba, &mut s)?;
                let lo = s[(off % SECTOR as u64) as usize];
                let hi = if (off % SECTOR as u64) as usize == SECTOR - 1 {
                    let mut t = [0u8; SECTOR];
                    self.dev.read_sector(lba + 1, &mut t)?;
                    t[0]
                } else {
                    s[(off % SECTOR as u64) as usize + 1]
                };
                let raw = lo as u32 | ((hi as u32) << 8);
                if cluster & 1 == 1 {
                    raw >> 4
                } else {
                    raw & 0xfff
                }
            }
        };
        let end = match self.kind {
            FatKind::Fat32 => v >= 0x0fff_fff8,
            FatKind::Fat16 => v >= 0xfff8,
            FatKind::Fat12 => v >= 0xff8,
        };
        if end {
            return Ok(None);
        }
        if v < 2 {
            return Err(FatError::Corrupted("cadeia de clusters"));
        }
        Ok(Some(v))
    }

    /// Percorre as entradas do diretório (`cluster` 0 = raiz); para quando `f` devolve `false`.
    pub fn for_each_entry(
        &mut self,
        cluster: u32,
        mut f: impl FnMut(&Entry) -> bool,
    ) -> Result<(), FatError> {
        let mut lfn = [0u8; NAME_MAX];
        let mut lfn_len = 0usize;
        let mut lfn_valid = false;
        let mut s = [0u8; SECTOR];
        let mut sectors_seen = 0u32;
        // Sequência de setores do diretório.
        let mut cur = if cluster == 0 && self.kind != FatKind::Fat32 {
            None
        } else {
            Some(if cluster == 0 {
                self.root_cluster
            } else {
                cluster
            })
        };
        let mut fixed_next = if cur.is_none() { Some(0u32) } else { None };
        loop {
            let lba = match (cur, fixed_next) {
                (None, Some(i)) => {
                    if i >= self.root_dir_sectors {
                        return Ok(());
                    }
                    fixed_next = Some(i + 1);
                    self.root_dir_start + i as u64
                }
                (Some(c), _) => {
                    let idx = sectors_seen % self.sectors_per_cluster;
                    let lba = self.cluster_lba(c)? + idx as u64;
                    if idx + 1 == self.sectors_per_cluster {
                        cur = self.next_cluster(c)?;
                        if cur.is_none() {
                            fixed_next = Some(u32::MAX);
                        }
                    }
                    lba
                }
                (None, None) => return Ok(()),
            };
            sectors_seen += 1;
            if sectors_seen > 65536 {
                return Err(FatError::Corrupted("diretorio sem fim"));
            }
            self.dev.read_sector(lba, &mut s)?;
            for i in 0..SECTOR / 32 {
                let e = &s[i * 32..(i + 1) * 32];
                match e[0] {
                    0x00 => return Ok(()),
                    0xe5 => {
                        lfn_valid = false;
                        continue;
                    }
                    _ => {}
                }
                let attr = e[11];
                if attr & 0x3f == 0x0f {
                    // Entrada de nome longo: sequência (bit 6 = última), 13 caracteres UTF-16.
                    let seq = (e[0] & 0x1f) as usize;
                    if seq == 0 || seq > 20 {
                        lfn_valid = false;
                        continue;
                    }
                    if e[0] & 0x40 != 0 {
                        lfn = [0; NAME_MAX];
                        lfn_len = 0;
                        lfn_valid = true;
                    }
                    if !lfn_valid {
                        continue;
                    }
                    let base = (seq - 1) * 13;
                    let positions = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
                    for (k, &p) in positions.iter().enumerate() {
                        let ch = u16_at(e, p);
                        let idx = base + k;
                        if idx >= NAME_MAX {
                            break;
                        }
                        if ch == 0 || ch == 0xffff {
                            continue;
                        }
                        lfn[idx] = if ch < 0x80 { ch as u8 } else { b'?' };
                        if idx + 1 > lfn_len {
                            lfn_len = idx + 1;
                        }
                    }
                    continue;
                }
                if attr & 0x08 != 0 {
                    lfn_valid = false;
                    continue; // rótulo do volume
                }
                let mut entry = Entry {
                    name: [0; NAME_MAX],
                    len: 0,
                    attr,
                    cluster: u16_at(e, 26) as u32 | ((u16_at(e, 20) as u32) << 16),
                    size: u32_at(e, 28),
                };
                if self.kind != FatKind::Fat32 {
                    entry.cluster &= 0xffff;
                }
                if lfn_valid && lfn_len > 0 {
                    entry.name[..lfn_len].copy_from_slice(&lfn[..lfn_len]);
                    entry.len = lfn_len as u8;
                } else {
                    // 8.3 com bits de caixa NT (0x08 = base minúscula, 0x10 = extensão minúscula).
                    let nt = e[12];
                    let mut n = 0;
                    for &c in e[0..8].iter().take_while(|&&c| c != b' ') {
                        let c = if c == 0x05 { 0xe5 } else { c };
                        entry.name[n] = if nt & 0x08 != 0 {
                            c.to_ascii_lowercase()
                        } else {
                            c
                        };
                        n += 1;
                    }
                    let ext: &[u8] = &e[8..11];
                    let ext_len = ext.iter().take_while(|&&c| c != b' ').count();
                    if ext_len > 0 {
                        entry.name[n] = b'.';
                        n += 1;
                        for &c in &ext[..ext_len] {
                            entry.name[n] = if nt & 0x10 != 0 {
                                c.to_ascii_lowercase()
                            } else {
                                c
                            };
                            n += 1;
                        }
                    }
                    entry.len = n as u8;
                }
                lfn_valid = false;
                if entry.name() == b"." || entry.name() == b".." {
                    continue;
                }
                if !f(&entry) {
                    return Ok(());
                }
            }
        }
    }

    fn find_in_dir(&mut self, cluster: u32, name: &[u8]) -> Result<Option<Entry>, FatError> {
        let mut found = None;
        self.for_each_entry(cluster, |e| {
            if e.name().eq_ignore_ascii_case(name) {
                found = Some(*e);
                false
            } else {
                true
            }
        })?;
        Ok(found)
    }

    /// Resolve um caminho (`/`, `\` ou sem separador inicial; comparação sem distinguir caixa).
    pub fn lookup(&mut self, path: &[u8]) -> Result<Entry, FatError> {
        let mut cur = Entry {
            name: [0; NAME_MAX],
            len: 0,
            attr: ATTR_DIR,
            cluster: 0,
            size: 0,
        };
        for comp in path.split(|&c| c == b'/' || c == b'\\') {
            if comp.is_empty() || comp == b"." {
                continue;
            }
            if !cur.is_dir() {
                return Err(FatError::NotDir);
            }
            cur = self
                .find_in_dir(cur.cluster, comp)?
                .ok_or(FatError::NotFound)?;
        }
        Ok(cur)
    }

    /// Lê `buf.len()` bytes de `file` a partir de `offset`; devolve quantos leu.
    pub fn read(&mut self, file: &Entry, offset: u64, buf: &mut [u8]) -> Result<usize, FatError> {
        if file.is_dir() {
            return Err(FatError::IsDir);
        }
        if offset >= file.size as u64 {
            return Ok(0);
        }
        let n = buf.len().min((file.size as u64 - offset) as usize);
        let cb = self.cluster_bytes() as u64;
        let mut cluster = file.cluster;
        let mut skip = offset / cb;
        let mut hops = 0u32;
        while skip > 0 {
            cluster = self
                .next_cluster(cluster)?
                .ok_or(FatError::Corrupted("arquivo mais curto que o tamanho"))?;
            skip -= 1;
            hops += 1;
            if hops > 1 << 20 {
                return Err(FatError::Corrupted("cadeia longa demais"));
            }
        }
        let mut done = 0usize;
        let mut pos = offset;
        let mut s = [0u8; SECTOR];
        while done < n {
            let in_cluster = pos % cb;
            let sector = in_cluster / SECTOR as u64;
            let in_sector = (in_cluster % SECTOR as u64) as usize;
            let lba = self.cluster_lba(cluster)? + sector;
            self.dev.read_sector(lba, &mut s)?;
            let take = (SECTOR - in_sector).min(n - done);
            buf[done..done + take].copy_from_slice(&s[in_sector..in_sector + take]);
            done += take;
            pos += take as u64;
            if done < n && pos.is_multiple_of(cb) {
                cluster = self
                    .next_cluster(cluster)?
                    .ok_or(FatError::Corrupted("arquivo mais curto que o tamanho"))?;
            }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests;
