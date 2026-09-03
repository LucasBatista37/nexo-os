//! `reset` — reset preservando arquivos (Plano §Fase 8: "criar reset preservando arquivos
//! quando possível"). Limpa o volume de dados em **pós-ordem** (filhos antes do diretório;
//! `unlink` do `nexo.fs` remove diretórios vazios), **preservando a subárvore do usuário** e
//! os ancestrais dela. O reset do SISTEMA já existe por outro caminho (slots A/B + ambiente de
//! recuperação); este serviço cuida do volume de dados. "Quando possível": o orquestrador
//! espelha o diretório preservado no disco de backup ANTES (serviço `backup`) — cinto e
//! suspensório. Handle 0 = orquestrador: pedido "limpa <dir-preservado>" TRAZ o canal do fs e
//! a resposta "ok <n>" o DEVOLVE (a capacidade é emprestada e volta, como no editor/backup).
#![no_std]
#![no_main]

use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
/// Profundidade máxima da limpeza (o `nexo.fs` também limita caminhos).
const DEPTH_MAX: u32 = 6;

/// Cliente `nexo.fs` mínimo: list (com tipo) e unlink.
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

    fn path_req(path: &[u8]) -> ([u8; 256], u32) {
        let mut p = [0u8; 256];
        let n = path.len().min(256);
        p[..n].copy_from_slice(&path[..n]);
        (p, n as u32)
    }

    /// Lista até 16 entradas de `dir`: nomes, tamanhos de nome e tipos (2 = diretório).
    fn list(
        &mut self,
        dir: &[u8],
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

    fn unlink(&mut self, path: &[u8]) -> Option<()> {
        use nexo_proto::fs as pfs;
        let (p, path_len) = Self::path_req(path);
        let m = (pfs::UnlinkRequest { path: p, path_len })
            .encode_msg(&mut self.req)
            .ok()?;
        let rn = self.rpc(m)?;
        pfs::decode_unlink_response(&self.reply[..rn]).ok()?;
        Some(())
    }
}

/// Monta `dir + "/" + nome` em `out`; devolve o tamanho (`None` se não coubesse).
fn join(out: &mut [u8; 256], dir: &[u8], name: &[u8]) -> Option<usize> {
    let sep = if dir == b"/" { 0 } else { 1 };
    let total = dir.len() + sep + name.len();
    if total > 256 {
        return None;
    }
    out[..dir.len()].copy_from_slice(dir);
    if sep == 1 {
        out[dir.len()] = b'/';
    }
    out[dir.len() + sep..total].copy_from_slice(name);
    Some(total)
}

/// `true` se `keep` está DENTRO de `path` (ou seja, `path` é ancestral da subárvore preservada).
fn is_ancestor_of_keep(path: &[u8], keep: &[u8]) -> bool {
    keep.len() > path.len() && keep.starts_with(path) && keep[path.len()] == b'/'
}

/// Limpa o conteúdo de `dir` em pós-ordem, preservando `keep` (subárvore) e seus ancestrais.
/// Devolve quantos nós removeu.
fn limpa(fs: &mut Fs, dir: &[u8], keep: &[u8], depth: u32, removed: &mut u32) -> Option<()> {
    if depth == 0 {
        return None; // fundo demais: melhor falhar do que remover errado
    }
    loop {
        let mut names = [[0u8; 32]; 16];
        let mut lens = [0usize; 16];
        let mut kinds = [0u8; 16];
        let count = fs.list(dir, &mut names, &mut lens, &mut kinds)?;
        let mut progress = false;
        for i in 0..count {
            let mut pb = [0u8; 256];
            let pl = join(&mut pb, dir, &names[i][..lens[i]])?;
            let path = &pb[..pl];
            if path == keep {
                continue; // a subárvore preservada fica inteira
            }
            if is_ancestor_of_keep(path, keep) {
                // ancestral do preservado: limpa por dentro, mas o diretório em si fica
                limpa(fs, path, keep, depth - 1, removed)?;
                continue;
            }
            if kinds[i] == 2 {
                limpa(fs, path, keep, depth - 1, removed)?;
            }
            fs.unlink(path)?;
            *removed += 1;
            progress = true;
        }
        if count == 0 || !progress {
            return Some(());
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];
    log!("reset: pronto (fs emprestado por pedido)");
    loop {
        let (n, nh) = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(_) => nexo_sys::exit(0), // cordão de vida
        };
        if nh != 1 || n <= 6 || &buf[..6] != b"limpa " {
            let _ = nexo_sys::channel_send(PIPE, b"?", &[]);
            continue;
        }
        let mut fs = Fs {
            ch: hs[0],
            req: [0; 4096],
            reply: [0; 4096],
        };
        let mut keep = [0u8; 256];
        let kl = (n - 6).min(256);
        keep[..kl].copy_from_slice(&buf[6..6 + kl]);
        let keep = &keep[..kl];
        if keep.first() != Some(&b'/') {
            let _ = nexo_sys::channel_send(PIPE, b"erro caminho", &[fs.ch]);
            continue;
        }
        let mut removed = 0u32;
        match limpa(&mut fs, b"/", keep, DEPTH_MAX, &mut removed) {
            Some(()) => {
                let keep_str = core::str::from_utf8(keep).unwrap_or("?");
                log!(
                    "reset: volume limpo — {} no(s) removido(s), '{}' preservado",
                    removed,
                    keep_str
                );
                let mut r = [0u8; 16];
                r[..3].copy_from_slice(b"ok ");
                let mut v = removed;
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
                let _ = nexo_sys::channel_send(PIPE, &r[..rl], &[fs.ch]);
            }
            None => {
                let _ = nexo_sys::channel_send(PIPE, b"erro", &[fs.ch]);
            }
        }
        let _ = fs;
    }
}
