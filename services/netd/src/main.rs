//! `netd` — serviço de rede residente. Handle 0 = canal do `netdev` (`nexo.net`),
//! handle 1 = canal do cliente (`nexo.sock`, `idl/sock.idl`).
//!
//! No arranque: MAC → DHCP (DISCOVER→OFFER→REQUEST→ACK) → ARP do gateway. Depois, um laço de
//! eventos único bombeia quadros do driver (ARP, UDP para filas por porta, TCP para a máquina
//! de estados de `docs/spec/tcp-states.md`) e atende pedidos do cliente sem bloquear
//! (`channel_try_recv`). Limitações documentadas: sem retransmissão própria (o par retransmite;
//! segmentos fora de ordem são descartados sem ACK), TIME_WAIT imediato, um cliente.
#![no_std]
#![no_main]

use nexo_netstack as nsk;
use nexo_proto::net::{self as pnet, RecvRequest, SendRequest};
use nexo_proto::sock::{self, Request};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const NET: Handle = 0;
const CLIENT: Handle = 1;

const E_INVALID: u32 = 1;
const E_NO_RES: u32 = 2;
const E_TIMEOUT: u32 = 3;
const E_RESET: u32 = 4;
const E_NOT_FOUND: u32 = 5;
const E_NOT_CONN: u32 = 6;

// ---- estado global (uma thread; padrao addr_of_mut como no vfs) ----
const UDP_SOCKS: usize = 4;
const UDP_QUEUE: usize = 4;
const UDP_MAX: usize = 1400;
const TCP_CONNS: usize = 4;
const DNS_CACHE: usize = 8;

struct UdpSock {
    used: bool,
    port: u16,
    queue: [([u8; 4], u16, u16, [u8; UDP_MAX]); UDP_QUEUE], // (ip, porta, len, dados)
    qlen: usize,
}

struct Netd {
    mac: [u8; 6],
    gw_mac: [u8; 6],
    lease: nsk::DhcpLease,
    udp: [UdpSock; UDP_SOCKS],
    tcp: [Option<nsk::tcp::TcpSocket>; TCP_CONNS],
    dns: [([u8; 64], u8, [u8; 4]); DNS_CACHE],
    dns_len: usize,
}

static mut NETD: Netd = Netd {
    mac: [0; 6],
    gw_mac: [0; 6],
    lease: nsk::DhcpLease {
        ip: [0; 4],
        server: [0; 4],
        mask: [0; 4],
        router: [0; 4],
        dns: [0; 4],
    },
    udp: [const {
        UdpSock {
            used: false,
            port: 0,
            queue: [([0; 4], 0, 0, [0; UDP_MAX]); UDP_QUEUE],
            qlen: 0,
        }
    }; UDP_SOCKS],
    tcp: [const { None }; TCP_CONNS],
    dns: [([0; 64], 0, [0; 4]); DNS_CACHE],
    dns_len: 0,
};

fn netd() -> &'static mut Netd {
    // SAFETY: processo com uma única thread; nenhuma reentrância.
    unsafe { &mut *core::ptr::addr_of_mut!(NETD) }
}

fn fail(code: i64, what: &str) -> ! {
    log!("netd: falha: {}", what);
    nexo_sys::exit(code)
}

// ---- E/S com o netdev (nexo.net) ----

fn net_send(frame: &[u8]) {
    let mut msg = [0u8; 4096];
    let mut sr = SendRequest {
        frame: [0; 1514],
        frame_len: frame.len().min(1514) as u32,
    };
    sr.frame[..frame.len().min(1514)].copy_from_slice(&frame[..frame.len().min(1514)]);
    let m = sr.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(NET, &msg[..m], &[]) != Status::Ok {
        fail(100, "send ao netdev");
    }
    let mut hs = [0u32; 1];
    match nexo_sys::channel_recv(NET, &mut msg, &mut hs) {
        Ok((n, _)) if pnet::decode_send_response(&msg[..n]).is_ok() => {}
        _ => fail(101, "resposta do netdev"),
    }
}

/// Lê um quadro do driver; devolve o tamanho (0 = nada).
fn net_recv(frame: &mut [u8; 1514]) -> usize {
    let mut msg = [0u8; 4096];
    let m = RecvRequest {}.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(NET, &msg[..m], &[]) != Status::Ok {
        fail(102, "recv ao netdev");
    }
    let mut hs = [0u32; 1];
    match nexo_sys::channel_recv(NET, &mut msg, &mut hs) {
        Ok((n, _)) => match pnet::decode_recv_response(&msg[..n]) {
            Ok(r) => {
                let l = r.frame().len();
                frame[..l].copy_from_slice(r.frame());
                l
            }
            Err(_) => 0,
        },
        _ => fail(103, "resposta do netdev"),
    }
}

// ---- TCP: a maquina de estados vive em nexo_netstack::tcp (testavel no host);
// aqui so montamos/enviamos os quadros e bombeamos o temporizador de retransmissao ----

/// Emite um segmento de `sock` (payload é copiado antes para evitar reemprestimos).
fn emit_seg(sock: &nsk::tcp::TcpSocket, seg: nsk::tcp::TxSeg) {
    let st = netd();
    let (mac, gw, ip) = (st.mac, st.gw_mac, st.lease.ip);
    let mut payload = [0u8; nsk::tcp::MSS];
    let pl = seg.payload_len.min(nsk::tcp::MSS);
    payload[..pl].copy_from_slice(&sock.tx_payload()[..pl]);
    let mut frame = [0u8; 1514];
    let n = nsk::tcp_write(
        &mut frame,
        mac,
        gw,
        ip,
        sock.remote_ip,
        sock.local_port,
        sock.remote_port,
        seg.seq,
        seg.ack,
        seg.flags,
        sock.window(),
        &payload[..pl],
    );
    net_send(&frame[..n]);
}

fn handle_tcp(frame: &[u8]) {
    let now = nexo_sys::time_now();
    let st = netd();
    for slot in st.tcp.iter_mut() {
        let Some(sock) = slot else { continue };
        let Some(seg) = nsk::tcp_parse(frame, sock.remote_ip, sock.local_port) else {
            continue;
        };
        if seg.src_port != sock.remote_port {
            continue;
        }
        if let Some(out) = sock.on_segment(&seg, now) {
            emit_seg(sock, out);
        }
        return;
    }
}

/// Retransmissoes devidas (RTO/MAX_RETRIES de nexo_netstack::tcp).
fn tcp_timers() {
    let now = nexo_sys::time_now();
    let st = netd();
    for slot in st.tcp.iter_mut() {
        if let Some(sock) = slot
            && let Some(out) = sock.poll(now)
        {
            emit_seg(sock, out);
        }
    }
}

// ---- bomba de quadros ----

fn pump() {
    tcp_timers();
    let st = netd();
    let mut frame = [0u8; 1514];
    loop {
        let n = net_recv(&mut frame);
        if n == 0 {
            return;
        }
        let f = &frame[..n];
        // ARP: responde pedidos pelo nosso IP e aprende o MAC do gateway
        if let Some((_, _, et, p)) = nsk::eth_parse(f) {
            if et == nsk::ETHERTYPE_ARP && p.len() >= 28 {
                let op = u16::from_be_bytes([p[6], p[7]]);
                if op == 1 && p[24..28] == st.lease.ip {
                    let mut req_mac = [0u8; 6];
                    req_mac.copy_from_slice(&p[8..14]);
                    let mut req_ip = [0u8; 4];
                    req_ip.copy_from_slice(&p[14..18]);
                    let mut reply = [0u8; 64];
                    let o = nsk::eth_write(&mut reply, req_mac, st.mac, nsk::ETHERTYPE_ARP);
                    let a = &mut reply[o..];
                    a[0..2].copy_from_slice(&1u16.to_be_bytes());
                    a[2..4].copy_from_slice(&nsk::ETHERTYPE_IPV4.to_be_bytes());
                    a[4] = 6;
                    a[5] = 4;
                    a[6..8].copy_from_slice(&2u16.to_be_bytes());
                    a[8..14].copy_from_slice(&st.mac);
                    a[14..18].copy_from_slice(&st.lease.ip);
                    a[18..24].copy_from_slice(&req_mac);
                    a[24..28].copy_from_slice(&req_ip);
                    net_send(&reply[..o + 28]);
                    continue;
                }
            }
            if let Some(m) = nsk::arp_reply_from(f, st.lease.router) {
                st.gw_mac = m;
                continue;
            }
            if et == nsk::ETHERTYPE_IPV4
                && let Some(ip) = nsk::ipv4_parse(p)
            {
                if ip.proto == nsk::IPPROTO_UDP {
                    if let Some((sport, dport, data)) = nsk::udp_parse(ip.payload) {
                        for u in st.udp.iter_mut() {
                            if u.used && u.port == dport {
                                if u.qlen < UDP_QUEUE && data.len() <= UDP_MAX {
                                    let q = &mut u.queue[u.qlen];
                                    q.0 = ip.src;
                                    q.1 = sport;
                                    q.2 = data.len() as u16;
                                    q.3[..data.len()].copy_from_slice(data);
                                    u.qlen += 1;
                                }
                                break;
                            }
                        }
                    }
                    continue;
                }
                if ip.proto == nsk::IPPROTO_TCP {
                    handle_tcp(f);
                    continue;
                }
            }
        }
    }
}

fn udp_bind(port: u16) -> Option<usize> {
    let st = netd();
    if let Some(i) = (0..UDP_SOCKS).find(|&i| st.udp[i].used && st.udp[i].port == port) {
        return Some(i);
    }
    let i = (0..UDP_SOCKS).find(|&i| !st.udp[i].used)?;
    st.udp[i].used = true;
    st.udp[i].port = port;
    st.udp[i].qlen = 0;
    Some(i)
}

fn udp_emit(dst_ip: [u8; 4], dst_port: u16, src_port: u16, data: &[u8]) {
    let st = netd();
    let mut frame = [0u8; 1514];
    let o = nsk::eth_write(&mut frame, st.gw_mac, st.mac, nsk::ETHERTYPE_IPV4);
    let po = nsk::ipv4_write(
        &mut frame,
        o,
        st.lease.ip,
        dst_ip,
        nsk::IPPROTO_UDP,
        8 + data.len(),
        64,
        src_port,
    );
    let bo = nsk::udp_write(&mut frame, po, src_port, dst_port, data.len());
    frame[bo..bo + data.len()].copy_from_slice(data);
    net_send(&frame[..bo + data.len()]);
}

/// Resolve `name` com cache; devolve (addr, veio_do_cache) ou erro.
fn resolve(name: &[u8]) -> Result<([u8; 4], bool), u32> {
    let st = netd();
    for (n, l, a) in st.dns.iter().take(st.dns_len) {
        if &n[..*l as usize] == name {
            return Ok((*a, true));
        }
    }
    let id = (nexo_sys::time_now() & 0xffff) as u16 | 1;
    let mut frame = [0u8; 1514];
    let n = nsk::dns_query(
        &mut frame,
        st.mac,
        st.gw_mac,
        st.lease.ip,
        st.lease.dns,
        40000,
        id,
        name,
    )
    .ok_or(E_INVALID)?;
    net_send(&frame[..n]);
    let start = nexo_sys::time_now();
    let mut rx = [0u8; 1514];
    loop {
        let fl = net_recv(&mut rx);
        if fl > 0 {
            if let Some(ans) = nsk::dns_parse(&rx[..fl], st.lease.dns, 40000, id) {
                let addr = ans.a.ok_or(E_NOT_FOUND)?;
                if name.len() <= 64 {
                    let slot = st.dns_len % DNS_CACHE;
                    st.dns[slot].0[..name.len()].copy_from_slice(name);
                    st.dns[slot].1 = name.len() as u8;
                    st.dns[slot].2 = addr;
                    st.dns_len = (st.dns_len + 1).min(DNS_CACHE).max(slot + 1);
                }
                return Ok((addr, false));
            }
            // outros quadros seguem o fluxo normal
            let f = rx;
            if let Some((_, _, et, p)) = nsk::eth_parse(&f[..fl])
                && et == nsk::ETHERTYPE_IPV4
                && let Some(ip) = nsk::ipv4_parse(p)
                && ip.proto == nsk::IPPROTO_TCP
            {
                handle_tcp(&f[..fl]);
            }
        } else {
            nexo_sys::sleep_ns(5_000_000);
        }
        if nexo_sys::time_now() - start > 10_000_000_000 {
            return Err(E_TIMEOUT);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    // 1. MAC do driver
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    let m = pnet::MacRequest {}.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(NET, &msg[..m], &[]) != Status::Ok {
        fail(104, "mac");
    }
    let mac = match nexo_sys::channel_recv(NET, &mut msg, &mut hs) {
        Ok((n, _)) => match pnet::decode_mac_response(&msg[..n]) {
            Ok(r) if r.addr().len() == 6 => {
                let mut m6 = [0u8; 6];
                m6.copy_from_slice(r.addr());
                m6
            }
            _ => fail(105, "mac"),
        },
        _ => fail(105, "mac"),
    };
    netd().mac = mac;
    // 2. DHCP
    let xid = 0x4e58_00d4u32;
    let mut frame = [0u8; 1514];
    let n = nsk::dhcp_build(&mut frame, mac, xid, None);
    net_send(&frame[..n]);
    let offer = wait_dhcp(2, xid);
    let n = nsk::dhcp_build(&mut frame, mac, xid, Some((offer.ip, offer.server)));
    net_send(&frame[..n]);
    let lease = wait_dhcp(5, xid);
    netd().lease = lease;
    // 3. ARP do gateway
    let n = nsk::arp_request(&mut frame, mac, lease.ip, lease.router);
    net_send(&frame[..n]);
    let start = nexo_sys::time_now();
    let mut rx = [0u8; 1514];
    loop {
        let fl = net_recv(&mut rx);
        if fl > 0
            && let Some(m) = nsk::arp_reply_from(&rx[..fl], lease.router)
        {
            netd().gw_mac = m;
            break;
        }
        if nexo_sys::time_now() - start > 10_000_000_000 {
            fail(106, "ARP do gateway");
        }
        nexo_sys::sleep_ns(5_000_000);
    }
    log!(
        "netd: pronto — ip {}.{}.{}.{} gw {}.{}.{}.{} dns {}.{}.{}.{}",
        lease.ip[0],
        lease.ip[1],
        lease.ip[2],
        lease.ip[3],
        lease.router[0],
        lease.router[1],
        lease.router[2],
        lease.router[3],
        lease.dns[0],
        lease.dns[1],
        lease.dns[2],
        lease.dns[3]
    );
    // 4. laço de eventos
    let mut out = [0u8; 4096];
    loop {
        pump();
        let (n, _) = match nexo_sys::channel_try_recv(CLIENT, &mut msg, &mut hs) {
            Ok(v) => v,
            Err(Status::WouldBlock) => {
                nexo_sys::sleep_ns(2_000_000);
                continue;
            }
            Err(Status::PeerClosed) => {
                log!("netd: cliente desconectou; encerrando");
                nexo_sys::exit(0)
            }
            Err(_) => fail(107, "recv do cliente"),
        };
        let request = match sock::decode_request(&msg[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = sock::encode_error(0, E_INVALID, &mut out).unwrap_or(0);
                let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
                continue;
            }
        };
        let m = serve(request, &mut out);
        if nexo_sys::channel_send(CLIENT, &out[..m], &[]) != Status::Ok {
            fail(108, "resposta ao cliente");
        }
    }
}

fn wait_dhcp(kind: u8, xid: u32) -> nsk::DhcpLease {
    let start = nexo_sys::time_now();
    let mut rx = [0u8; 1514];
    loop {
        let fl = net_recv(&mut rx);
        if fl > 0
            && let Some((k, lease)) = nsk::dhcp_parse(&rx[..fl], xid)
            && k == kind
        {
            return lease;
        }
        if nexo_sys::time_now() - start > 15_000_000_000 {
            fail(109, "DHCP sem resposta");
        }
        if fl == 0 {
            nexo_sys::sleep_ns(5_000_000);
        }
    }
}

fn serve(request: Request, out: &mut [u8; 4096]) -> usize {
    let st = netd();
    match request {
        Request::Info(_) => {
            let l = st.lease;
            sock::InfoResponse {
                ip: l.ip,
                ip_len: 4,
                mask: l.mask,
                mask_len: 4,
                gateway: l.router,
                gateway_len: 4,
                dns: l.dns,
                dns_len: 4,
                mac: st.mac,
                mac_len: 6,
            }
            .encode_msg(out)
            .unwrap_or(0)
        }
        Request::Resolve(rq) => match resolve(rq.name()) {
            Ok((addr, cached)) => sock::ResolveResponse {
                addr,
                addr_len: 4,
                cached: cached as u8,
            }
            .encode_msg(out)
            .unwrap_or(0),
            Err(e) => sock::encode_error(sock::ResolveRequest::METHOD_ID, e, out).unwrap_or(0),
        },
        Request::UdpSend(rq) => {
            if rq.dst_ip().len() != 4 || rq.data().is_empty() {
                return sock::encode_error(sock::UdpSendRequest::METHOD_ID, E_INVALID, out)
                    .unwrap_or(0);
            }
            if udp_bind(rq.src_port).is_none() {
                return sock::encode_error(sock::UdpSendRequest::METHOD_ID, E_NO_RES, out)
                    .unwrap_or(0);
            }
            let mut dst = [0u8; 4];
            dst.copy_from_slice(rq.dst_ip());
            udp_emit(dst, rq.dst_port, rq.src_port, rq.data());
            sock::UdpSendResponse {}.encode_msg(out).unwrap_or(0)
        }
        Request::UdpRecv(rq) => {
            let Some(i) = udp_bind(rq.port) else {
                return sock::encode_error(sock::UdpRecvRequest::METHOD_ID, E_NO_RES, out)
                    .unwrap_or(0);
            };
            let u = &mut st.udp[i];
            let mut resp = sock::UdpRecvResponse {
                from_ip: [0; 4],
                from_ip_len: 0,
                from_port: 0,
                data: [0; 1400],
                data_len: 0,
            };
            if u.qlen > 0 {
                let (ip, port, len, data) = u.queue[0];
                resp.from_ip = ip;
                resp.from_ip_len = 4;
                resp.from_port = port;
                resp.data[..len as usize].copy_from_slice(&data[..len as usize]);
                resp.data_len = len as u32;
                u.queue.copy_within(1..u.qlen, 0);
                u.qlen -= 1;
            }
            resp.encode_msg(out).unwrap_or(0)
        }
        Request::TcpConnect(rq) => {
            if rq.dst_ip().len() != 4 {
                return sock::encode_error(sock::TcpConnectRequest::METHOD_ID, E_INVALID, out)
                    .unwrap_or(0);
            }
            let Some(i) = (0..TCP_CONNS).find(|&i| {
                st.tcp[i].is_none()
                    || st.tcp[i]
                        .as_ref()
                        .is_some_and(|c| c.state == nsk::tcp::State::Closed)
            }) else {
                return sock::encode_error(sock::TcpConnectRequest::METHOD_ID, E_NO_RES, out)
                    .unwrap_or(0);
            };
            let mut dst = [0u8; 4];
            dst.copy_from_slice(rq.dst_ip());
            let now = nexo_sys::time_now();
            let iss = (now as u32) | 1;
            let (sock_new, syn) =
                nsk::tcp::TcpSocket::connect(41000 + i as u16, dst, rq.dst_port, iss, now);
            emit_seg(&sock_new, syn);
            st.tcp[i] = Some(sock_new);
            let start = nexo_sys::time_now();
            loop {
                pump();
                let state = st.tcp[i].as_ref().map(|c| (c.state, c.reset));
                match state {
                    Some((nsk::tcp::State::Established, _)) => {
                        break sock::TcpConnectResponse { conn: i as u32 }
                            .encode_msg(out)
                            .unwrap_or(0);
                    }
                    Some((nsk::tcp::State::Closed, _)) | None => {
                        st.tcp[i] = None;
                        break sock::encode_error(sock::TcpConnectRequest::METHOD_ID, E_RESET, out)
                            .unwrap_or(0);
                    }
                    _ => {}
                }
                if nexo_sys::time_now() - start > 10_000_000_000 {
                    st.tcp[i] = None;
                    break sock::encode_error(sock::TcpConnectRequest::METHOD_ID, E_TIMEOUT, out)
                        .unwrap_or(0);
                }
                nexo_sys::sleep_ns(2_000_000);
            }
        }
        Request::TcpSend(rq) => {
            let i = rq.conn as usize;
            if i >= TCP_CONNS || st.tcp[i].is_none() {
                return sock::encode_error(sock::TcpSendRequest::METHOD_ID, E_INVALID, out)
                    .unwrap_or(0);
            }
            let data = rq.data();
            let start = nexo_sys::time_now();
            loop {
                let now = nexo_sys::time_now();
                let r = st.tcp[i].as_mut().map(|c| c.send(data, now));
                match r {
                    Some(Ok(seg)) => {
                        let sock = st.tcp[i].as_ref().unwrap();
                        emit_seg(sock, seg);
                        break sock::TcpSendResponse {
                            sent: data.len() as u32,
                        }
                        .encode_msg(out)
                        .unwrap_or(0);
                    }
                    Some(Err(true)) => {
                        // pendencia anterior ainda sem ACK: bombeia e tenta de novo
                        pump();
                        if nexo_sys::time_now() - start > 5_000_000_000 {
                            break sock::encode_error(
                                sock::TcpSendRequest::METHOD_ID,
                                E_TIMEOUT,
                                out,
                            )
                            .unwrap_or(0);
                        }
                        nexo_sys::sleep_ns(2_000_000);
                    }
                    _ => {
                        let e = if st.tcp[i].as_ref().is_some_and(|c| c.reset) {
                            E_RESET
                        } else {
                            E_NOT_CONN
                        };
                        break sock::encode_error(sock::TcpSendRequest::METHOD_ID, e, out)
                            .unwrap_or(0);
                    }
                }
            }
        }
        Request::TcpRecv(rq) => {
            let i = rq.conn as usize;
            if i >= TCP_CONNS || st.tcp[i].is_none() {
                return sock::encode_error(sock::TcpRecvRequest::METHOD_ID, E_INVALID, out)
                    .unwrap_or(0);
            }
            let c = st.tcp[i].as_mut().unwrap();
            let mut resp = sock::TcpRecvResponse {
                data: [0; 1400],
                data_len: 0,
                closed: (c.peer_closed || c.reset || c.state == nsk::tcp::State::Closed) as u8,
            };
            resp.data_len = c.take_rx(&mut resp.data) as u32;
            resp.encode_msg(out).unwrap_or(0)
        }
        Request::TcpClose(rq) => {
            let i = rq.conn as usize;
            if i >= TCP_CONNS || st.tcp[i].is_none() {
                return sock::encode_error(sock::TcpCloseRequest::METHOD_ID, E_INVALID, out)
                    .unwrap_or(0);
            }
            let was_reset = st.tcp[i].as_ref().is_some_and(|c| c.reset);
            let start = nexo_sys::time_now();
            // espera a pendencia anterior, emite o FIN e acompanha o fecho (melhor esforco)
            loop {
                pump();
                let now = nexo_sys::time_now();
                let (state, fin) = {
                    let c = st.tcp[i].as_mut().unwrap();
                    (c.state, c.close(now))
                };
                if let Some(f) = fin {
                    let sock = st.tcp[i].as_ref().unwrap();
                    emit_seg(sock, f);
                } else if state == nsk::tcp::State::Closed {
                    break;
                }
                if nexo_sys::time_now() - start > 3_000_000_000 {
                    break;
                }
                nexo_sys::sleep_ns(2_000_000);
            }
            st.tcp[i] = None;
            if was_reset {
                sock::encode_error(sock::TcpCloseRequest::METHOD_ID, E_RESET, out).unwrap_or(0)
            } else {
                sock::TcpCloseResponse {}.encode_msg(out).unwrap_or(0)
            }
        }
    }
}
