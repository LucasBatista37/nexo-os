//! `backup` — backup e restauração de dados (Plano §Fase 8: "criar backup e restauração de
//! dados de usuário"). Espelha os ARQUIVOS de um diretório entre dois volumes `nexo.fs`
//! independentes — na prática, dois DISCOS físicos distintos (o principal virtio-blk e o de
//! backup AHCI): perder um disco não perde os dados. Cópia pelo protocolo tipado, arquivo a
//! arquivo e **recursiva** (árvores aninhadas descem inteiras); o serviço só copia, nunca apaga.
//! Handle 0 = orquestrador: "espelha <dir>" / "restaura <dir>", cada pedido TRAZ o canal do
//! fs de origem e a resposta "ok <n>" o DEVOLVE (o fs atende um cliente por vez — a
//! capacidade é emprestada e volta, como no editor); handle 1 = fs de BACKUP (permanente).
#![no_std]
#![no_main]

use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const DST: Handle = 1;
const FILE_MAX: usize = 8192;

static mut COPYBUF: [u8; FILE_MAX] = [0; FILE_MAX];

fn fail(code: i64, what: &str) -> ! {
    log!("backup: falha: {}", what);
    nexo_sys::exit(code)
}

/// Cliente `nexo.fs` mínimo: list, leitura completa, escrita completa (create+truncate+write).
struct Fs {
    ch: Handle,
    req: [u8; 4096],
    reply: [u8; 4096],
}

impl Fs {
    fn rpc(&mut self, m: usize) -> Option<usize> {
        if nexo_sys::channel_send(self.ch, &self.req[..m], &[]) != Status::Ok {
            return None;
        }
        let mut hs = [0u32; 1];
        nexo_sys::channel_recv(self.ch, &mut self.reply, &mut hs)
            .ok()
            .map(|(n, _)| n)
    }

    fn path_req(path: &str) -> ([u8; 256], u32) {
        let mut p = [0u8; 256];
        let n = path.len().min(256);
        p[..n].copy_from_slice(&path.as_bytes()[..n]);
        (p, n as u32)
    }

    /// Lista as entradas de `dir` (nome, tamanho do nome, tipo — 2 = diretório).
    fn list_entries(
        &mut self,
        dir: &str,
        names: &mut [[u8; 32]; 16],
        lens: &mut [usize; 16],
        kinds: &mut [u8; 16],
    ) -> Option<usize> {
        use nexo_proto::fs as pfs;
        let (path, path_len) = Self::path_req(dir);
        let m = pfs::ListRequest { path, path_len }
            .encode_msg(&mut self.req)
            .ok()?;
        let rn = self.rpc(m)?;
        let r = pfs::decode_list_response(&self.reply[..rn]).ok()?;
        let entries = r.entries();
        let mut count = 0usize;
        let mut pos = 0usize;
        while pos + 6 <= entries.len() && count < 16 {
            let kind = entries[pos + 4];
            let nl = entries[pos + 5] as usize;
            if pos + 6 + nl > entries.len() {
                break;
            }
            if nl <= 32 {
                names[count][..nl].copy_from_slice(&entries[pos + 6..pos + 6 + nl]);
                lens[count] = nl;
                kinds[count] = kind;
                count += 1;
            }
            pos += 6 + nl;
        }
        Some(count)
    }

    fn read_all(&mut self, path: &str, out: &mut [u8]) -> Option<usize> {
        use nexo_proto::fs as pfs;
        let (p, path_len) = Self::path_req(path);
        let m = pfs::StatRequest { path: p, path_len }
            .encode_msg(&mut self.req)
            .ok()?;
        let rn = self.rpc(m)?;
        let st = pfs::decode_stat_response(&self.reply[..rn]).ok()?;
        let size = st.size as usize;
        if size > out.len() {
            return None;
        }
        let mut off = 0usize;
        while off < size {
            let want = (size - off).min(3900) as u32;
            let m = pfs::ReadRequest {
                ino: st.ino,
                offset: off as u64,
                len: want,
            }
            .encode_msg(&mut self.req)
            .ok()?;
            let rn = self.rpc(m)?;
            let r = pfs::decode_read_response(&self.reply[..rn]).ok()?;
            let dl = r.data().len();
            if dl == 0 {
                return None;
            }
            out[off..off + dl].copy_from_slice(r.data());
            off += dl;
        }
        Some(size)
    }

    /// Cria `dir` E os ancestrais (mkdir -p): o destino do espelho pode ser aninhado.
    fn mkdir(&mut self, dir: &str) {
        use nexo_proto::fs as pfs;
        let bytes = dir.as_bytes();
        let mut end = 1;
        while end <= bytes.len() {
            if end == bytes.len() || bytes[end] == b'/' {
                let (path, path_len) = Self::path_req(&dir[..end]);
                if let Ok(m) = (pfs::MkdirRequest { path, path_len }).encode_msg(&mut self.req) {
                    let _ = self.rpc(m); // idempotente: Exists é aceitável
                }
            }
            end += 1;
        }
    }

    fn write_all(&mut self, path: &str, data: &[u8]) -> Option<()> {
        use nexo_proto::fs as pfs;
        let (p, path_len) = Self::path_req(path);
        let m = pfs::StatRequest { path: p, path_len }
            .encode_msg(&mut self.req)
            .ok()?;
        let rn = self.rpc(m)?;
        let ino = match pfs::decode_stat_response(&self.reply[..rn]) {
            Ok(st) => st.ino,
            Err(_) => {
                let (p, path_len) = Self::path_req(path);
                let m = pfs::CreateRequest { path: p, path_len }
                    .encode_msg(&mut self.req)
                    .ok()?;
                let rn = self.rpc(m)?;
                pfs::decode_create_response(&self.reply[..rn]).ok()?.ino
            }
        };
        let m = pfs::TruncateRequest { ino, size: 0 }
            .encode_msg(&mut self.req)
            .ok()?;
        let rn = self.rpc(m)?;
        pfs::decode_truncate_response(&self.reply[..rn]).ok()?;
        let mut off = 0usize;
        while off < data.len() {
            let n = (data.len() - off).min(3900);
            let mut rq = pfs::WriteRequest {
                ino,
                offset: off as u64,
                data: [0; 3900],
                data_len: n as u32,
            };
            rq.data[..n].copy_from_slice(&data[off..off + n]);
            let m = rq.encode_msg(&mut self.req).ok()?;
            let rn = self.rpc(m)?;
            let w = pfs::decode_write_response(&self.reply[..rn]).ok()?;
            if w.written as usize != n {
                return None;
            }
            off += n;
        }
        let m = pfs::TruncateRequest {
            ino,
            size: data.len() as u64,
        }
        .encode_msg(&mut self.req)
        .ok()?;
        let rn = self.rpc(m)?;
        pfs::decode_truncate_response(&self.reply[..rn]).ok()?;
        Some(())
    }
}

/// Profundidade máxima do espelho (o `nexo.fs` também limita caminhos).
const DEPTH_MAX: u32 = 6;

/// Escreve `v` em decimal no início de `out`; devolve quantos bytes usou.
fn fmt_u32(v: u32, out: &mut [u8]) -> usize {
    let mut digits = [0u8; 10];
    let mut d = 0;
    let mut v = v;
    loop {
        digits[d] = b'0' + (v % 10) as u8;
        v /= 10;
        d += 1;
        if v == 0 {
            break;
        }
    }
    for i in 0..d {
        out[i] = digits[d - 1 - i];
    }
    d
}

/// Espelha `dir` do volume `from` para o `to`, **recursivamente** (árvores aninhadas):
/// arquivos são copiados, subdiretórios criados e descidos. Devolve quantos ARQUIVOS copiou.
fn mirror(from: &mut Fs, to: &mut Fs, dir: &str, depth: u32) -> Option<u32> {
    if depth == 0 {
        return None; // fundo demais: melhor falhar do que espelhar pela metade em silêncio
    }
    let mut names = [[0u8; 32]; 16];
    let mut lens = [0usize; 16];
    let mut kinds = [0u8; 16];
    let count = from.list_entries(dir, &mut names, &mut lens, &mut kinds)?;
    to.mkdir(dir);
    let mut copied = 0u32;
    for i in 0..count {
        let mut pb = [0u8; 300];
        let mut pl = 0;
        pb[..dir.len()].copy_from_slice(dir.as_bytes());
        pl += dir.len();
        if !dir.ends_with('/') {
            pb[pl] = b'/';
            pl += 1;
        }
        pb[pl..pl + lens[i]].copy_from_slice(&names[i][..lens[i]]);
        pl += lens[i];
        let path = core::str::from_utf8(&pb[..pl]).ok()?;
        if kinds[i] == 2 {
            copied += mirror(from, to, path, depth - 1)?;
            continue;
        }
        // SAFETY: unico acesso; processo de uma so thread (buffer estatico para arquivos
        // maiores; a recursao nao o usa em dois niveis ao mesmo tempo — copia um arquivo
        // por vez, sempre por inteiro, antes de descer ou seguir adiante).
        let buf = unsafe { &mut *core::ptr::addr_of_mut!(COPYBUF) };
        let n = from.read_all(path, buf)?;
        to.write_all(path, &buf[..n])?;
        copied += 1;
    }
    Some(copied)
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut dst = Fs {
        ch: DST,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];
    // Agendamento: com um contrato ativo, o fs de origem fica RETIDO (emprestado pela duração
    // do contrato, não por pedido) e o espelho roda sozinho a cada intervalo até "cancela".
    let mut agenda: Option<(Fs, u64, [u8; 128], usize, u64, u32)> = None; // (fs, ns, dir, dl, prazo, execs)
    log!("backup: pronto (backup=1; origem emprestada por pedido ou por contrato de agenda)");
    loop {
        let (n, nh) = if let Some(a) = agenda.as_mut() {
            // contrato ativo: atende pedidos sem bloquear e espelha quando o prazo vence
            match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
                Ok(v) => v,
                Err(Status::WouldBlock) => {
                    let now = nexo_sys::time_now();
                    if now >= a.4 {
                        let dir = core::str::from_utf8(&a.2[..a.3]).unwrap_or("/");
                        if mirror(&mut a.0, &mut dst, dir, DEPTH_MAX).is_some() {
                            a.5 += 1;
                            log!("backup: agenda — espelho #{} de '{}'", a.5, dir);
                        } else {
                            log!(
                                "backup: agenda — espelho de '{}' falhou; contrato segue",
                                dir
                            );
                        }
                        a.4 = now + a.1;
                    }
                    nexo_sys::sleep_ns(20_000_000);
                    continue;
                }
                Err(_) => nexo_sys::exit(0),
            }
        } else {
            match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
                Ok(v) => v,
                Err(_) => nexo_sys::exit(0), // cordão de vida
            }
        };
        // "cancela": encerra o contrato e DEVOLVE a capacidade do fs com a contagem
        if &buf[..n] == b"cancela" {
            match agenda.take() {
                Some((src, _, _, _, _, execs)) => {
                    let mut r = [0u8; 16];
                    r[..3].copy_from_slice(b"ok ");
                    let rl = 3 + fmt_u32(execs, &mut r[3..]);
                    let _ = nexo_sys::channel_send(PIPE, &r[..rl], &[src.ch]);
                }
                None => {
                    let _ = nexo_sys::channel_send(PIPE, b"?", &[]);
                }
            }
            continue;
        }
        if nh != 1 {
            let _ = nexo_sys::channel_send(PIPE, b"?", &[]);
            continue;
        }
        let mut src = Fs {
            ch: hs[0],
            req: [0; 4096],
            reply: [0; 4096],
        };
        // "agenda <segundos> <dir>": retém o fs e espelha `dir` a cada intervalo
        if n > 7 && &buf[..7] == b"agenda " {
            let rest = &buf[7..n];
            let Some(sp) = rest.iter().position(|&c| c == b' ') else {
                let _ = nexo_sys::channel_send(PIPE, b"erro sintaxe", &[src.ch]);
                continue;
            };
            let mut secs = 0u64;
            let mut ok = !rest[..sp].is_empty();
            for &c in &rest[..sp] {
                if c.is_ascii_digit() {
                    secs = secs * 10 + (c - b'0') as u64;
                } else {
                    ok = false;
                }
            }
            if !ok || secs == 0 || rest.len() - sp - 1 == 0 || rest.len() - sp - 1 > 128 {
                let _ = nexo_sys::channel_send(PIPE, b"erro agenda", &[src.ch]);
                continue;
            }
            let mut dirb = [0u8; 128];
            let dl = rest.len() - sp - 1;
            dirb[..dl].copy_from_slice(&rest[sp + 1..]);
            let ns = secs * 1_000_000_000;
            let prazo = nexo_sys::time_now() + ns;
            agenda = Some((src, ns, dirb, dl, prazo, 0));
            log!("backup: contrato de agenda — a cada {} s", secs);
            let _ = nexo_sys::channel_send(PIPE, b"agendado", &[]);
            continue;
        }
        let (verbo, dir_bytes) = if n > 8 && &buf[..8] == b"espelha " {
            (true, &buf[8..n])
        } else if n > 9 && &buf[..9] == b"restaura " {
            (false, &buf[9..n])
        } else {
            let _ = nexo_sys::channel_send(PIPE, b"?", &[]);
            continue;
        };
        let mut dirb = [0u8; 128];
        let dl = dir_bytes.len().min(128);
        dirb[..dl].copy_from_slice(&dir_bytes[..dl]);
        let dir = core::str::from_utf8(&dirb[..dl]).unwrap_or_else(|_| fail(20, "dir"));
        let done = if verbo {
            mirror(&mut src, &mut dst, dir, DEPTH_MAX)
        } else {
            mirror(&mut dst, &mut src, dir, DEPTH_MAX)
        };
        match done {
            Some(copied_n) => {
                let c = copied_n;
                log!(
                    "backup: {} '{}' — {} arquivo(s)",
                    if verbo { "espelhado" } else { "restaurado" },
                    dir,
                    c
                );
                let mut r = [0u8; 16];
                r[..3].copy_from_slice(b"ok ");
                let mut v = c;
                let mut digits = [0u8; 10];
                let mut d = 0;
                loop {
                    digits[d] = b'0' + (v % 10) as u8;
                    v /= 10;
                    d += 1;
                    if v == 0 {
                        break;
                    }
                }
                let mut rl = 3;
                while d > 0 {
                    d -= 1;
                    r[rl] = digits[d];
                    rl += 1;
                }
                let _ = nexo_sys::channel_send(PIPE, &r[..rl], &[src.ch]);
            }
            None => {
                let _ = nexo_sys::channel_send(PIPE, b"erro", &[src.ch]);
            }
        }
    }
}
