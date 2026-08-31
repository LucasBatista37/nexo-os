//! Personalidade **POSIX de sockets** (ADR-0014, §3: POSIX como compatibilidade em espaço de
//! usuário, mapeando descritores para handles) sobre a API nativa `nexo.sock` do `netd`.
//!
//! Não é uma libc: expõe uma API no estilo BSD (`socket`/`connect`/`send`/`recv`/`close`,
//! `sockaddr_in`, `getaddrinfo`) com descritores inteiros numa tabela do processo, cada um
//! ligado a uma conexão/porta do `netd`. Blocos assíncronos e `poll`/`select` completos ficam
//! para depois; `recv` de TCP faz *spin* curto até haver dados ou o par fechar.
#![no_std]
#![forbid(unsafe_code)]

use nexo_proto::sock::{
    self, ResolveRequest, TcpCloseRequest, TcpConnectRequest, TcpRecvRequest, TcpSendRequest,
    UdpRecvRequest, UdpSendRequest, decode_resolve_response, decode_tcp_close_response,
    decode_tcp_connect_response, decode_tcp_recv_response, decode_tcp_send_response,
    decode_udp_recv_response, decode_udp_send_response,
};
use nexo_sys::Handle;
use nexo_sys::abi::Status;

/// Família de endereços: IPv4.
pub const AF_INET: i32 = 2;
/// Tipo: fluxo confiável (TCP).
pub const SOCK_STREAM: i32 = 1;
/// Tipo: datagrama (UDP).
pub const SOCK_DGRAM: i32 = 2;

/// Códigos de erro estilo `errno` (negativos nas funções que devolvem `isize`).
pub mod errno {
    /// Argumento inválido.
    pub const EINVAL: i32 = 22;
    /// Sem descritores livres / sem recursos.
    pub const EMFILE: i32 = 24;
    /// Não conectado.
    pub const ENOTCONN: i32 = 107;
    /// Conexão reiniciada.
    pub const ECONNRESET: i32 = 104;
    /// Conexão recusada / tempo esgotado.
    pub const ETIMEDOUT: i32 = 110;
    /// Recurso indisponível (E/S).
    pub const EIO: i32 = 5;
    /// Negado pela política (firewall).
    pub const EACCES: i32 = 13;
    /// Nome não resolvido.
    pub const EAI_FAIL: i32 = -1;
}

/// Endereço IPv4 + porta (equivalente a `struct sockaddr_in`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SockAddrIn {
    /// Endereço.
    pub addr: [u8; 4],
    /// Porta (ordem do host).
    pub port: u16,
}

#[derive(Clone, Copy)]
enum Kind {
    Free,
    /// TCP: id da conexão no netd (`u32::MAX` = criado mas não conectado).
    Tcp(u32),
    /// UDP: porta local ligada.
    Udp(u16),
}

/// Tabela de descritores POSIX de um processo, ligada a uma sessão `nexo.sock` (`chan`).
pub struct Sockets {
    chan: Handle,
    fds: [Kind; Self::MAX_FDS],
    next_udp_port: u16,
    buf: [u8; 4096],
    hs: [u32; 1],
}

fn map_remote(code: u32) -> i32 {
    match code {
        1 => errno::EINVAL,
        2 => errno::EMFILE,
        3 => errno::ETIMEDOUT,
        4 => errno::ECONNRESET,
        5 => errno::EAI_FAIL,
        6 => errno::ENOTCONN,
        7 => errno::EACCES,
        _ => errno::EIO,
    }
}

impl Sockets {
    /// Máximo de descritores abertos.
    pub const MAX_FDS: usize = 16;

    /// Cria a tabela sobre o canal `chan` (a ponta do cliente de um `netd`).
    pub fn new(chan: Handle) -> Self {
        Sockets {
            chan,
            fds: [Kind::Free; Self::MAX_FDS],
            next_udp_port: 40500,
            buf: [0; 4096],
            hs: [0; 1],
        }
    }

    fn rpc(&mut self, m: usize) -> Result<usize, i32> {
        if nexo_sys::channel_send(self.chan, &self.buf[..m], &[]) != Status::Ok {
            return Err(errno::EIO);
        }
        match nexo_sys::channel_recv(self.chan, &mut self.buf, &mut self.hs) {
            Ok((n, _)) => Ok(n),
            Err(_) => Err(errno::EIO),
        }
    }

    /// `socket(domain, type)` → descritor, ou `-errno`.
    pub fn socket(&mut self, domain: i32, ty: i32) -> i32 {
        if domain != AF_INET {
            return -errno::EINVAL;
        }
        let Some(fd) = self.fds.iter().position(|k| matches!(k, Kind::Free)) else {
            return -errno::EMFILE;
        };
        match ty {
            SOCK_STREAM => self.fds[fd] = Kind::Tcp(u32::MAX),
            SOCK_DGRAM => {
                let port = self.next_udp_port;
                self.next_udp_port = self.next_udp_port.wrapping_add(1).max(40500);
                self.fds[fd] = Kind::Udp(port);
            }
            _ => return -errno::EINVAL,
        }
        fd as i32
    }

    /// `connect(fd, addr)` (TCP) → 0 ou `-errno`.
    pub fn connect(&mut self, fd: i32, addr: &SockAddrIn) -> i32 {
        if !matches!(self.slot(fd), Some(Kind::Tcp(_))) {
            return -errno::EINVAL;
        }
        let m = TcpConnectRequest {
            dst_ip: addr.addr,
            dst_ip_len: 4,
            dst_port: addr.port,
        }
        .encode_msg(&mut self.buf)
        .unwrap_or(0);
        let n = match self.rpc(m) {
            Ok(n) => n,
            Err(e) => return -e,
        };
        match decode_tcp_connect_response(&self.buf[..n]) {
            Ok(r) => {
                self.fds[fd as usize] = Kind::Tcp(r.conn);
                0
            }
            Err(nexo_proto::ProtoError::Remote(c)) => -map_remote(c),
            Err(_) => -errno::EIO,
        }
    }

    /// `send(fd, buf)` → bytes enviados ou `-errno`.
    pub fn send(&mut self, fd: i32, data: &[u8]) -> isize {
        let conn = match self.slot(fd) {
            Some(Kind::Tcp(c)) if c != u32::MAX => c,
            _ => return -(errno::ENOTCONN as isize),
        };
        let dn = data.len().min(1400);
        let mut req = TcpSendRequest {
            conn,
            data: [0; 1400],
            data_len: dn as u32,
        };
        req.data[..dn].copy_from_slice(&data[..dn]);
        let m = req.encode_msg(&mut self.buf).unwrap_or(0);
        let n = match self.rpc(m) {
            Ok(n) => n,
            Err(e) => return -(e as isize),
        };
        match decode_tcp_send_response(&self.buf[..n]) {
            Ok(r) => r.sent as isize,
            Err(nexo_proto::ProtoError::Remote(c)) => -(map_remote(c) as isize),
            Err(_) => -(errno::EIO as isize),
        }
    }

    /// `recv(fd, out)` (TCP, bloqueante até dados ou fecho) → bytes lidos (0 = fim) ou `-errno`.
    pub fn recv(&mut self, fd: i32, out: &mut [u8]) -> isize {
        let conn = match self.slot(fd) {
            Some(Kind::Tcp(c)) if c != u32::MAX => c,
            _ => return -(errno::ENOTCONN as isize),
        };
        loop {
            let m = TcpRecvRequest { conn }
                .encode_msg(&mut self.buf)
                .unwrap_or(0);
            let n = match self.rpc(m) {
                Ok(n) => n,
                Err(e) => return -(e as isize),
            };
            match decode_tcp_recv_response(&self.buf[..n]) {
                Ok(r) => {
                    let d = r.data();
                    if !d.is_empty() {
                        let k = d.len().min(out.len());
                        out[..k].copy_from_slice(&d[..k]);
                        return k as isize;
                    }
                    if r.closed != 0 {
                        return 0;
                    }
                    nexo_sys::sleep_ns(5_000_000);
                }
                Err(nexo_proto::ProtoError::Remote(c)) => return -(map_remote(c) as isize),
                Err(_) => return -(errno::EIO as isize),
            }
        }
    }

    /// `sendto(fd, buf, dst)` (UDP) → bytes enviados ou `-errno`.
    pub fn sendto(&mut self, fd: i32, data: &[u8], dst: &SockAddrIn) -> isize {
        let port = match self.slot(fd) {
            Some(Kind::Udp(p)) => p,
            _ => return -(errno::EINVAL as isize),
        };
        let dn = data.len().min(1400);
        let mut req = UdpSendRequest {
            dst_ip: dst.addr,
            dst_ip_len: 4,
            dst_port: dst.port,
            src_port: port,
            data: [0; 1400],
            data_len: dn as u32,
        };
        req.data[..dn].copy_from_slice(&data[..dn]);
        let m = req.encode_msg(&mut self.buf).unwrap_or(0);
        let n = match self.rpc(m) {
            Ok(n) => n,
            Err(e) => return -(e as isize),
        };
        match decode_udp_send_response(&self.buf[..n]) {
            Ok(_) => dn as isize,
            Err(nexo_proto::ProtoError::Remote(c)) => -(map_remote(c) as isize),
            Err(_) => -(errno::EIO as isize),
        }
    }

    /// `recvfrom(fd, out)` (UDP, não bloqueante) → (bytes, origem) ou `-errno`; 0 bytes = nada.
    pub fn recvfrom(&mut self, fd: i32, out: &mut [u8]) -> Result<(usize, SockAddrIn), i32> {
        let port = match self.slot(fd) {
            Some(Kind::Udp(p)) => p,
            _ => return Err(errno::EINVAL),
        };
        let m = UdpRecvRequest { port }
            .encode_msg(&mut self.buf)
            .unwrap_or(0);
        let n = self.rpc(m)?;
        match decode_udp_recv_response(&self.buf[..n]) {
            Ok(r) => {
                let d = r.data();
                let k = d.len().min(out.len());
                out[..k].copy_from_slice(&d[..k]);
                let mut ip = [0u8; 4];
                if r.from_ip().len() == 4 {
                    ip.copy_from_slice(r.from_ip());
                }
                Ok((
                    k,
                    SockAddrIn {
                        addr: ip,
                        port: r.from_port,
                    },
                ))
            }
            Err(nexo_proto::ProtoError::Remote(c)) => Err(map_remote(c)),
            Err(_) => Err(errno::EIO),
        }
    }

    /// `close(fd)` → 0 ou `-errno`.
    pub fn close(&mut self, fd: i32) -> i32 {
        match self.slot(fd) {
            Some(Kind::Tcp(c)) if c != u32::MAX => {
                let m = TcpCloseRequest { conn: c }
                    .encode_msg(&mut self.buf)
                    .unwrap_or(0);
                let _ = self.rpc(m).and_then(|n| {
                    decode_tcp_close_response(&self.buf[..n]).map_err(|_| errno::EIO)
                });
            }
            Some(Kind::Free) | None => return -errno::EINVAL,
            _ => {}
        }
        self.fds[fd as usize] = Kind::Free;
        0
    }

    /// `getaddrinfo`-like: resolve `name` (A) usando o DNS do `netd`; devolve o endereço ou `-errno`.
    pub fn getaddrinfo(&mut self, name: &[u8]) -> Result<[u8; 4], i32> {
        if name.is_empty() || name.len() > 253 {
            return Err(errno::EINVAL);
        }
        let mut req = ResolveRequest {
            name: [0; 253],
            name_len: name.len() as u32,
        };
        req.name[..name.len()].copy_from_slice(name);
        let m = req.encode_msg(&mut self.buf).unwrap_or(0);
        let n = self.rpc(m)?;
        match decode_resolve_response(&self.buf[..n]) {
            Ok(r) if r.addr().len() == 4 => {
                let mut a = [0u8; 4];
                a.copy_from_slice(r.addr());
                Ok(a)
            }
            Ok(_) => Err(errno::EIO),
            Err(nexo_proto::ProtoError::Remote(c)) => Err(map_remote(c)),
            Err(_) => Err(errno::EIO),
        }
    }

    fn slot(&self, fd: i32) -> Option<Kind> {
        if fd < 0 || fd as usize >= Self::MAX_FDS {
            return None;
        }
        Some(self.fds[fd as usize])
    }

    /// Suprime avisos de `sock::` não usados em builds mínimos.
    #[doc(hidden)]
    pub fn _touch() {
        let _ = sock::InfoRequest {};
    }
}
