//! IPv6 mínimo (Fase 4: "implementar IPv6"): endereço, cabeçalho fixo de 40 bytes, ICMPv6 com
//! o checksum do pseudo-cabeçalho, e NDP (Neighbor Solicitation/Advertisement, incl. os
//! endereços multicast solicited-node) — o suficiente para resolver vizinhos e responder a
//! echo. Sem alocação; buffers do chamador. Testável no host (incl. fuzz-lite).

use crate::{ETHERTYPE_IPV4, Mac, eth_parse, eth_write};

/// EtherType IPv6.
pub const ETHERTYPE_IPV6: u16 = 0x86dd;
/// Next Header: ICMPv6.
pub const IPPROTO_ICMPV6: u8 = 58;
/// Next Header: UDP.
pub const IPPROTO_UDP6: u8 = 17;
/// Next Header: TCP.
pub const IPPROTO_TCP6: u8 = 6;
/// Cabeçalho IPv6 fixo.
pub const IPV6_HLEN: usize = 40;

/// Tipos ICMPv6 relevantes.
pub const ICMP6_ECHO_REQUEST: u8 = 128;
/// Echo reply.
pub const ICMP6_ECHO_REPLY: u8 = 129;
/// Neighbor Solicitation.
pub const ICMP6_NS: u8 = 135;
/// Neighbor Advertisement.
pub const ICMP6_NA: u8 = 136;

/// Endereço IPv6.
pub type Ipv6Addr = [u8; 16];

/// `fe80::` + EUI-64 do MAC (link-local, com o bit U/L invertido).
pub fn link_local(mac: Mac) -> Ipv6Addr {
    let mut a = [0u8; 16];
    a[0] = 0xfe;
    a[1] = 0x80;
    a[8] = mac[0] ^ 0x02;
    a[9] = mac[1];
    a[10] = mac[2];
    a[11] = 0xff;
    a[12] = 0xfe;
    a[13] = mac[3];
    a[14] = mac[4];
    a[15] = mac[5];
    a
}

/// Multicast solicited-node de `addr`: `ff02::1:ffXX:XXXX` com os últimos 24 bits de `addr`.
pub fn solicited_node(addr: &Ipv6Addr) -> Ipv6Addr {
    let mut a = [0u8; 16];
    a[0] = 0xff;
    a[1] = 0x02;
    a[11] = 0x01;
    a[12] = 0xff;
    a[13] = addr[13];
    a[14] = addr[14];
    a[15] = addr[15];
    a
}

/// MAC multicast de um endereço IPv6 multicast (`33:33:` + os últimos 4 bytes).
pub fn multicast_mac(addr: &Ipv6Addr) -> Mac {
    [0x33, 0x33, addr[12], addr[13], addr[14], addr[15]]
}

fn be16(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}

/// Escreve o cabeçalho IPv6; devolve o offset do payload.
#[allow(clippy::too_many_arguments)]
pub fn ipv6_write(
    frame: &mut [u8],
    eth_payload_off: usize,
    src: &Ipv6Addr,
    dst: &Ipv6Addr,
    next: u8,
    payload_len: usize,
    hop_limit: u8,
) -> usize {
    let h = &mut frame[eth_payload_off..eth_payload_off + IPV6_HLEN];
    h[0] = 0x60; // versão 6, sem traffic class/flow label
    h[1] = 0;
    h[2] = 0;
    h[3] = 0;
    h[4..6].copy_from_slice(&(payload_len as u16).to_be_bytes());
    h[6] = next;
    h[7] = hop_limit;
    h[8..24].copy_from_slice(src);
    h[24..40].copy_from_slice(dst);
    eth_payload_off + IPV6_HLEN
}

/// Pacote IPv6 decodificado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv6<'a> {
    /// Origem.
    pub src: Ipv6Addr,
    /// Destino.
    pub dst: Ipv6Addr,
    /// Next Header (protocolo).
    pub next: u8,
    /// Hop limit.
    pub hop_limit: u8,
    /// Payload.
    pub payload: &'a [u8],
}

/// Lê um cabeçalho IPv6 (sem cabeçalhos de extensão).
pub fn ipv6_parse(p: &[u8]) -> Option<Ipv6<'_>> {
    if p.len() < IPV6_HLEN || p[0] >> 4 != 6 {
        return None;
    }
    let plen = be16(p, 4) as usize;
    if IPV6_HLEN + plen > p.len() {
        return None;
    }
    let mut src = [0u8; 16];
    let mut dst = [0u8; 16];
    src.copy_from_slice(&p[8..24]);
    dst.copy_from_slice(&p[24..40]);
    Some(Ipv6 {
        src,
        dst,
        next: p[6],
        hop_limit: p[7],
        payload: &p[IPV6_HLEN..IPV6_HLEN + plen],
    })
}

/// Checksum ICMPv6/UDP/TCP sobre IPv6 (pseudo-cabeçalho de 40 bytes + payload).
pub fn checksum6(src: &Ipv6Addr, dst: &Ipv6Addr, next: u8, payload: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut pseudo = [0u8; 40];
    pseudo[0..16].copy_from_slice(src);
    pseudo[16..32].copy_from_slice(dst);
    pseudo[32..36].copy_from_slice(&(payload.len() as u32).to_be_bytes());
    pseudo[39] = next;
    for chunk in [&pseudo[..], payload] {
        let (pairs, rest) = chunk.as_chunks::<2>();
        for c in pairs {
            sum += u16::from_be_bytes(*c) as u32;
        }
        if let [last] = rest {
            sum += (*last as u32) << 8;
        }
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Monta um Neighbor Solicitation por `target` (Ethernet + IPv6 + ICMPv6, com a opção Source
/// Link-Layer Address); devolve o tamanho do quadro.
pub fn neighbor_solicitation(
    frame: &mut [u8],
    src_mac: Mac,
    src_ip: &Ipv6Addr,
    target: &Ipv6Addr,
) -> usize {
    let dst_ip = solicited_node(target);
    let dst_mac = multicast_mac(&dst_ip);
    let o = eth_write(frame, dst_mac, src_mac, ETHERTYPE_IPV6);
    // ICMPv6: type(1) code(1) cksum(2) reserved(4) target(16) option(8) = 32
    let icmp_len = 32;
    let po = ipv6_write(frame, o, src_ip, &dst_ip, IPPROTO_ICMPV6, icmp_len, 255);
    {
        let m = &mut frame[po..po + icmp_len];
        m.fill(0);
        m[0] = ICMP6_NS;
        m[8..24].copy_from_slice(target);
        m[24] = 1; // opção Source Link-Layer Address
        m[25] = 1; // comprimento (em unidades de 8 bytes)
        m[26..32].copy_from_slice(&src_mac);
    }
    let ck = checksum6(src_ip, &dst_ip, IPPROTO_ICMPV6, &frame[po..po + icmp_len]);
    frame[po + 2..po + 4].copy_from_slice(&ck.to_be_bytes());
    po + icmp_len
}

/// Se `frame` é um Neighbor Advertisement pelo `target`, devolve o MAC anunciado (opção Target
/// Link-Layer Address).
pub fn neighbor_advert_mac(frame: &[u8], target: &Ipv6Addr) -> Option<Mac> {
    let (_, _, et, p) = eth_parse(frame)?;
    if et != ETHERTYPE_IPV6 {
        return None;
    }
    let ip = ipv6_parse(p)?;
    if ip.next != IPPROTO_ICMPV6 {
        return None;
    }
    let m = ip.payload;
    if m.len() < 24 || m[0] != ICMP6_NA || &m[8..24] != target {
        return None;
    }
    // opções após os 24 bytes fixos
    let mut o = 24;
    while o + 8 <= m.len() {
        let (otype, olen) = (m[o], m[o + 1] as usize * 8);
        if olen == 0 || o + olen > m.len() {
            break;
        }
        if otype == 2 && olen >= 8 {
            let mut mac = [0u8; 6];
            mac.copy_from_slice(&m[o + 2..o + 8]);
            return Some(mac);
        }
        o += olen;
    }
    None
}

/// Monta um ICMPv6 echo request; devolve o tamanho do quadro.
#[allow(clippy::too_many_arguments)]
pub fn icmp6_echo_request(
    frame: &mut [u8],
    src_mac: Mac,
    dst_mac: Mac,
    src_ip: &Ipv6Addr,
    dst_ip: &Ipv6Addr,
    ident: u16,
    seq: u16,
    data: &[u8],
) -> usize {
    let o = eth_write(frame, dst_mac, src_mac, ETHERTYPE_IPV6);
    let icmp_len = 8 + data.len();
    let po = ipv6_write(frame, o, src_ip, dst_ip, IPPROTO_ICMPV6, icmp_len, 64);
    {
        let m = &mut frame[po..po + icmp_len];
        m[0] = ICMP6_ECHO_REQUEST;
        m[1] = 0;
        m[2..4].copy_from_slice(&0u16.to_be_bytes());
        m[4..6].copy_from_slice(&ident.to_be_bytes());
        m[6..8].copy_from_slice(&seq.to_be_bytes());
        m[8..].copy_from_slice(data);
    }
    let ck = checksum6(src_ip, dst_ip, IPPROTO_ICMPV6, &frame[po..po + icmp_len]);
    frame[po + 2..po + 4].copy_from_slice(&ck.to_be_bytes());
    po + icmp_len
}

/// Se `frame` é um echo reply de `from_ip` com (`ident`, `seq`), devolve (hop_limit, dados).
pub fn icmp6_echo_reply<'a>(
    frame: &'a [u8],
    from_ip: &Ipv6Addr,
    ident: u16,
    seq: u16,
) -> Option<(u8, &'a [u8])> {
    let (_, _, et, p) = eth_parse(frame)?;
    if et != ETHERTYPE_IPV6 {
        return None;
    }
    let ip = ipv6_parse(p)?;
    if ip.next != IPPROTO_ICMPV6 || &ip.src != from_ip {
        return None;
    }
    let m = ip.payload;
    if m.len() < 8 || m[0] != ICMP6_ECHO_REPLY {
        return None;
    }
    if checksum6(&ip.src, &ip.dst, IPPROTO_ICMPV6, m) != 0 {
        return None;
    }
    if be16(m, 4) != ident || be16(m, 6) != seq {
        return None;
    }
    Some((ip.hop_limit, &m[8..]))
}

/// `frame` é um Neighbor Solicitation pelo nosso `me`? Se sim, monta o Advertisement de resposta
/// em `out` e devolve o tamanho; senão `None`.
pub fn respond_ns(frame: &[u8], my_mac: Mac, me: &Ipv6Addr, out: &mut [u8]) -> Option<usize> {
    let (_, src_mac, et, p) = eth_parse(frame)?;
    if et != ETHERTYPE_IPV6 {
        return None;
    }
    let ip = ipv6_parse(p)?;
    if ip.next != IPPROTO_ICMPV6 {
        return None;
    }
    let m = ip.payload;
    if m.len() < 24 || m[0] != ICMP6_NS || &m[8..24] != me {
        return None;
    }
    // NA: flags Solicited+Override, target = me, opção Target Link-Layer Address
    let o = eth_write(out, src_mac, my_mac, ETHERTYPE_IPV6);
    let icmp_len = 32;
    let po = ipv6_write(out, o, me, &ip.src, IPPROTO_ICMPV6, icmp_len, 255);
    {
        let na = &mut out[po..po + icmp_len];
        na.fill(0);
        na[0] = ICMP6_NA;
        na[4] = 0x60; // Solicited | Override
        na[8..24].copy_from_slice(me);
        na[24] = 2; // Target Link-Layer Address
        na[25] = 1;
        na[26..32].copy_from_slice(&my_mac);
    }
    let ck = checksum6(me, &ip.src, IPPROTO_ICMPV6, &out[po..po + icmp_len]);
    out[po + 2..po + 4].copy_from_slice(&ck.to_be_bytes());
    let _ = ETHERTYPE_IPV4;
    Some(po + icmp_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC_A: Mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    const MAC_B: Mac = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];

    #[test]
    fn link_local_and_solicited_node() {
        let ll = link_local(MAC_A);
        assert_eq!(ll[0..2], [0xfe, 0x80]);
        // U/L invertido: 0x52 ^ 0x02 = 0x50
        assert_eq!(ll[8], 0x50);
        assert_eq!(&ll[11..13], &[0xff, 0xfe]);
        let sn = solicited_node(&ll);
        assert_eq!(sn[0..2], [0xff, 0x02]);
        assert_eq!(&sn[11..13], &[0x01, 0xff]);
        assert_eq!(&sn[13..16], &ll[13..16]);
        assert_eq!(
            multicast_mac(&sn),
            [0x33, 0x33, 0xff, ll[13], ll[14], ll[15]]
        );
    }

    #[test]
    fn ns_then_na_roundtrip() {
        let me = link_local(MAC_A);
        let peer = link_local(MAC_B);
        let mut ns = [0u8; 128];
        let n = neighbor_solicitation(&mut ns, MAC_B, &peer, &me);
        // do lado A: e um NS pelo nosso endereco -> responde NA
        let mut na = [0u8; 128];
        let m = respond_ns(&ns[..n], MAC_A, &me, &mut na).unwrap();
        // do lado B: extrai o MAC de A do NA
        assert_eq!(neighbor_advert_mac(&na[..m], &me), Some(MAC_A));
        // NS nao e NA
        assert_eq!(neighbor_advert_mac(&ns[..n], &me), None);
        // NS por outro alvo nao gera resposta
        assert!(respond_ns(&ns[..n], MAC_A, &peer, &mut na).is_none());
    }

    #[test]
    fn echo6_roundtrip_and_checksum() {
        let a = link_local(MAC_A);
        let b = link_local(MAC_B);
        let mut req = [0u8; 128];
        let n = icmp6_echo_request(&mut req, MAC_A, MAC_B, &a, &b, 0xbeef, 1, b"nexo6");
        // constroi o reply correspondente
        let mut rep = [0u8; 128];
        let rn = icmp6_echo_request(&mut rep, MAC_B, MAC_A, &b, &a, 0xbeef, 1, b"nexo6");
        rep[crate::ETH_HLEN + IPV6_HLEN] = ICMP6_ECHO_REPLY;
        let icmp_len = rn - crate::ETH_HLEN - IPV6_HLEN;
        let po = crate::ETH_HLEN + IPV6_HLEN;
        rep[po + 2..po + 4].copy_from_slice(&[0, 0]);
        let ck = checksum6(&b, &a, IPPROTO_ICMPV6, &rep[po..po + icmp_len]);
        rep[po + 2..po + 4].copy_from_slice(&ck.to_be_bytes());
        let (hop, data) = icmp6_echo_reply(&rep[..rn], &b, 0xbeef, 1).unwrap();
        assert_eq!(hop, 64);
        assert_eq!(data, b"nexo6");
        assert!(icmp6_echo_reply(&req[..n], &b, 0xbeef, 1).is_none()); // request nao e reply
        assert!(icmp6_echo_reply(&rep[..rn], &b, 0xbeef, 2).is_none()); // seq errada
    }

    #[test]
    fn fuzz_lite_never_panics() {
        let a = link_local(MAC_A);
        let b = link_local(MAC_B);
        let mut req = [0u8; 128];
        let n = icmp6_echo_request(&mut req, MAC_A, MAC_B, &a, &b, 1, 1, &[9u8; 16]);
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..20_000 {
            let mut m = req;
            for _ in 0..1 + seed % 4 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let i = (seed % n as u64) as usize;
                m[i] ^= (seed % 255 + 1) as u8;
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            let cut = (seed % (n as u64 + 4)) as usize;
            let s = &m[..cut.min(128)];
            let _ = ipv6_parse(if s.len() > crate::ETH_HLEN {
                &s[crate::ETH_HLEN..]
            } else {
                s
            });
            let _ = icmp6_echo_reply(s, &b, 1, 1);
            let _ = neighbor_advert_mac(s, &a);
            let mut out = [0u8; 128];
            let _ = respond_ns(s, MAC_A, &a, &mut out);
        }
    }

    fn _unused() {}
}
