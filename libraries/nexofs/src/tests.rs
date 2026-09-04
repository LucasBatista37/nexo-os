extern crate std;
use super::*;
use std::boxed::Box;
use std::vec::Vec;

/// Disco em memória com simulação de corte de energia: falha a partir da `cut`-ésima escrita,
/// opcionalmente gravando só os primeiros `torn` setores dela.
struct MemDisk {
    data: Vec<u8>,
    writes: usize,
    cut: Option<(usize, usize)>,
}

impl MemDisk {
    fn new(blocks: usize) -> Self {
        MemDisk {
            data: std::vec![0u8; blocks * BLOCK],
            writes: 0,
            cut: None,
        }
    }
}

impl BlockDevice for MemDisk {
    fn block_count(&self) -> u64 {
        (self.data.len() / BLOCK) as u64
    }
    fn read_block(&mut self, block: u64, buf: &mut [u8; BLOCK]) -> Result<(), IoError> {
        let o = block as usize * BLOCK;
        if o + BLOCK > self.data.len() {
            return Err(IoError);
        }
        buf.copy_from_slice(&self.data[o..o + BLOCK]);
        Ok(())
    }
    fn write_block(&mut self, block: u64, buf: &[u8; BLOCK]) -> Result<(), IoError> {
        let o = block as usize * BLOCK;
        if o + BLOCK > self.data.len() {
            return Err(IoError);
        }
        if let Some((cut, torn)) = self.cut {
            if self.writes > cut {
                return Err(IoError);
            }
            if self.writes == cut {
                self.data[o..o + torn * 512].copy_from_slice(&buf[..torn * 512]);
                self.writes += 1;
                return Err(IoError);
            }
        }
        self.writes += 1;
        self.data[o..o + BLOCK].copy_from_slice(buf);
        Ok(())
    }
}

#[test]
fn format_mount_roundtrip() {
    let fs = Fs::format(MemDisk::new(512), 64).unwrap();
    let info = fs.info();
    assert_eq!(info.total_blocks, 512);
    assert_eq!(info.repairs, 0);
    let dev = fs.into_device();
    let mut fs = Fs::mount(dev).unwrap();
    assert_eq!(fs.info().repairs, 0);
    assert_eq!(fs.list(b"/", |_, _| {}).unwrap(), 0);
}

/// Blocos livres depois de a raiz ganhar seu primeiro bloco de entradas (permanece alocado).
fn baseline_free(fs: &mut Fs<MemDisk>) -> u64 {
    fs.create(b"tmp-baseline", Kind::File).unwrap();
    fs.unlink(b"tmp-baseline").unwrap();
    fs.info().free_blocks
}

#[test]
fn create_write_read_modify_remove() {
    let mut fs = Fs::format(MemDisk::new(512), 64).unwrap();
    let free0 = baseline_free(&mut fs);
    fs.create(b"/docs", Kind::Dir).unwrap();
    let ino = fs.create(b"/docs/a.txt", Kind::File).unwrap();
    assert_eq!(fs.create(b"/docs/a.txt", Kind::File), Err(FsError::Exists));
    assert_eq!(fs.write(ino, 0, b"hello world").unwrap(), 11);
    let mut buf = [0u8; 32];
    assert_eq!(fs.read(ino, 0, &mut buf).unwrap(), 11);
    assert_eq!(&buf[..11], b"hello world");
    // modificação parcial (copy-on-write) e extensão
    fs.write(ino, 6, b"nexo!").unwrap();
    assert_eq!(fs.read(ino, 0, &mut buf).unwrap(), 11);
    assert_eq!(&buf[..11], b"hello nexo!");
    let big: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
    assert_eq!(fs.write(ino, 11, &big).unwrap(), big.len());
    let mut out = std::vec![0u8; 20_011];
    assert_eq!(fs.read(ino, 0, &mut out).unwrap(), 20_011);
    assert_eq!(&out[..11], b"hello nexo!");
    assert_eq!(&out[11..], &big[..]);
    assert_eq!(fs.stat(b"docs/a.txt").unwrap().size, 20_011);
    // truncar e reler
    fs.truncate(ino, 100).unwrap();
    assert_eq!(fs.read(ino, 0, &mut out).unwrap(), 100);
    assert_eq!(&out[..11], b"hello nexo!");
    // listar
    let mut names = Vec::new();
    fs.list(b"/docs", |n, s| names.push((n.to_vec(), s.kind)))
        .unwrap();
    assert_eq!(names, std::vec![(b"a.txt".to_vec(), Kind::File)]);
    // remover
    assert_eq!(fs.unlink(b"/docs"), Err(FsError::NotEmpty));
    fs.unlink(b"/docs/a.txt").unwrap();
    assert_eq!(fs.stat(b"/docs/a.txt"), Err(FsError::NotFound));
    fs.unlink(b"/docs").unwrap();
    assert_eq!(fs.info().free_blocks, free0, "blocos vazaram");
    // persistencia: remonta
    let dev = fs.into_device();
    let mut fs = Fs::mount(dev).unwrap();
    assert_eq!(fs.info().repairs, 0);
    assert_eq!(fs.list(b"/", |_, _| {}).unwrap(), 0);
}

#[test]
fn indirect_blocks_and_large_files() {
    let mut fs = Fs::format(MemDisk::new(1024), 64).unwrap();
    let free0 = baseline_free(&mut fs);
    let ino = fs.create(b"big", Kind::File).unwrap();
    let data: Vec<u8> = (0..(NDIRECT + 40) * BLOCK)
        .map(|i| (i * 7 % 256) as u8)
        .collect();
    assert_eq!(fs.write(ino, 0, &data).unwrap(), data.len());
    let mut out = std::vec![0u8; data.len()];
    assert_eq!(fs.read(ino, 0, &mut out).unwrap(), data.len());
    assert_eq!(out, data);
    assert_eq!(fs.write(ino, MAX_FILE - 1, b"xy"), Err(FsError::TooBig));
    fs.unlink(b"/big").unwrap();
    assert_eq!(fs.info().free_blocks, free0);
}

#[test]
fn many_entries_and_names() {
    let mut fs = Fs::format(MemDisk::new(512), 200).unwrap();
    for i in 0..100 {
        let name = std::format!("f{i:03}");
        fs.create(name.as_bytes(), Kind::File).unwrap();
    }
    assert_eq!(fs.list(b"/", |_, _| {}).unwrap(), 100);
    for i in (0..100).step_by(2) {
        let name = std::format!("f{i:03}");
        fs.unlink(name.as_bytes()).unwrap();
    }
    assert_eq!(fs.list(b"/", |_, _| {}).unwrap(), 50);
    fs.create(b"novo", Kind::File).unwrap(); // reutiliza um slot livre
    assert_eq!(fs.list(b"/", |_, _| {}).unwrap(), 51);
    assert_eq!(fs.create(b"", Kind::File), Err(FsError::InvalidName));
    assert_eq!(fs.create(b"a/b", Kind::File), Err(FsError::NotFound));
    assert_eq!(
        fs.create(&[b'x'; 56], Kind::File),
        Err(FsError::InvalidName)
    );
    assert_eq!(fs.create(b"f001/x", Kind::File), Err(FsError::NotDir));
    assert_eq!(fs.write(ROOT_INO, 0, b"x"), Err(FsError::IsDir));
}

#[test]
fn no_space_is_clean() {
    let mut fs = Fs::format(MemDisk::new(16), 8).unwrap();
    let ino = fs.create(b"f", Kind::File).unwrap();
    let data = std::vec![1u8; 64 * BLOCK];
    let r = fs.write(ino, 0, &data);
    assert!(matches!(r, Ok(_) | Err(FsError::NoSpace)));
    let dev = fs.into_device();
    let fs = Fs::mount(dev).unwrap();
    assert_eq!(fs.info().repairs, 0, "escrita sem espaco deixou orfaos");
}

type Step = Box<dyn Fn(&mut Fs<MemDisk>) -> Result<(), FsError>>;
type Versions = Vec<Option<Vec<u8>>>;

/// Roteiro de operações; cada passo registra o estado esperado (arquivo → conteúdo).
fn workload(fs: &mut Fs<MemDisk>, upto: Option<usize>) -> Result<(), FsError> {
    let steps: Vec<Step> = std::vec![
        Box::new(|fs| fs.create(b"d", Kind::Dir).map(|_| ())),
        Box::new(|fs| fs.create(b"d/a", Kind::File).map(|_| ())),
        Box::new(|fs| {
            let i = fs.stat(b"d/a")?.ino;
            fs.write(i, 0, b"versao 1 de a").map(|_| ())
        }),
        Box::new(|fs| fs.create(b"b", Kind::File).map(|_| ())),
        Box::new(|fs| {
            let i = fs.stat(b"b")?.ino;
            fs.write(i, 0, &std::vec![7u8; 3 * BLOCK + 5]).map(|_| ())
        }),
        Box::new(|fs| {
            let i = fs.stat(b"d/a")?.ino;
            fs.write(i, 7, b"2").map(|_| ())
        }),
        Box::new(|fs| fs.unlink(b"b")),
        Box::new(|fs| {
            let i = fs.stat(b"d/a")?.ino;
            fs.write(i, 0, &std::vec![9u8; 20 * BLOCK]).map(|_| ())
        }),
        Box::new(|fs| {
            let i = fs.stat(b"d/a")?.ino;
            fs.truncate(i, 3)
        }),
        Box::new(|fs| fs.unlink(b"d/a")),
        Box::new(|fs| fs.unlink(b"d")),
    ];
    for (i, s) in steps.iter().enumerate() {
        if upto == Some(i) {
            break;
        }
        s(fs)?;
    }
    Ok(())
}

/// Conteúdos válidos por arquivo em cada instante: após o passo k, `a` e `b` têm uma destas versões.
fn allowed_versions(step: usize) -> (Versions, Versions) {
    // versoes de d/a
    let a_versions: Versions = std::vec![
        None,
        Some(Vec::new()),
        Some(b"versao 1 de a".to_vec()),
        Some(b"versao 2 de a".to_vec()),
        Some(std::vec![9u8; 20 * BLOCK]),
        Some(std::vec![9u8; 3]),
    ];
    let b_versions: Versions =
        std::vec![None, Some(Vec::new()), Some(std::vec![7u8; 3 * BLOCK + 5])];
    // (indice de a antes/depois, idem b) por passo
    let a_idx = [0, 0, 1, 2, 2, 2, 3, 3, 4, 5, 0, 0];
    let b_idx = [0, 0, 0, 0, 1, 2, 2, 0, 0, 0, 0, 0];
    let a_before = a_idx[step];
    let a_after = a_idx[(step + 1).min(11)];
    let b_before = b_idx[step];
    let b_after = b_idx[(step + 1).min(11)];
    (
        std::vec![a_versions[a_before].clone(), a_versions[a_after].clone()],
        std::vec![b_versions[b_before].clone(), b_versions[b_after].clone()],
    )
}

fn read_all(fs: &mut Fs<MemDisk>, path: &[u8]) -> Option<Vec<u8>> {
    let st = fs.stat(path).ok()?;
    let mut v = std::vec![0u8; st.size as usize];
    let n = fs.read(st.ino, 0, &mut v).unwrap();
    v.truncate(n);
    Some(v)
}

#[test]
fn rename_semantics() {
    let mut fs = Fs::format(MemDisk::new(512), 64).unwrap();
    let free0 = baseline_free(&mut fs);
    fs.create(b"/d1", Kind::Dir).unwrap();
    fs.create(b"/d2", Kind::Dir).unwrap();
    let ino = fs.create(b"/d1/a.txt", Kind::File).unwrap();
    fs.write(ino, 0, b"conteudo do a").unwrap();
    // mesmo diretorio: o inode nao muda
    fs.rename(b"/d1/a.txt", b"/d1/b.txt").unwrap();
    assert_eq!(fs.stat(b"/d1/a.txt"), Err(FsError::NotFound));
    assert_eq!(fs.stat(b"/d1/b.txt").unwrap().ino, ino);
    // entre diretorios: mesmo inode, conteudo intacto
    fs.rename(b"/d1/b.txt", b"/d2/c.txt").unwrap();
    assert_eq!(fs.stat(b"/d2/c.txt").unwrap().ino, ino);
    assert_eq!(read_all(&mut fs, b"/d2/c.txt").unwrap(), b"conteudo do a");
    // destino existente (incluindo ele mesmo) e origem inexistente
    fs.create(b"/d2/x.txt", Kind::File).unwrap();
    assert_eq!(fs.rename(b"/d2/c.txt", b"/d2/x.txt"), Err(FsError::Exists));
    assert_eq!(fs.rename(b"/d2/x.txt", b"/d2/x.txt"), Err(FsError::Exists));
    assert_eq!(fs.rename(b"/nada", b"/algo"), Err(FsError::NotFound));
    // diretorio renomeado leva a subarvore junto
    fs.create(b"/d1/sub", Kind::Dir).unwrap();
    let f = fs.create(b"/d1/sub/f.txt", Kind::File).unwrap();
    fs.write(f, 0, b"fundo").unwrap();
    fs.rename(b"/d1", b"/renomeado").unwrap();
    assert_eq!(
        read_all(&mut fs, b"/renomeado/sub/f.txt").unwrap(),
        b"fundo"
    );
    // mover um diretorio para dentro da propria subarvore criaria um ciclo
    assert_eq!(
        fs.rename(b"/renomeado", b"/renomeado/sub/ciclo"),
        Err(FsError::InvalidArgs)
    );
    // limpeza total: nenhum bloco vazou
    fs.unlink(b"/renomeado/sub/f.txt").unwrap();
    fs.unlink(b"/renomeado/sub").unwrap();
    fs.unlink(b"/renomeado").unwrap();
    fs.unlink(b"/d2/c.txt").unwrap();
    fs.unlink(b"/d2/x.txt").unwrap();
    fs.unlink(b"/d2").unwrap();
    assert_eq!(fs.info().free_blocks, free0, "blocos vazaram");
    let mut fs = Fs::mount(fs.into_device()).unwrap();
    assert_eq!(fs.info().repairs, 0);
    assert_eq!(fs.list(b"/", |_, _| {}).unwrap(), 0);
}

#[test]
fn power_cut_during_rename_never_loses_the_file() {
    let monta = || {
        let mut fs = Fs::format(MemDisk::new(256), 32).unwrap();
        fs.create(b"/de", Kind::Dir).unwrap();
        fs.create(b"/para", Kind::Dir).unwrap();
        let ino = fs.create(b"/de/a.txt", Kind::File).unwrap();
        fs.write(ino, 0, b"sobrevive ao corte").unwrap();
        fs
    };
    let total = {
        let mut fs = monta();
        let base = fs.dev.writes;
        fs.rename(b"/de/a.txt", b"/para/b.txt").unwrap();
        fs.dev.writes - base
    };
    assert!(total >= 2, "rename curto demais: {total} escritas");
    let conteudo = b"sobrevive ao corte".to_vec();
    for cut in 0..total {
        for torn in [0usize, 1, 3] {
            let mut fs = monta();
            fs.dev.cut = Some((fs.dev.writes + cut, torn));
            assert_eq!(fs.rename(b"/de/a.txt", b"/para/b.txt"), Err(FsError::Io));
            let mut dev = fs.into_device();
            dev.cut = None;
            let mut fs = Fs::mount(dev)
                .unwrap_or_else(|e| panic!("montagem apos corte em {cut}/{torn}: {e:?}"));
            // A ordem do rename garante: o arquivo existe sob o nome velho OU o novo (um
            // corte entre o commit do destino e a limpeza da origem deixa os DOIS — nunca
            // nenhum), sempre com o conteudo integro.
            let velho = read_all(&mut fs, b"/de/a.txt");
            let novo = read_all(&mut fs, b"/para/b.txt");
            assert!(
                velho.as_ref() == Some(&conteudo) || novo.as_ref() == Some(&conteudo),
                "corte {cut}/{torn}: arquivo perdido (velho={velho:?} novo={novo:?})"
            );
            // volume utilizavel apos o reparo
            let ino = fs.create(b"/pos", Kind::File).unwrap();
            fs.write(ino, 0, b"ok").unwrap();
            fs.unlink(b"/pos").unwrap();
        }
    }
}

#[test]
fn power_cut_at_every_write_keeps_files_consistent() {
    // Conta as escritas de cada passo para saber em que passo o corte cai.
    let mut boundaries = Vec::new();
    {
        let mut fs = Fs::format(MemDisk::new(256), 32).unwrap();
        let base = fs.dev.writes;
        for k in 0..=11 {
            let mut fs2 = Fs::mount(fs.into_device()).unwrap();
            fs2.dev.writes = base;
            workload(&mut fs2, Some(k)).unwrap();
            boundaries.push(fs2.dev.writes - base);
            fs = Fs::format(MemDisk::new(256), 32).unwrap();
            fs.dev.writes = base;
        }
    }
    let total = *boundaries.last().unwrap();
    let mut checked = 0;
    for cut in 0..total {
        for torn in [0usize, 1, 3] {
            let mut fs = Fs::format(MemDisk::new(256), 32).unwrap();
            let base = fs.dev.writes;
            fs.dev.cut = Some((base + cut, torn));
            let r = workload(&mut fs, None);
            assert_eq!(
                r,
                Err(FsError::Io),
                "corte em {cut} nao produziu erro de E/S"
            );
            let step = boundaries
                .iter()
                .position(|&b| b > cut)
                .unwrap_or(11)
                .saturating_sub(1);
            let mut dev = fs.into_device();
            dev.cut = None;
            let mut fs = Fs::mount(dev)
                .unwrap_or_else(|e| panic!("montagem apos corte em {cut}/{torn}: {e:?}"));
            let (a_ok, b_ok) = allowed_versions(step);
            let a = read_all(&mut fs, b"d/a");
            let b = read_all(&mut fs, b"b");
            assert!(
                a_ok.contains(&a),
                "corte {cut}/{torn} passo {step}: d/a = {:?}",
                a.as_ref().map(|v| v.len())
            );
            assert!(
                b_ok.contains(&b),
                "corte {cut}/{torn} passo {step}: b = {:?}",
                b.as_ref().map(|v| v.len())
            );
            // O volume continua utilizavel e sem vazamentos apos o reparo.
            let before = baseline_free(&mut fs);
            let ino = fs.create(b"pos-corte", Kind::File).unwrap();
            fs.write(ino, 0, b"ok").unwrap();
            fs.unlink(b"pos-corte").unwrap();
            assert_eq!(fs.info().free_blocks, before);
            let fs = Fs::mount(fs.into_device()).unwrap();
            assert_eq!(fs.info().repairs, 0, "segunda montagem ainda reparou algo");
            checked += 1;
        }
    }
    assert!(checked >= 60, "poucos cortes testados: {checked}");
}

#[test]
fn fuzz_lite_corrupted_images_never_panic() {
    let mut fs = Fs::format(MemDisk::new(128), 32).unwrap();
    workload(&mut fs, Some(9)).unwrap();
    let image = fs.into_device().data;
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut mounted = 0;
    for _ in 0..400 {
        let mut d = MemDisk::new(128);
        d.data.copy_from_slice(&image);
        let flips = 1 + (next() % 8) as usize;
        for _ in 0..flips {
            // Concentra nas areas de metadados (primeiros 8 blocos) em metade dos casos.
            let range = if next() % 2 == 0 {
                8 * BLOCK
            } else {
                image.len()
            };
            let i = (next() % range as u64) as usize;
            d.data[i] ^= (next() % 255 + 1) as u8;
        }
        if let Ok(mut fs) = Fs::mount(d) {
            mounted += 1;
            let _ = fs.list(b"/", |_, _| {});
            let _ = fs.list(b"/d", |_, _| {});
            let _ = read_all(&mut fs, b"d/a");
            let _ = fs
                .create(b"z", Kind::File)
                .and_then(|i| fs.write(i, 0, &[1u8; 5000]));
            let _ = fs.unlink(b"d/a");
            let _ = Fs::mount(fs.into_device()).map(|fs| fs.info());
        }
    }
    assert!(mounted > 0);
}
