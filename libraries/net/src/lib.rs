//! Camadas básicas de rede (Fase 4): montagem e leitura de quadros **Ethernet**, **ARP**,
//! **IPv4** (com checksum) e **ICMP echo** — sem alocação; buffers do chamador. Usada pelos
//! clientes do `netdev` (`nexo.net`) e testável no host (incl. fuzz-lite).
#![no_std]
#![forbid(unsafe_code)]

/// Endereço MAC.
pub type Mac = [u8; 6];
/// Endereço IPv4.
pub type Ipv4Addr = [u8; 4];

/// EtherType ARP.
pub const ETHERTYPE_ARP: u16 = 0x0806;
/// EtherType IPv4.
pub const ETHERTYPE_IPV4: u16 = 0x0800;
/// Protocolo ICMP.
pub const IPPROTO_ICMP: u8 = 1;
/// Protocolo UDP.
pub const IPPROTO_UDP: u8 = 17;
/// Cabeçalho Ethernet.
pub const ETH_HLEN: usize = 14;
/// Cabeçalho IPv4 mínimo.
pub const IPV4_HLEN: usize = 20;

fn be16(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}

/// Escreve o cabeçalho Ethernet; devolve o offset do payload.
pub fn eth_write(frame: &mut [u8], dst: Mac, src: Mac, ethertype: u16) -> usize {
    frame[0..6].copy_from_slice(&dst);
    frame[6..12].copy_from_slice(&src);
    frame[12..14].copy_from_slice(&ethertype.to_be_bytes());
    ETH_HLEN
}

/// Lê o cabeçalho Ethernet: (destino, origem, ethertype, payload).
pub fn eth_parse(frame: &[u8]) -> Option<(Mac, Mac, u16, &[u8])> {
    if frame.len() < ETH_HLEN {
        return None;
    }
    let mut dst = [0u8; 6];
    let mut src = [0u8; 6];
    dst.copy_from_slice(&frame[0..6]);
    src.copy_from_slice(&frame[6..12]);
    Some((dst, src, be16(frame, 12), &frame[ETH_HLEN..]))
}

/// Monta um ARP request "quem tem `target_ip`?"; devolve o tamanho do quadro.
pub fn arp_request(frame: &mut [u8], src_mac: Mac, src_ip: Ipv4Addr, target_ip: Ipv4Addr) -> usize {
    let o = eth_write(frame, [0xff; 6], src_mac, ETHERTYPE_ARP);
    let a = &mut frame[o..];
    a[0..2].copy_from_slice(&1u16.to_be_bytes()); // Ethernet
    a[2..4].copy_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    a[4] = 6;
    a[5] = 4;
    a[6..8].copy_from_slice(&1u16.to_be_bytes()); // request
    a[8..14].copy_from_slice(&src_mac);
    a[14..18].copy_from_slice(&src_ip);
    a[18..24].copy_from_slice(&[0; 6]);
    a[24..28].copy_from_slice(&target_ip);
    o + 28
}

/// Se `frame` é um ARP reply de `from_ip`, devolve o MAC anunciado.
pub fn arp_reply_from(frame: &[u8], from_ip: Ipv4Addr) -> Option<Mac> {
    let (_, _, et, p) = eth_parse(frame)?;
    if et != ETHERTYPE_ARP || p.len() < 28 || be16(p, 6) != 2 || p[14..18] != from_ip {
        return None;
    }
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&p[8..14]);
    Some(mac)
}

/// Checksum de Internet (RFC 1071) sobre `data`.
pub fn inet_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let (pairs, rest) = data.as_chunks::<2>();
    for c in pairs {
        sum += u16::from_be_bytes(*c) as u32;
    }
    if let [last] = rest {
        sum += (*last as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Escreve um cabeçalho IPv4 (sem opções); devolve o offset do payload no quadro.
#[allow(clippy::too_many_arguments)]
pub fn ipv4_write(
    frame: &mut [u8],
    eth_payload_off: usize,
    src: Ipv4Addr,
    dst: Ipv4Addr,
    proto: u8,
    payload_len: usize,
    ttl: u8,
    ident: u16,
) -> usize {
    let total = (IPV4_HLEN + payload_len) as u16;
    let h = &mut frame[eth_payload_off..eth_payload_off + IPV4_HLEN];
    h[0] = 0x45;
    h[1] = 0;
    h[2..4].copy_from_slice(&total.to_be_bytes());
    h[4..6].copy_from_slice(&ident.to_be_bytes());
    h[6..8].copy_from_slice(&0u16.to_be_bytes()); // sem fragmentação
    h[8] = ttl;
    h[9] = proto;
    h[10..12].copy_from_slice(&0u16.to_be_bytes());
    h[12..16].copy_from_slice(&src);
    h[16..20].copy_from_slice(&dst);
    let ck = inet_checksum(&frame[eth_payload_off..eth_payload_off + IPV4_HLEN]);
    frame[eth_payload_off + 10..eth_payload_off + 12].copy_from_slice(&ck.to_be_bytes());
    eth_payload_off + IPV4_HLEN
}

/// Pacote IPv4 decodificado (sem fragmentação).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4<'a> {
    /// Origem.
    pub src: Ipv4Addr,
    /// Destino.
    pub dst: Ipv4Addr,
    /// Protocolo.
    pub proto: u8,
    /// TTL restante.
    pub ttl: u8,
    /// Payload (após o cabeçalho, limitado ao tamanho declarado).
    pub payload: &'a [u8],
}

/// Lê um cabeçalho IPv4 validando versão, IHL, tamanho e checksum.
pub fn ipv4_parse(p: &[u8]) -> Option<Ipv4<'_>> {
    if p.len() < IPV4_HLEN || p[0] >> 4 != 4 {
        return None;
    }
    let ihl = ((p[0] & 0xf) as usize) * 4;
    if ihl < IPV4_HLEN || p.len() < ihl {
        return None;
    }
    let total = be16(p, 2) as usize;
    if total < ihl || total > p.len() {
        return None;
    }
    if inet_checksum(&p[..ihl]) != 0 {
        return None;
    }
    let mut src = [0u8; 4];
    let mut dst = [0u8; 4];
    src.copy_from_slice(&p[12..16]);
    dst.copy_from_slice(&p[16..20]);
    Some(Ipv4 {
        src,
        dst,
        proto: p[9],
        ttl: p[8],
        payload: &p[ihl..total],
    })
}

/// Monta um ICMP echo request completo (Ethernet + IPv4 + ICMP); devolve o tamanho do quadro.
#[allow(clippy::too_many_arguments)]
pub fn icmp_echo_request(
    frame: &mut [u8],
    src_mac: Mac,
    dst_mac: Mac,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    ident: u16,
    seq: u16,
    data: &[u8],
) -> usize {
    let o = eth_write(frame, dst_mac, src_mac, ETHERTYPE_IPV4);
    let icmp_len = 8 + data.len();
    let po = ipv4_write(frame, o, src_ip, dst_ip, IPPROTO_ICMP, icmp_len, 64, ident);
    let icmp = &mut frame[po..po + icmp_len];
    icmp[0] = 8; // echo request
    icmp[1] = 0;
    icmp[2..4].copy_from_slice(&0u16.to_be_bytes());
    icmp[4..6].copy_from_slice(&ident.to_be_bytes());
    icmp[6..8].copy_from_slice(&seq.to_be_bytes());
    icmp[8..].copy_from_slice(data);
    let ck = inet_checksum(&frame[po..po + icmp_len]);
    frame[po + 2..po + 4].copy_from_slice(&ck.to_be_bytes());
    po + icmp_len
}

/// Se `frame` é um ICMP echo reply de `from_ip` com (`ident`, `seq`), devolve (ttl, dados).
pub fn icmp_echo_reply(
    frame: &[u8],
    from_ip: Ipv4Addr,
    ident: u16,
    seq: u16,
) -> Option<(u8, &[u8])> {
    let (_, _, et, p) = eth_parse(frame)?;
    if et != ETHERTYPE_IPV4 {
        return None;
    }
    let ip = ipv4_parse(p)?;
    if ip.proto != IPPROTO_ICMP || ip.src != from_ip {
        return None;
    }
    let icmp = ip.payload;
    if icmp.len() < 8 || icmp[0] != 0 || inet_checksum(icmp) != 0 {
        return None;
    }
    if be16(icmp, 4) != ident || be16(icmp, 6) != seq {
        return None;
    }
    Some((ip.ttl, &icmp[8..]))
}

/// Porta do servidor DHCP.
pub const DHCP_SERVER_PORT: u16 = 67;
/// Porta do cliente DHCP.
pub const DHCP_CLIENT_PORT: u16 = 68;

/// Escreve um cabeçalho UDP (checksum 0 = ausente, permitido no IPv4); devolve o offset do payload.
pub fn udp_write(
    frame: &mut [u8],
    ip_payload_off: usize,
    src_port: u16,
    dst_port: u16,
    payload_len: usize,
) -> usize {
    let h = &mut frame[ip_payload_off..ip_payload_off + 8];
    h[0..2].copy_from_slice(&src_port.to_be_bytes());
    h[2..4].copy_from_slice(&dst_port.to_be_bytes());
    h[4..6].copy_from_slice(&((8 + payload_len) as u16).to_be_bytes());
    h[6..8].copy_from_slice(&0u16.to_be_bytes());
    ip_payload_off + 8
}

/// Lê um cabeçalho UDP: (porta origem, porta destino, payload).
pub fn udp_parse(p: &[u8]) -> Option<(u16, u16, &[u8])> {
    if p.len() < 8 {
        return None;
    }
    let len = be16(p, 4) as usize;
    if len < 8 || len > p.len() {
        return None;
    }
    Some((be16(p, 0), be16(p, 2), &p[8..len]))
}

/// Lease devolvido pelo DHCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DhcpLease {
    /// Endereço oferecido/confirmado.
    pub ip: Ipv4Addr,
    /// Servidor DHCP (opção 54).
    pub server: Ipv4Addr,
    /// Máscara (opção 1), se veio.
    pub mask: Ipv4Addr,
    /// Gateway (opção 3), se veio.
    pub router: Ipv4Addr,
    /// DNS (opção 6, primeiro endereço), se veio.
    pub dns: Ipv4Addr,
}

/// Monta um DHCPDISCOVER (ou DHCPREQUEST, se `request_ip`/`server` vierem) em broadcast;
/// devolve o tamanho do quadro.
pub fn dhcp_build(
    frame: &mut [u8],
    src_mac: Mac,
    xid: u32,
    request: Option<(Ipv4Addr, Ipv4Addr)>,
) -> usize {
    let o = eth_write(frame, [0xff; 6], src_mac, ETHERTYPE_IPV4);
    // BOOTP fixo (236 B) + magic (4) + opções (~18)
    let mut opts = [0u8; 32];
    let mut ol = 0usize;
    opts[ol] = 53; // message type
    opts[ol + 1] = 1;
    opts[ol + 2] = if request.is_some() { 3 } else { 1 }; // REQUEST : DISCOVER
    ol += 3;
    if let Some((ip, server)) = request {
        opts[ol] = 50; // requested IP
        opts[ol + 1] = 4;
        opts[ol + 2..ol + 6].copy_from_slice(&ip);
        ol += 6;
        opts[ol] = 54; // server id
        opts[ol + 1] = 4;
        opts[ol + 2..ol + 6].copy_from_slice(&server);
        ol += 6;
    }
    opts[ol] = 55; // parameter request list: mask, router, dns
    opts[ol + 1] = 3;
    opts[ol + 2] = 1;
    opts[ol + 3] = 3;
    opts[ol + 4] = 6;
    ol += 5;
    opts[ol] = 255;
    ol += 1;
    let dhcp_len = 236 + 4 + ol;
    let udp_len = 8 + dhcp_len;
    let po = ipv4_write(
        frame,
        o,
        [0, 0, 0, 0],
        [255, 255, 255, 255],
        IPPROTO_UDP,
        udp_len,
        64,
        xid as u16,
    );
    let bo = udp_write(frame, po, DHCP_CLIENT_PORT, DHCP_SERVER_PORT, dhcp_len);
    let b = &mut frame[bo..bo + dhcp_len];
    b.fill(0);
    b[0] = 1; // BOOTREQUEST
    b[1] = 1; // Ethernet
    b[2] = 6;
    b[4..8].copy_from_slice(&xid.to_be_bytes());
    b[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); // broadcast
    b[28..34].copy_from_slice(&src_mac);
    b[236..240].copy_from_slice(&[99, 130, 83, 99]); // magic cookie
    b[240..240 + ol].copy_from_slice(&opts[..ol]);
    bo + dhcp_len
}

/// Se `frame` é um DHCPOFFER (2) ou DHCPACK (5) para o `xid`, devolve (tipo, lease).
pub fn dhcp_parse(frame: &[u8], xid: u32) -> Option<(u8, DhcpLease)> {
    let (_, _, et, p) = eth_parse(frame)?;
    if et != ETHERTYPE_IPV4 {
        return None;
    }
    let ip = ipv4_parse(p)?;
    if ip.proto != IPPROTO_UDP {
        return None;
    }
    let (sport, dport, b) = udp_parse(ip.payload)?;
    if sport != DHCP_SERVER_PORT || dport != DHCP_CLIENT_PORT || b.len() < 240 {
        return None;
    }
    if b[0] != 2 || u32::from_be_bytes([b[4], b[5], b[6], b[7]]) != xid {
        return None;
    }
    if b[236..240] != [99, 130, 83, 99] {
        return None;
    }
    let mut lease = DhcpLease::default();
    lease.ip.copy_from_slice(&b[16..20]);
    let mut kind = 0u8;
    let mut i = 240usize;
    while i + 1 < b.len() {
        let code = b[i];
        if code == 255 {
            break;
        }
        if code == 0 {
            i += 1;
            continue;
        }
        let len = b[i + 1] as usize;
        if i + 2 + len > b.len() {
            return None;
        }
        let v = &b[i + 2..i + 2 + len];
        match (code, len) {
            (53, 1) => kind = v[0],
            (54, 4) => lease.server.copy_from_slice(v),
            (1, 4) => lease.mask.copy_from_slice(v),
            (3, 4) => lease.router.copy_from_slice(v),
            (6, l) if l >= 4 => lease.dns.copy_from_slice(&v[..4]),
            _ => {}
        }
        i += 2 + len;
    }
    if kind == 2 || kind == 5 {
        Some((kind, lease))
    } else {
        None
    }
}

/// Porta do DNS.
pub const DNS_PORT: u16 = 53;

/// Monta uma consulta DNS A por `name` (rótulos ASCII separados por `.`) num quadro completo
/// (Ethernet + IPv4 + UDP); devolve o tamanho do quadro, ou `None` se o nome for inválido.
#[allow(clippy::too_many_arguments)]
pub fn dns_query(
    frame: &mut [u8],
    src_mac: Mac,
    dst_mac: Mac,
    src_ip: Ipv4Addr,
    dns_ip: Ipv4Addr,
    src_port: u16,
    id: u16,
    name: &[u8],
) -> Option<usize> {
    if name.is_empty() || name.len() > 253 {
        return None;
    }
    let mut q = [0u8; 300];
    q[0..2].copy_from_slice(&id.to_be_bytes());
    q[2] = 0x01; // RD
    q[5] = 1; // QDCOUNT = 1
    let mut o = 12usize;
    for label in name.split(|&c| c == b'.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        q[o] = label.len() as u8;
        q[o + 1..o + 1 + label.len()].copy_from_slice(label);
        o += 1 + label.len();
    }
    q[o] = 0;
    o += 1;
    q[o..o + 2].copy_from_slice(&1u16.to_be_bytes()); // A
    q[o + 2..o + 4].copy_from_slice(&1u16.to_be_bytes()); // IN
    o += 4;
    let eo = eth_write(frame, dst_mac, src_mac, ETHERTYPE_IPV4);
    let po = ipv4_write(frame, eo, src_ip, dns_ip, IPPROTO_UDP, 8 + o, 64, id);
    let bo = udp_write(frame, po, src_port, DNS_PORT, o);
    frame[bo..bo + o].copy_from_slice(&q[..o]);
    Some(bo + o)
}

/// Resposta DNS decodificada (mínimo necessário).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DnsAnswer {
    /// RCODE (0 = sem erro, 3 = NXDOMAIN, …).
    pub rcode: u8,
    /// Número de respostas.
    pub answers: u16,
    /// Primeiro registro A, se houver.
    pub a: Option<Ipv4Addr>,
}

/// Se `frame` é uma resposta DNS do servidor `dns_ip` para (`id`, porta `src_port`), decodifica.
pub fn dns_parse(frame: &[u8], dns_ip: Ipv4Addr, src_port: u16, id: u16) -> Option<DnsAnswer> {
    let (_, _, et, p) = eth_parse(frame)?;
    if et != ETHERTYPE_IPV4 {
        return None;
    }
    let ip = ipv4_parse(p)?;
    if ip.proto != IPPROTO_UDP || ip.src != dns_ip {
        return None;
    }
    let (sport, dport, d) = udp_parse(ip.payload)?;
    if sport != DNS_PORT || dport != src_port || d.len() < 12 {
        return None;
    }
    if be16(d, 0) != id || d[2] & 0x80 == 0 {
        return None;
    }
    let mut ans = DnsAnswer {
        rcode: d[3] & 0xf,
        answers: be16(d, 6),
        a: None,
    };
    // pula as perguntas
    let qd = be16(d, 4) as usize;
    let mut o = 12usize;
    for _ in 0..qd {
        let mut hops = 0;
        while o < d.len() && d[o] != 0 {
            if d[o] & 0xc0 == 0xc0 {
                o += 1;
                break;
            }
            o += 1 + d[o] as usize;
            hops += 1;
            if hops > 64 {
                return None;
            }
        }
        o += 1 + 4; // fim do nome + tipo/classe
    }
    // percorre as respostas atras do primeiro registro A
    for _ in 0..ans.answers {
        if o >= d.len() {
            break;
        }
        // nome: rotulos ou ponteiro
        let mut hops = 0;
        while o < d.len() && d[o] != 0 {
            if d[o] & 0xc0 == 0xc0 {
                o += 1;
                break;
            }
            o += 1 + d[o] as usize;
            hops += 1;
            if hops > 64 {
                return None;
            }
        }
        o += 1;
        if o + 10 > d.len() {
            break;
        }
        let rtype = be16(d, o);
        let rdlen = be16(d, o + 8) as usize;
        let ro = o + 10;
        if ro + rdlen > d.len() {
            break;
        }
        if rtype == 1 && rdlen == 4 && ans.a.is_none() {
            let mut a = [0u8; 4];
            a.copy_from_slice(&d[ro..ro + 4]);
            ans.a = Some(a);
        }
        o = ro + rdlen;
    }
    Some(ans)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    const MAC_A: Mac = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
    const MAC_B: Mac = [0x52, 0x55, 0x0a, 0, 2, 2];
    const IP_A: Ipv4Addr = [10, 0, 2, 15];
    const IP_B: Ipv4Addr = [10, 0, 2, 2];

    #[test]
    fn arp_roundtrip() {
        let mut f = [0u8; 64];
        let n = arp_request(&mut f, MAC_A, IP_A, IP_B);
        assert_eq!(n, 42);
        // transforma em reply e reconhece
        let mut r = f;
        r[0..6].copy_from_slice(&MAC_A);
        r[6..12].copy_from_slice(&MAC_B);
        r[20] = 0;
        r[21] = 2;
        r[22..28].copy_from_slice(&MAC_B);
        r[28..32].copy_from_slice(&IP_B);
        assert_eq!(arp_reply_from(&r[..n], IP_B), Some(MAC_B));
        assert_eq!(arp_reply_from(&r[..n], IP_A), None);
        assert_eq!(arp_reply_from(&f[..n], IP_B), None); // request nao e reply
    }

    #[test]
    fn checksum_known_vector() {
        // RFC 1071 §3: 0x0001 + 0xf203 + 0xf4f5 + 0xf6f7 = 0x2ddf0 -> dobra 0xddf2 -> !0xddf2
        let data = [0x00u8, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7];
        assert_eq!(inet_checksum(&data), 0x220d);
        // um cabecalho valido tem checksum 0 ao revalidar
        let mut f = [0u8; 128];
        let o = eth_write(&mut f, MAC_B, MAC_A, ETHERTYPE_IPV4);
        ipv4_write(&mut f, o, IP_A, IP_B, IPPROTO_ICMP, 8, 64, 7);
        assert_eq!(inet_checksum(&f[o..o + IPV4_HLEN]), 0);
    }

    #[test]
    fn ipv4_parse_validates() {
        let mut f = [0u8; 128];
        let o = eth_write(&mut f, MAC_B, MAC_A, ETHERTYPE_IPV4);
        let po = ipv4_write(&mut f, o, IP_A, IP_B, IPPROTO_ICMP, 4, 64, 7);
        f[po..po + 4].copy_from_slice(b"abcd");
        let ip = ipv4_parse(&f[o..po + 4]).unwrap();
        assert_eq!(ip.src, IP_A);
        assert_eq!(ip.dst, IP_B);
        assert_eq!(ip.proto, IPPROTO_ICMP);
        assert_eq!(ip.payload, b"abcd");
        // checksum corrompido -> rejeita
        let mut bad = f;
        bad[o + 12] ^= 1;
        assert!(ipv4_parse(&bad[o..po + 4]).is_none());
    }

    #[test]
    fn icmp_echo_roundtrip() {
        let mut f = [0u8; 256];
        let n = icmp_echo_request(&mut f, MAC_A, MAC_B, IP_A, IP_B, 0xbeef, 1, b"nexo-ping");
        // constroi o reply correspondente: troca MAC/IP e tipo 0, recalculando checksums
        let mut r = [0u8; 256];
        let rn = icmp_echo_request(&mut r, MAC_B, MAC_A, IP_B, IP_A, 0xbeef, 1, b"nexo-ping");
        r[ETH_HLEN + IPV4_HLEN] = 0; // echo reply
        let icmp_len = rn - ETH_HLEN - IPV4_HLEN;
        r[ETH_HLEN + IPV4_HLEN + 2..ETH_HLEN + IPV4_HLEN + 4].copy_from_slice(&[0, 0]);
        let ck = inet_checksum(&r[ETH_HLEN + IPV4_HLEN..rn]);
        r[ETH_HLEN + IPV4_HLEN + 2..ETH_HLEN + IPV4_HLEN + 4].copy_from_slice(&ck.to_be_bytes());
        let _ = icmp_len;
        let (ttl, data) = icmp_echo_reply(&r[..rn], IP_B, 0xbeef, 1).unwrap();
        assert_eq!(ttl, 64);
        assert_eq!(data, b"nexo-ping");
        assert!(icmp_echo_reply(&f[..n], IP_B, 0xbeef, 1).is_none()); // request nao e reply
        assert!(icmp_echo_reply(&r[..rn], IP_B, 0xbeef, 2).is_none()); // seq errada
    }

    #[test]
    fn udp_roundtrip() {
        let mut f = [0u8; 256];
        let o = eth_write(&mut f, MAC_B, MAC_A, ETHERTYPE_IPV4);
        let po = ipv4_write(&mut f, o, IP_A, IP_B, IPPROTO_UDP, 8 + 5, 64, 9);
        let bo = udp_write(&mut f, po, 1234, 5678, 5);
        f[bo..bo + 5].copy_from_slice(b"hello");
        let ip = ipv4_parse(&f[o..bo + 5]).unwrap();
        let (sp, dp, data) = udp_parse(ip.payload).unwrap();
        assert_eq!((sp, dp, data), (1234, 5678, &b"hello"[..]));
        assert!(udp_parse(&[0u8; 4]).is_none());
    }

    #[test]
    fn dhcp_discover_offer() {
        let mut f = [0u8; 640];
        let n = dhcp_build(&mut f, MAC_A, 0x4e58_0001, None);
        assert!(n > 240);
        // constroi um OFFER correspondente: BOOTREPLY com yiaddr e opcoes
        let mut off = [0u8; 640];
        let oo = eth_write(&mut off, MAC_A, MAC_B, ETHERTYPE_IPV4);
        let opts: &[u8] = &[
            53, 1, 2, 54, 4, 10, 0, 2, 2, 1, 4, 255, 255, 255, 0, 3, 4, 10, 0, 2, 2, 6, 4, 10, 0,
            2, 3, 255,
        ];
        let dhcp_len = 240 + opts.len();
        let po = ipv4_write(
            &mut off,
            oo,
            IP_B,
            [255, 255, 255, 255],
            IPPROTO_UDP,
            8 + dhcp_len,
            64,
            1,
        );
        let bo = udp_write(&mut off, po, DHCP_SERVER_PORT, DHCP_CLIENT_PORT, dhcp_len);
        off[bo] = 2; // BOOTREPLY
        off[bo + 4..bo + 8].copy_from_slice(&0x4e58_0001u32.to_be_bytes());
        off[bo + 16..bo + 20].copy_from_slice(&IP_A);
        off[bo + 236..bo + 240].copy_from_slice(&[99, 130, 83, 99]);
        off[bo + 240..bo + 240 + opts.len()].copy_from_slice(opts);
        let (kind, lease) = dhcp_parse(&off[..bo + dhcp_len], 0x4e58_0001).unwrap();
        assert_eq!(kind, 2);
        assert_eq!(lease.ip, IP_A);
        assert_eq!(lease.server, IP_B);
        assert_eq!(lease.mask, [255, 255, 255, 0]);
        assert_eq!(lease.router, IP_B);
        assert_eq!(lease.dns, [10, 0, 2, 3]);
        // xid errado -> ignora
        assert!(dhcp_parse(&off[..bo + dhcp_len], 7).is_none());
        // REQUEST inclui as opcoes 50/54
        let rn = dhcp_build(&mut f, MAC_A, 2, Some((IP_A, IP_B)));
        assert!(rn > n);
    }

    #[test]
    fn dns_query_and_answer() {
        let mut f = [0u8; 512];
        let n = dns_query(
            &mut f,
            MAC_A,
            MAC_B,
            IP_A,
            [10, 0, 2, 3],
            40000,
            0x1234,
            b"exemplo.nexo",
        )
        .unwrap();
        assert!(n > 42);
        assert!(dns_query(&mut f, MAC_A, MAC_B, IP_A, [10, 0, 2, 3], 40000, 1, b"").is_none());
        // resposta sintetica: copia a pergunta, marca QR, 1 resposta A via ponteiro
        let mut r = [0u8; 512];
        let ro = eth_write(&mut r, MAC_A, MAC_B, ETHERTYPE_IPV4);
        let qlen = n - ETH_HLEN - IPV4_HLEN - 8;
        let alen = qlen + 16;
        let po = ipv4_write(
            &mut r,
            ro,
            [10, 0, 2, 3],
            IP_A,
            IPPROTO_UDP,
            8 + alen,
            64,
            2,
        );
        let bo = udp_write(&mut r, po, DNS_PORT, 40000, alen);
        r[bo..bo + qlen].copy_from_slice(&f[n - qlen..n]);
        r[bo + 2] = 0x81; // QR + RD
        r[bo + 3] = 0x80; // RA, rcode 0
        r[bo + 7] = 1; // ANCOUNT = 1
        let ao = bo + qlen;
        r[ao..ao + 2].copy_from_slice(&0xc00cu16.to_be_bytes()); // ponteiro para a pergunta
        r[ao + 2..ao + 4].copy_from_slice(&1u16.to_be_bytes()); // A
        r[ao + 4..ao + 6].copy_from_slice(&1u16.to_be_bytes()); // IN
        r[ao + 6..ao + 10].copy_from_slice(&60u32.to_be_bytes()); // TTL
        r[ao + 10..ao + 12].copy_from_slice(&4u16.to_be_bytes());
        r[ao + 12..ao + 16].copy_from_slice(&[93, 184, 216, 34]);
        let total = ao + 16;
        let ans = dns_parse(&r[..total], [10, 0, 2, 3], 40000, 0x1234).unwrap();
        assert_eq!(ans.rcode, 0);
        assert_eq!(ans.answers, 1);
        assert_eq!(ans.a, Some([93, 184, 216, 34]));
        // id errado -> ignora
        assert!(dns_parse(&r[..total], [10, 0, 2, 3], 40000, 9).is_none());
        // porta errada -> ignora
        assert!(dns_parse(&r[..total], [10, 0, 2, 3], 40001, 0x1234).is_none());
    }

    #[test]
    fn fuzz_lite_parsers_never_panic() {
        let mut f = [0u8; 256];
        let n = icmp_echo_request(&mut f, MAC_A, MAC_B, IP_A, IP_B, 1, 1, &[7u8; 32]);
        let mut seed = 0x0dd0_51deu64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20_000 {
            let mut m = f;
            for _ in 0..1 + next() % 4 {
                let i = (next() % n as u64) as usize;
                m[i] ^= (next() % 255 + 1) as u8;
            }
            let cut = (next() % (n as u64 + 4)) as usize;
            let s = &m[..cut.min(256)];
            let _ = eth_parse(s);
            let _ = arp_reply_from(s, IP_B);
            if s.len() > ETH_HLEN {
                let _ = ipv4_parse(&s[ETH_HLEN..]);
            }
            let _ = icmp_echo_reply(s, IP_B, 1, 1);
            let _ = dhcp_parse(s, 1);
            let _ = dns_parse(s, [10, 0, 2, 3], 40000, 1);
            if s.len() > ETH_HLEN + IPV4_HLEN {
                let _ = udp_parse(&s[ETH_HLEN + IPV4_HLEN..]);
            }
        }
    }
}
