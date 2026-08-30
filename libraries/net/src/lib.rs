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
        }
    }
}
