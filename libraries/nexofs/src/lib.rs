//! NexoFS v0 — sistema de arquivos persistente **de teste** (Fase 3).
//!
//! Formato em disco (blocos de 2 KiB = 4 setores):
//! - bloco 0: superbloco (`NEXOFS00`, geometria, CRC32);
//! - `bitmap_start..`: bitmap de blocos (1 bit por bloco; cache — é reconstruído na montagem);
//! - `inode_start..`: tabela de inodes de 128 B (tipo, tamanho, 12 ponteiros diretos, 1 indireto, CRC32);
//! - `data_start..`: blocos de dados. Diretórios são arquivos de entradas de 64 B (inode, CRC32, nome ≤ 55 B).
//!
//! Consistência em cortes de energia: toda operação termina com **um** registro de commit que cabe
//! em um setor de 512 B (inode ou entrada de diretório); os dados são escritos antes em blocos novos
//! (copy-on-write) e os antigos liberados depois. Um corte em qualquer ponto deixa cada arquivo na
//! versão anterior ou na nova; blocos/inodes órfãos são recuperados na montagem (`repairs`).
//! Limitação documentada: arquivos com mais de 12 blocos usam um bloco indireto cujos ponteiros
//! são confirmados antes do inode (mistura de versões por bloco possível, nunca ponteiros pendentes).
#![no_std]
#![forbid(unsafe_code)]

/// Tamanho do bloco em bytes.
pub const BLOCK: usize = 2048;
/// Setores de 512 B por bloco.
pub const SECTORS_PER_BLOCK: u64 = (BLOCK / 512) as u64;
/// Assinatura do superbloco.
pub const MAGIC: &[u8; 8] = b"NEXOFS00";
/// Versão do formato.
pub const VERSION: u32 = 0;
/// Tamanho de um inode.
pub const INODE_SIZE: usize = 128;
/// Tamanho de uma entrada de diretório.
pub const DIRENT_SIZE: usize = 64;
/// Tamanho máximo de um nome.
pub const NAME_MAX: usize = 55;
/// Ponteiros diretos por inode.
pub const NDIRECT: usize = 12;
/// Ponteiros por bloco indireto.
pub const PTRS_PER_BLOCK: usize = BLOCK / 8;
/// Tamanho máximo de arquivo.
pub const MAX_FILE: u64 = ((NDIRECT + PTRS_PER_BLOCK) * BLOCK) as u64;
/// Máximo de blocos administrados (bitmap em memória de 8 KiB).
pub const MAX_BLOCKS: usize = 65536;
/// Inode do diretório raiz.
pub const ROOT_INO: u32 = 1;
/// Número de inodes criado por `format`.
pub const DEFAULT_INODES: u32 = 256;
/// Profundidade máxima de diretórios.
pub const MAX_DEPTH: usize = 16;

/// Erro de E/S do dispositivo de blocos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoError;

/// Dispositivo de blocos (2 KiB por bloco).
pub trait BlockDevice {
    /// Número de blocos.
    fn block_count(&self) -> u64;
    /// Lê um bloco.
    fn read_block(&mut self, block: u64, buf: &mut [u8; BLOCK]) -> Result<(), IoError>;
    /// Escreve um bloco.
    fn write_block(&mut self, block: u64, buf: &[u8; BLOCK]) -> Result<(), IoError>;
    /// Garante que escritas anteriores estão duráveis.
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

/// Erros do sistema de arquivos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    /// Falha de E/S.
    Io,
    /// Estrutura inválida em disco.
    Corrupted(&'static str),
    /// Caminho inexistente.
    NotFound,
    /// Nome já existe.
    Exists,
    /// Componente do caminho não é diretório.
    NotDir,
    /// Operação de arquivo sobre um diretório.
    IsDir,
    /// Diretório não vazio.
    NotEmpty,
    /// Sem blocos ou inodes livres.
    NoSpace,
    /// Excede o tamanho máximo de arquivo.
    TooBig,
    /// Nome vazio, longo demais ou com `/`.
    InvalidName,
    /// Argumentos inválidos (inode, offset).
    InvalidArgs,
}

impl FsError {
    /// Código numérico para protocolos.
    pub const fn code(self) -> u8 {
        match self {
            FsError::Io => 1,
            FsError::Corrupted(_) => 2,
            FsError::NotFound => 3,
            FsError::Exists => 4,
            FsError::NotDir => 5,
            FsError::IsDir => 6,
            FsError::NotEmpty => 7,
            FsError::NoSpace => 8,
            FsError::TooBig => 9,
            FsError::InvalidName => 10,
            FsError::InvalidArgs => 11,
        }
    }
}

impl From<IoError> for FsError {
    fn from(_: IoError) -> Self {
        FsError::Io
    }
}

/// Tipo de inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Arquivo regular.
    File = 1,
    /// Diretório.
    Dir = 2,
}

/// Metadados de um objeto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    /// Número do inode.
    pub ino: u32,
    /// Tipo.
    pub kind: Kind,
    /// Tamanho em bytes (diretórios: capacidade de entradas × 64).
    pub size: u64,
}

/// CRC-32 (IEEE, refletido).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}

/// Superbloco decodificado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Superblock {
    /// Total de blocos administrados.
    pub total_blocks: u64,
    /// Primeiro bloco do bitmap.
    pub bitmap_start: u64,
    /// Blocos do bitmap.
    pub bitmap_blocks: u64,
    /// Primeiro bloco da tabela de inodes.
    pub inode_start: u64,
    /// Blocos da tabela de inodes.
    pub inode_blocks: u64,
    /// Primeiro bloco de dados.
    pub data_start: u64,
    /// Número de inodes (índices 1..count).
    pub inode_count: u32,
    /// Geração (incrementada por `format`).
    pub generation: u64,
}

impl Superblock {
    /// Decodifica e valida o bloco 0.
    pub fn decode(b: &[u8; BLOCK]) -> Result<Self, FsError> {
        if &b[0..8] != MAGIC {
            return Err(FsError::Corrupted("assinatura do superbloco"));
        }
        if u32_at(b, 80) != crc32(&b[0..80]) {
            return Err(FsError::Corrupted("crc do superbloco"));
        }
        if u32_at(b, 8) != VERSION || u32_at(b, 12) as usize != BLOCK {
            return Err(FsError::Corrupted("versao/tamanho de bloco"));
        }
        let sb = Superblock {
            total_blocks: u64_at(b, 16),
            bitmap_start: u64_at(b, 24),
            bitmap_blocks: u64_at(b, 32),
            inode_start: u64_at(b, 40),
            inode_blocks: u64_at(b, 48),
            data_start: u64_at(b, 56),
            inode_count: u32_at(b, 64),
            generation: u64_at(b, 72),
        };
        let ok = sb.total_blocks >= 4
            && sb.total_blocks as usize <= MAX_BLOCKS
            && sb.bitmap_start == 1
            && sb.bitmap_blocks >= 1
            && sb.inode_start == sb.bitmap_start + sb.bitmap_blocks
            && sb.inode_blocks >= 1
            && sb.data_start == sb.inode_start + sb.inode_blocks
            && sb.data_start < sb.total_blocks
            && sb.inode_count >= 2
            && (sb.inode_count as u64 * INODE_SIZE as u64).div_ceil(BLOCK as u64)
                == sb.inode_blocks
            && sb.total_blocks.div_ceil(8 * BLOCK as u64) == sb.bitmap_blocks;
        if !ok {
            return Err(FsError::Corrupted("geometria do superbloco"));
        }
        Ok(sb)
    }

    /// Codifica no bloco 0.
    pub fn encode(&self, b: &mut [u8; BLOCK]) {
        b.fill(0);
        b[0..8].copy_from_slice(MAGIC);
        put32(b, 8, VERSION);
        put32(b, 12, BLOCK as u32);
        put64(b, 16, self.total_blocks);
        put64(b, 24, self.bitmap_start);
        put64(b, 32, self.bitmap_blocks);
        put64(b, 40, self.inode_start);
        put64(b, 48, self.inode_blocks);
        put64(b, 56, self.data_start);
        put32(b, 64, self.inode_count);
        put64(b, 72, self.generation);
        let c = crc32(&b[0..80]);
        put32(b, 80, c);
    }
}

/// Inode decodificado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inode {
    /// 0 = livre, 1 = arquivo, 2 = diretório.
    pub kind: u32,
    /// Tamanho em bytes.
    pub size: u64,
    /// Ponteiros diretos (0 = buraco).
    pub direct: [u64; NDIRECT],
    /// Bloco indireto (0 = nenhum).
    pub indirect: u64,
    /// Geração (incrementada a cada commit).
    pub generation: u32,
}

impl Inode {
    const FREE: Inode = Inode {
        kind: 0,
        size: 0,
        direct: [0; NDIRECT],
        indirect: 0,
        generation: 0,
    };

    fn decode(b: &[u8]) -> Result<Self, FsError> {
        let kind = u32_at(b, 0);
        if kind == 0 {
            return Ok(Inode::FREE);
        }
        if u32_at(b, 124) != crc32(&b[0..124]) {
            return Err(FsError::Corrupted("crc de inode"));
        }
        if kind != 1 && kind != 2 {
            return Err(FsError::Corrupted("tipo de inode"));
        }
        let mut direct = [0u64; NDIRECT];
        for (i, d) in direct.iter_mut().enumerate() {
            *d = u64_at(b, 16 + 8 * i);
        }
        Ok(Inode {
            kind,
            size: u64_at(b, 8),
            direct,
            indirect: u64_at(b, 112),
            generation: u32_at(b, 120),
        })
    }

    fn encode(&self, b: &mut [u8]) {
        b[..INODE_SIZE].fill(0);
        if self.kind == 0 {
            return;
        }
        put32(b, 0, self.kind);
        put64(b, 8, self.size);
        for (i, d) in self.direct.iter().enumerate() {
            put64(b, 16 + 8 * i, *d);
        }
        put64(b, 112, self.indirect);
        put32(b, 120, self.generation);
        let c = crc32(&b[0..124]);
        put32(b, 124, c);
    }

    /// Tipo, se em uso.
    pub fn kind(&self) -> Option<Kind> {
        match self.kind {
            1 => Some(Kind::File),
            2 => Some(Kind::Dir),
            _ => None,
        }
    }
}

/// Entrada de diretório decodificada.
#[derive(Clone, Copy)]
pub struct Dirent {
    /// Inode apontado (0 = livre).
    pub ino: u32,
    /// Nome.
    name: [u8; NAME_MAX],
    len: u8,
}

impl Dirent {
    fn decode(b: &[u8]) -> Result<Option<Self>, FsError> {
        let ino = u32_at(b, 0);
        if ino == 0 {
            return Ok(None);
        }
        let mut tmp = [0u8; 60];
        tmp[0..4].copy_from_slice(&b[0..4]);
        tmp[4..60].copy_from_slice(&b[8..64]);
        if u32_at(b, 4) != crc32(&tmp) {
            return Err(FsError::Corrupted("crc de entrada de diretorio"));
        }
        let len = b[8];
        if len == 0 || len as usize > NAME_MAX {
            return Err(FsError::Corrupted("nome de entrada"));
        }
        let mut name = [0u8; NAME_MAX];
        name.copy_from_slice(&b[9..64]);
        Ok(Some(Dirent { ino, name, len }))
    }

    fn encode(ino: u32, name: &[u8], b: &mut [u8]) {
        b[..DIRENT_SIZE].fill(0);
        put32(b, 0, ino);
        b[8] = name.len() as u8;
        b[9..9 + name.len()].copy_from_slice(name);
        let mut tmp = [0u8; 60];
        tmp[0..4].copy_from_slice(&b[0..4]);
        tmp[4..60].copy_from_slice(&b[8..64]);
        let c = crc32(&tmp);
        put32(b, 4, c);
    }

    /// Nome da entrada.
    pub fn name(&self) -> &[u8] {
        &self.name[..self.len as usize]
    }
}

/// Estatísticas do volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Info {
    /// Blocos totais.
    pub total_blocks: u64,
    /// Blocos livres.
    pub free_blocks: u64,
    /// Inodes totais.
    pub inode_count: u32,
    /// Reparos feitos na montagem (blocos/inodes órfãos, bitmap divergente).
    pub repairs: u32,
    /// Geração do volume.
    pub generation: u64,
}

/// Sistema de arquivos montado.
pub struct Fs<D: BlockDevice> {
    dev: D,
    sb: Superblock,
    bitmap: [u8; MAX_BLOCKS / 8],
    repairs: u32,
}

fn valid_name(name: &[u8]) -> Result<(), FsError> {
    if name.is_empty()
        || name.len() > NAME_MAX
        || name.contains(&b'/')
        || name == b"."
        || name == b".."
    {
        return Err(FsError::InvalidName);
    }
    Ok(())
}

impl<D: BlockDevice> Fs<D> {
    /// Formata `dev` (apaga tudo) e monta.
    pub fn format(mut dev: D, inode_count: u32) -> Result<Self, FsError> {
        let total = dev.block_count().min(MAX_BLOCKS as u64);
        let inode_count = inode_count.max(2);
        let bitmap_blocks = total.div_ceil(8 * BLOCK as u64);
        let inode_blocks = (inode_count as u64 * INODE_SIZE as u64).div_ceil(BLOCK as u64);
        let sb = Superblock {
            total_blocks: total,
            bitmap_start: 1,
            bitmap_blocks,
            inode_start: 1 + bitmap_blocks,
            inode_blocks,
            data_start: 1 + bitmap_blocks + inode_blocks,
            inode_count,
            generation: 1,
        };
        if sb.data_start + 4 > total {
            return Err(FsError::NoSpace);
        }
        let mut buf = [0u8; BLOCK];
        // Invalida o superbloco antigo primeiro: um corte durante o format deixa o volume "vazio".
        dev.write_block(0, &buf)?;
        for b in sb.bitmap_start..sb.data_start {
            dev.write_block(b, &buf)?;
        }
        let root = Inode {
            kind: 2,
            size: 0,
            direct: [0; NDIRECT],
            indirect: 0,
            generation: 1,
        };
        root.encode(&mut buf[INODE_SIZE * (ROOT_INO as usize)..]);
        dev.write_block(sb.inode_start, &buf)?;
        // Bitmap inicial: so os blocos de metadados em uso.
        buf.fill(0);
        for b in 0..sb.data_start {
            buf[(b / 8) as usize] |= 1 << (b % 8);
        }
        dev.write_block(sb.bitmap_start, &buf)?;
        sb.encode(&mut buf);
        dev.write_block(0, &buf)?;
        dev.flush()?;
        Self::mount(dev)
    }

    /// Monta um volume existente, verificando e reparando bitmap e inodes órfãos.
    pub fn mount(mut dev: D) -> Result<Self, FsError> {
        let mut buf = [0u8; BLOCK];
        dev.read_block(0, &mut buf)?;
        let sb = Superblock::decode(&buf)?;
        if sb.total_blocks > dev.block_count() {
            return Err(FsError::Corrupted("volume maior que o dispositivo"));
        }
        let mut fs = Fs {
            dev,
            sb,
            bitmap: [0; MAX_BLOCKS / 8],
            repairs: 0,
        };
        fs.check()?;
        Ok(fs)
    }

    /// Devolve o dispositivo.
    pub fn into_device(self) -> D {
        self.dev
    }

    /// Acesso ao dispositivo montado.
    pub fn device(&self) -> &D {
        &self.dev
    }

    /// Estatísticas.
    pub fn info(&self) -> Info {
        let mut free = 0;
        for b in self.sb.data_start..self.sb.total_blocks {
            if !self.bit(b) {
                free += 1;
            }
        }
        Info {
            total_blocks: self.sb.total_blocks,
            free_blocks: free,
            inode_count: self.sb.inode_count,
            repairs: self.repairs,
            generation: self.sb.generation,
        }
    }

    // ---- bitmap ----

    fn bit(&self, b: u64) -> bool {
        self.bitmap[(b / 8) as usize] & (1 << (b % 8)) != 0
    }
    fn set_bit(&mut self, b: u64, v: bool) {
        let (i, m) = ((b / 8) as usize, 1u8 << (b % 8));
        if v {
            self.bitmap[i] |= m;
        } else {
            self.bitmap[i] &= !m;
        }
    }
    fn alloc_block(&mut self) -> Result<u64, FsError> {
        for b in self.sb.data_start..self.sb.total_blocks {
            if !self.bit(b) {
                self.set_bit(b, true);
                return Ok(b);
            }
        }
        Err(FsError::NoSpace)
    }
    fn free_block(&mut self, b: u64) {
        if b >= self.sb.data_start && b < self.sb.total_blocks {
            self.set_bit(b, false);
        }
    }
    fn write_bitmap(&mut self) -> Result<(), FsError> {
        let mut buf = [0u8; BLOCK];
        for i in 0..self.sb.bitmap_blocks {
            let start = (i as usize) * BLOCK;
            let end = (start + BLOCK).min(self.bitmap.len());
            buf.fill(0);
            buf[..end - start].copy_from_slice(&self.bitmap[start..end]);
            self.dev.write_block(self.sb.bitmap_start + i, &buf)?;
        }
        Ok(())
    }
    fn read_bitmap(&mut self, out: &mut [u8; MAX_BLOCKS / 8]) -> Result<(), FsError> {
        let mut buf = [0u8; BLOCK];
        for i in 0..self.sb.bitmap_blocks {
            self.dev.read_block(self.sb.bitmap_start + i, &mut buf)?;
            let start = (i as usize) * BLOCK;
            let end = (start + BLOCK).min(out.len());
            out[start..end].copy_from_slice(&buf[..end - start]);
        }
        Ok(())
    }

    // ---- blocos ----

    fn read_data_block(&mut self, b: u64, buf: &mut [u8; BLOCK]) -> Result<(), FsError> {
        if b < self.sb.data_start || b >= self.sb.total_blocks {
            return Err(FsError::Corrupted(
                "ponteiro de bloco fora da area de dados",
            ));
        }
        Ok(self.dev.read_block(b, buf)?)
    }

    // ---- inodes ----

    fn inode_pos(&self, ino: u32) -> Result<(u64, usize), FsError> {
        if ino == 0 || ino >= self.sb.inode_count {
            return Err(FsError::Corrupted("numero de inode"));
        }
        let off = ino as usize * INODE_SIZE;
        Ok((self.sb.inode_start + (off / BLOCK) as u64, off % BLOCK))
    }

    /// Lê um inode.
    pub fn read_inode(&mut self, ino: u32) -> Result<Inode, FsError> {
        let (blk, off) = self.inode_pos(ino)?;
        let mut buf = [0u8; BLOCK];
        self.dev.read_block(blk, &mut buf)?;
        Inode::decode(&buf[off..off + INODE_SIZE])
    }

    fn write_inode(&mut self, ino: u32, inode: &Inode) -> Result<(), FsError> {
        let (blk, off) = self.inode_pos(ino)?;
        let mut buf = [0u8; BLOCK];
        self.dev.read_block(blk, &mut buf)?;
        inode.encode(&mut buf[off..off + INODE_SIZE]);
        Ok(self.dev.write_block(blk, &buf)?)
    }

    fn alloc_inode(&mut self) -> Result<u32, FsError> {
        let mut buf = [0u8; BLOCK];
        for blk in 0..self.sb.inode_blocks {
            self.dev.read_block(self.sb.inode_start + blk, &mut buf)?;
            for i in 0..BLOCK / INODE_SIZE {
                let ino = (blk as usize * (BLOCK / INODE_SIZE) + i) as u32;
                if ino <= ROOT_INO || ino >= self.sb.inode_count {
                    continue;
                }
                if u32_at(&buf, i * INODE_SIZE) == 0 {
                    return Ok(ino);
                }
            }
        }
        Err(FsError::NoSpace)
    }

    /// Ponteiro do bloco lógico `idx` (0 = buraco).
    fn block_ptr(&mut self, inode: &Inode, idx: usize) -> Result<u64, FsError> {
        if idx < NDIRECT {
            return Ok(inode.direct[idx]);
        }
        let i = idx - NDIRECT;
        if i >= PTRS_PER_BLOCK {
            return Err(FsError::TooBig);
        }
        if inode.indirect == 0 {
            return Ok(0);
        }
        let mut buf = [0u8; BLOCK];
        self.read_data_block(inode.indirect, &mut buf)?;
        Ok(u64_at(&buf, i * 8))
    }

    /// Lê `buf.len()` bytes de `ino` a partir de `offset`; devolve quantos foram lidos.
    pub fn read(&mut self, ino: u32, offset: u64, buf: &mut [u8]) -> Result<usize, FsError> {
        let inode = self.read_inode(ino)?;
        if inode.kind == 0 {
            return Err(FsError::InvalidArgs);
        }
        if offset >= inode.size {
            return Ok(0);
        }
        let n = buf.len().min((inode.size - offset) as usize);
        let mut done = 0;
        let mut blk = [0u8; BLOCK];
        while done < n {
            let pos = offset + done as u64;
            let idx = (pos / BLOCK as u64) as usize;
            let in_off = (pos % BLOCK as u64) as usize;
            let take = (BLOCK - in_off).min(n - done);
            let ptr = self.block_ptr(&inode, idx)?;
            if ptr == 0 {
                buf[done..done + take].fill(0);
            } else {
                self.read_data_block(ptr, &mut blk)?;
                buf[done..done + take].copy_from_slice(&blk[in_off..in_off + take]);
            }
            done += take;
        }
        Ok(n)
    }

    /// Escreve `data` em `ino` a partir de `offset` (copy-on-write, commit pelo inode).
    pub fn write(&mut self, ino: u32, offset: u64, data: &[u8]) -> Result<usize, FsError> {
        let mut inode = self.read_inode(ino)?;
        match inode.kind() {
            Some(Kind::File) => {}
            Some(Kind::Dir) => return Err(FsError::IsDir),
            None => return Err(FsError::InvalidArgs),
        }
        if data.is_empty() {
            return Ok(0);
        }
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or(FsError::TooBig)?;
        if end > MAX_FILE {
            return Err(FsError::TooBig);
        }
        const CHUNK: usize = 32;
        let mut old = [0u64; CHUNK];
        let mut nold = 0;
        let mut done = 0;
        let mut blk = [0u8; BLOCK];
        let mut ind = [0u8; BLOCK];
        let mut ind_loaded = false;
        let mut ind_dirty = false;
        while done < data.len() {
            let pos = offset + done as u64;
            let idx = (pos / BLOCK as u64) as usize;
            let in_off = (pos % BLOCK as u64) as usize;
            let take = (BLOCK - in_off).min(data.len() - done);
            let old_ptr = self.block_ptr(&inode, idx)?;
            if take < BLOCK && old_ptr != 0 {
                self.read_data_block(old_ptr, &mut blk)?;
                // Bytes alem do tamanho antigo sao lixo: zera.
                let valid = inode
                    .size
                    .saturating_sub(idx as u64 * BLOCK as u64)
                    .min(BLOCK as u64) as usize;
                blk[valid..].fill(0);
            } else {
                blk.fill(0);
            }
            blk[in_off..in_off + take].copy_from_slice(&data[done..done + take]);
            let nb = self.alloc_block()?;
            if let Err(e) = self.dev.write_block(nb, &blk) {
                self.free_block(nb);
                return Err(e.into());
            }
            if idx < NDIRECT {
                inode.direct[idx] = nb;
            } else {
                if inode.indirect == 0 {
                    let ib = self.alloc_block()?;
                    ind.fill(0);
                    self.dev.write_block(ib, &ind)?;
                    inode.indirect = ib;
                    ind_loaded = true;
                } else if !ind_loaded {
                    self.read_data_block(inode.indirect, &mut ind)?;
                    ind_loaded = true;
                }
                put64(&mut ind, (idx - NDIRECT) * 8, nb);
                ind_dirty = true;
            }
            if old_ptr != 0 {
                old[nold] = old_ptr;
                nold += 1;
            }
            done += take;
            if nold == CHUNK {
                break;
            }
        }
        if ind_dirty {
            self.dev.write_block(inode.indirect, &ind)?;
        }
        inode.size = inode.size.max(offset + done as u64);
        inode.generation = inode.generation.wrapping_add(1);
        self.write_inode(ino, &inode)?; // commit
        for &o in &old[..nold] {
            self.free_block(o);
        }
        self.write_bitmap()?;
        if done < data.len() {
            let more = self.write(ino, offset + done as u64, &data[done..])?;
            return Ok(done + more);
        }
        Ok(done)
    }

    /// Trunca `ino` para `size` bytes (só encolhe).
    pub fn truncate(&mut self, ino: u32, size: u64) -> Result<(), FsError> {
        let mut inode = self.read_inode(ino)?;
        if inode.kind() != Some(Kind::File) {
            return Err(FsError::InvalidArgs);
        }
        if size >= inode.size {
            return Ok(());
        }
        let keep = size.div_ceil(BLOCK as u64) as usize;
        let old = inode;
        for d in inode.direct.iter_mut().skip(keep) {
            *d = 0;
        }
        let mut ind = [0u8; BLOCK];
        if inode.indirect != 0 {
            if keep <= NDIRECT {
                inode.indirect = 0;
            } else {
                self.read_data_block(inode.indirect, &mut ind)?;
                for i in (keep - NDIRECT)..PTRS_PER_BLOCK {
                    put64(&mut ind, i * 8, 0);
                }
                self.dev.write_block(inode.indirect, &ind)?;
            }
        }
        inode.size = size;
        inode.generation = inode.generation.wrapping_add(1);
        self.write_inode(ino, &inode)?; // commit
        // Libera o que saiu.
        for (i, d) in old.direct.iter().enumerate() {
            if i >= keep && *d != 0 {
                self.free_block(*d);
            }
        }
        if old.indirect != 0 {
            let mut oind = [0u8; BLOCK];
            self.read_data_block(old.indirect, &mut oind)?;
            let from = keep.saturating_sub(NDIRECT);
            for i in from..PTRS_PER_BLOCK {
                let p = u64_at(&oind, i * 8);
                if p != 0 {
                    self.free_block(p);
                }
            }
            if keep <= NDIRECT {
                self.free_block(old.indirect);
            }
        }
        self.write_bitmap()
    }

    // ---- diretorios ----

    /// Chama `f(indice, entrada)` para cada entrada em uso; para se `f` devolver `false`.
    fn for_each_entry(
        &mut self,
        dir: &Inode,
        mut f: impl FnMut(usize, &Dirent) -> bool,
    ) -> Result<(), FsError> {
        let entries = (dir.size / DIRENT_SIZE as u64) as usize;
        let mut blk = [0u8; BLOCK];
        let per_block = BLOCK / DIRENT_SIZE;
        let mut idx = 0;
        while idx < entries {
            let ptr = self.block_ptr(dir, idx / per_block)?;
            if ptr == 0 {
                idx += per_block;
                continue;
            }
            self.read_data_block(ptr, &mut blk)?;
            for i in 0..per_block {
                if idx + i >= entries {
                    break;
                }
                if let Some(e) = Dirent::decode(&blk[i * DIRENT_SIZE..(i + 1) * DIRENT_SIZE])?
                    && !f(idx + i, &e)
                {
                    return Ok(());
                }
            }
            idx += per_block;
        }
        Ok(())
    }

    fn find_in_dir(&mut self, dir: &Inode, name: &[u8]) -> Result<Option<(usize, u32)>, FsError> {
        let mut found = None;
        self.for_each_entry(dir, |i, e| {
            if e.name() == name {
                found = Some((i, e.ino));
                false
            } else {
                true
            }
        })?;
        Ok(found)
    }

    fn write_entry(
        &mut self,
        dir: &Inode,
        index: usize,
        ino: u32,
        name: &[u8],
    ) -> Result<(), FsError> {
        let per_block = BLOCK / DIRENT_SIZE;
        let ptr = self.block_ptr(dir, index / per_block)?;
        if ptr == 0 {
            return Err(FsError::Corrupted("bloco de diretorio ausente"));
        }
        let mut blk = [0u8; BLOCK];
        self.read_data_block(ptr, &mut blk)?;
        let off = (index % per_block) * DIRENT_SIZE;
        if ino == 0 {
            blk[off..off + DIRENT_SIZE].fill(0);
        } else {
            Dirent::encode(ino, name, &mut blk[off..off + DIRENT_SIZE]);
        }
        Ok(self.dev.write_block(ptr, &blk)?)
    }

    /// Resolve `path` → (inode, dados).
    pub fn resolve(&mut self, path: &[u8]) -> Result<(u32, Inode), FsError> {
        let mut ino = ROOT_INO;
        let mut inode = self.read_inode(ino)?;
        if inode.kind() != Some(Kind::Dir) {
            return Err(FsError::Corrupted("raiz nao e diretorio"));
        }
        for comp in path.split(|&c| c == b'/') {
            if comp.is_empty() || comp == b"." {
                continue;
            }
            if inode.kind() != Some(Kind::Dir) {
                return Err(FsError::NotDir);
            }
            let (_, next) = self.find_in_dir(&inode, comp)?.ok_or(FsError::NotFound)?;
            inode = self.read_inode(next)?;
            if inode.kind == 0 {
                return Err(FsError::Corrupted("entrada aponta para inode livre"));
            }
            ino = next;
        }
        Ok((ino, inode))
    }

    fn split_parent(path: &[u8]) -> Result<(&[u8], &[u8]), FsError> {
        let trimmed = {
            let mut end = path.len();
            while end > 0 && path[end - 1] == b'/' {
                end -= 1;
            }
            &path[..end]
        };
        let cut = trimmed.iter().rposition(|&c| c == b'/');
        let (parent, name) = match cut {
            Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
            None => (&trimmed[..0], trimmed),
        };
        valid_name(name)?;
        Ok((parent, name))
    }

    /// Metadados de `path`.
    pub fn stat(&mut self, path: &[u8]) -> Result<Stat, FsError> {
        let (ino, inode) = self.resolve(path)?;
        Ok(Stat {
            ino,
            kind: inode.kind().ok_or(FsError::Corrupted("tipo"))?,
            size: inode.size,
        })
    }

    /// Cria um arquivo ou diretório; devolve o inode.
    pub fn create(&mut self, path: &[u8], kind: Kind) -> Result<u32, FsError> {
        let (parent_path, name) = Self::split_parent(path)?;
        let (pino, mut parent) = self.resolve(parent_path)?;
        if parent.kind() != Some(Kind::Dir) {
            return Err(FsError::NotDir);
        }
        if self.find_in_dir(&parent, name)?.is_some() {
            return Err(FsError::Exists);
        }
        // 1. slot livre no diretorio (ou extensao)
        let entries = (parent.size / DIRENT_SIZE as u64) as usize;
        let mut used = [0u8; (NDIRECT + PTRS_PER_BLOCK) * (BLOCK / DIRENT_SIZE) / 8];
        self.for_each_entry(&parent, |i, _| {
            used[i / 8] |= 1 << (i % 8);
            true
        })?;
        let mut slot = None;
        for i in 0..entries {
            if used[i / 8] & (1 << (i % 8)) == 0 {
                let per_block = BLOCK / DIRENT_SIZE;
                if self.block_ptr(&parent, i / per_block)? != 0 {
                    slot = Some(i);
                    break;
                }
            }
        }
        let slot = match slot {
            Some(s) => s,
            None => {
                let per_block = BLOCK / DIRENT_SIZE;
                let idx = entries / per_block;
                if idx >= NDIRECT {
                    return Err(FsError::NoSpace); // diretorios v0: ate 12 blocos (384 entradas)
                }
                let nb = self.alloc_block()?;
                let zero = [0u8; BLOCK];
                self.dev.write_block(nb, &zero)?;
                parent.direct[idx] = nb;
                parent.size = ((idx + 1) * per_block * DIRENT_SIZE) as u64;
                parent.generation = parent.generation.wrapping_add(1);
                self.write_inode(pino, &parent)?; // commit da extensao (entradas vazias)
                self.write_bitmap()?;
                idx * per_block
            }
        };
        // 2. inode novo
        let ino = self.alloc_inode()?;
        let inode = Inode {
            kind: kind as u32,
            size: 0,
            direct: [0; NDIRECT],
            indirect: 0,
            generation: 1,
        };
        self.write_inode(ino, &inode)?;
        // 3. commit: entrada de diretorio
        self.write_entry(&parent, slot, ino, name)?;
        Ok(ino)
    }

    /// Remove um arquivo ou diretório vazio.
    pub fn unlink(&mut self, path: &[u8]) -> Result<(), FsError> {
        let (parent_path, name) = Self::split_parent(path)?;
        let (_, parent) = self.resolve(parent_path)?;
        if parent.kind() != Some(Kind::Dir) {
            return Err(FsError::NotDir);
        }
        let (slot, ino) = self.find_in_dir(&parent, name)?.ok_or(FsError::NotFound)?;
        let inode = self.read_inode(ino)?;
        if inode.kind() == Some(Kind::Dir) {
            let mut empty = true;
            self.for_each_entry(&inode, |_, _| {
                empty = false;
                false
            })?;
            if !empty {
                return Err(FsError::NotEmpty);
            }
        }
        self.write_entry(&parent, slot, 0, b"")?; // commit
        self.release_inode(ino, &inode)?;
        self.write_bitmap()
    }

    fn release_inode(&mut self, ino: u32, inode: &Inode) -> Result<(), FsError> {
        for &d in &inode.direct {
            if d != 0 {
                self.free_block(d);
            }
        }
        if inode.indirect != 0 {
            let mut ind = [0u8; BLOCK];
            if self.read_data_block(inode.indirect, &mut ind).is_ok() {
                for i in 0..PTRS_PER_BLOCK {
                    let p = u64_at(&ind, i * 8);
                    if p != 0 {
                        self.free_block(p);
                    }
                }
            }
            self.free_block(inode.indirect);
        }
        self.write_inode(ino, &Inode::FREE)
    }

    /// Lista `path` chamando `f(nome, stat)`.
    pub fn list(&mut self, path: &[u8], mut f: impl FnMut(&[u8], Stat)) -> Result<usize, FsError> {
        let (_, dir) = self.resolve(path)?;
        if dir.kind() != Some(Kind::Dir) {
            return Err(FsError::NotDir);
        }
        let mut items = [(0u32, [0u8; NAME_MAX], 0u8); 64];
        let mut n = 0;
        let mut start = 0;
        loop {
            let mut got = 0;
            self.for_each_entry(&dir, |i, e| {
                if i >= start && got < items.len() {
                    items[got] = (e.ino, e.name, e.name().len() as u8);
                    got += 1;
                    start = i + 1;
                }
                got < items.len()
            })?;
            for (ino, name, len) in items.iter().take(got) {
                let inode = self.read_inode(*ino)?;
                let kind = inode
                    .kind()
                    .ok_or(FsError::Corrupted("entrada para inode livre"))?;
                f(
                    &name[..*len as usize],
                    Stat {
                        ino: *ino,
                        kind,
                        size: inode.size,
                    },
                );
                n += 1;
            }
            if got < items.len() {
                break;
            }
        }
        Ok(n)
    }

    /// Força a durabilidade.
    pub fn sync(&mut self) -> Result<(), FsError> {
        self.write_bitmap()?;
        Ok(self.dev.flush()?)
    }

    // ---- verificacao ----

    /// Reconstrói o bitmap a partir da raiz, libera inodes órfãos e reescreve o bitmap se divergir.
    fn check(&mut self) -> Result<(), FsError> {
        let mut disk_bitmap = [0u8; MAX_BLOCKS / 8];
        self.read_bitmap(&mut disk_bitmap)?;
        self.bitmap.fill(0);
        for b in 0..self.sb.data_start {
            self.set_bit(b, true);
        }
        let mut inode_used = [0u8; 8192];
        let mut stack = [(0u32, 0usize); MAX_DEPTH];
        let mut depth = 0;
        let root = self.read_inode(ROOT_INO)?;
        if root.kind() != Some(Kind::Dir) {
            return Err(FsError::Corrupted("raiz"));
        }
        self.mark_inode(ROOT_INO, &root, &mut inode_used)?;
        stack[0] = (ROOT_INO, 0);
        loop {
            let (dino, next) = stack[depth];
            let dir = self.read_inode(dino)?;
            let mut found = None;
            self.for_each_entry(&dir, |i, e| {
                if i >= next {
                    found = Some((i, e.ino));
                    false
                } else {
                    true
                }
            })?;
            match found {
                Some((i, child)) => {
                    stack[depth].1 = i + 1;
                    let inode = self.read_inode(child)?;
                    self.mark_inode(child, &inode, &mut inode_used)?;
                    if inode.kind() == Some(Kind::Dir) {
                        depth += 1;
                        if depth >= MAX_DEPTH {
                            return Err(FsError::Corrupted("diretorios profundos demais"));
                        }
                        stack[depth] = (child, 0);
                    }
                }
                None => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
            }
        }
        // Inodes orfaos.
        let mut buf = [0u8; BLOCK];
        for blk in 0..self.sb.inode_blocks {
            self.dev.read_block(self.sb.inode_start + blk, &mut buf)?;
            let mut dirty = false;
            for i in 0..BLOCK / INODE_SIZE {
                let ino = (blk as usize * (BLOCK / INODE_SIZE) + i) as u32;
                if ino == 0 || ino >= self.sb.inode_count {
                    continue;
                }
                let off = i * INODE_SIZE;
                if u32_at(&buf, off) != 0 && inode_used[ino as usize / 8] & (1 << (ino % 8)) == 0 {
                    buf[off..off + INODE_SIZE].fill(0);
                    dirty = true;
                    self.repairs += 1;
                }
            }
            if dirty {
                self.dev.write_block(self.sb.inode_start + blk, &buf)?;
            }
        }
        if disk_bitmap[..] != self.bitmap[..] {
            self.repairs += 1;
            self.write_bitmap()?;
        }
        Ok(())
    }

    fn mark_inode(
        &mut self,
        ino: u32,
        inode: &Inode,
        used: &mut [u8; 8192],
    ) -> Result<(), FsError> {
        if inode.kind == 0 {
            return Err(FsError::Corrupted("entrada aponta para inode livre"));
        }
        let (i, m) = (ino as usize / 8, 1u8 << (ino % 8));
        if used[i] & m != 0 {
            return Err(FsError::Corrupted("inode referenciado duas vezes"));
        }
        used[i] |= m;
        for &d in &inode.direct {
            if d != 0 {
                self.mark_block(d)?;
            }
        }
        if inode.indirect != 0 {
            self.mark_block(inode.indirect)?;
            let mut ind = [0u8; BLOCK];
            self.read_data_block(inode.indirect, &mut ind)?;
            for k in 0..PTRS_PER_BLOCK {
                let p = u64_at(&ind, k * 8);
                if p != 0 {
                    self.mark_block(p)?;
                }
            }
        }
        Ok(())
    }

    fn mark_block(&mut self, b: u64) -> Result<(), FsError> {
        if b < self.sb.data_start || b >= self.sb.total_blocks {
            return Err(FsError::Corrupted(
                "ponteiro de bloco fora da area de dados",
            ));
        }
        if self.bit(b) {
            return Err(FsError::Corrupted("bloco referenciado duas vezes"));
        }
        self.set_bit(b, true);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
