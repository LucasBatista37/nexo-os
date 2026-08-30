//! Protocolo tipado `nexo.sock` v1.0 — **gerado por `tools/idlgen` de `idl/sock.idl`; nao editar**.

use crate::{FLAG_ERROR, FLAG_RESPONSE, HEADER_LEN, Header, ProtoError};

/// FNV-1a de `"nexo.sock"`.
pub const PROTOCOL_ID: u32 = 0x60281105;
/// Versao maior (incompatibilidades).
pub const VERSION_MAJOR: u16 = 1;
/// Versao menor (adicoes compativeis).
pub const VERSION_MINOR: u16 = 0;

/// `nexo.sock.info` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoRequest {}

impl InfoRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(InfoRequest {})
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: 0,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.info` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoResponse {
    /// Bytes de `ip` (ate 4).
    pub ip: [u8; 4],
    /// Tamanho valido de `ip`.
    pub ip_len: u32,
    /// Bytes de `mask` (ate 4).
    pub mask: [u8; 4],
    /// Tamanho valido de `mask`.
    pub mask_len: u32,
    /// Bytes de `gateway` (ate 4).
    pub gateway: [u8; 4],
    /// Tamanho valido de `gateway`.
    pub gateway_len: u32,
    /// Bytes de `dns` (ate 4).
    pub dns: [u8; 4],
    /// Tamanho valido de `dns`.
    pub dns_len: u32,
    /// Bytes de `mac` (ate 6).
    pub mac: [u8; 6],
    /// Tamanho valido de `mac`.
    pub mac_len: u32,
}

impl InfoResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Fatia valida de `ip`.
    pub fn ip(&self) -> &[u8] {
        &self.ip[..(self.ip_len as usize).min(4)]
    }
    /// Fatia valida de `mask`.
    pub fn mask(&self) -> &[u8] {
        &self.mask[..(self.mask_len as usize).min(4)]
    }
    /// Fatia valida de `gateway`.
    pub fn gateway(&self) -> &[u8] {
        &self.gateway[..(self.gateway_len as usize).min(4)]
    }
    /// Fatia valida de `dns`.
    pub fn dns(&self) -> &[u8] {
        &self.dns[..(self.dns_len as usize).min(4)]
    }
    /// Fatia valida de `mac`.
    pub fn mac(&self) -> &[u8] {
        &self.mac[..(self.mac_len as usize).min(6)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.ip_len as usize;
        if n > 4 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.ip[..n]);
        o += 4 + n;
        let n = self.mask_len as usize;
        if n > 4 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.mask[..n]);
        o += 4 + n;
        let n = self.gateway_len as usize;
        if n > 4 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.gateway[..n]);
        o += 4 + n;
        let n = self.dns_len as usize;
        if n > 4 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.dns[..n]);
        o += 4 + n;
        let n = self.mac_len as usize;
        if n > 6 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.mac[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut ip = [0u8; 4];
        let ip_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            ip[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            ip_len = l as u32;
            o += 4 + l;
        }
        let mut mask = [0u8; 4];
        let mask_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            mask[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            mask_len = l as u32;
            o += 4 + l;
        }
        let mut gateway = [0u8; 4];
        let gateway_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            gateway[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            gateway_len = l as u32;
            o += 4 + l;
        }
        let mut dns = [0u8; 4];
        let dns_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            dns[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            dns_len = l as u32;
            o += 4 + l;
        }
        let mut mac = [0u8; 6];
        let mac_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 6 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            mac[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            mac_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(InfoResponse {
            ip,
            ip_len,
            mask,
            mask_len,
            gateway,
            gateway_len,
            dns,
            dns_len,
            mac,
            mac_len,
        })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: FLAG_RESPONSE,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.resolve` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveRequest {
    /// Bytes de `name` (ate 253).
    pub name: [u8; 253],
    /// Tamanho valido de `name`.
    pub name_len: u32,
}

impl ResolveRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 2;
    /// Fatia valida de `name`.
    pub fn name(&self) -> &[u8] {
        &self.name[..(self.name_len as usize).min(253)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.name_len as usize;
        if n > 253 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.name[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut name = [0u8; 253];
        let name_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 253 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            name[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            name_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(ResolveRequest { name, name_len })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: 0,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.resolve` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveResponse {
    /// Bytes de `addr` (ate 4).
    pub addr: [u8; 4],
    /// Tamanho valido de `addr`.
    pub addr_len: u32,
    /// Campo `cached`.
    pub cached: u8,
}

impl ResolveResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 2;
    /// Fatia valida de `addr`.
    pub fn addr(&self) -> &[u8] {
        &self.addr[..(self.addr_len as usize).min(4)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.addr_len as usize;
        if n > 4 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.addr[..n]);
        o += 4 + n;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.cached.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut addr = [0u8; 4];
        let addr_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            addr[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            addr_len = l as u32;
            o += 4 + l;
        }
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let cached = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(ResolveResponse {
            addr,
            addr_len,
            cached,
        })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: FLAG_RESPONSE,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.udp_send` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UdpSendRequest {
    /// Bytes de `dst_ip` (ate 4).
    pub dst_ip: [u8; 4],
    /// Tamanho valido de `dst_ip`.
    pub dst_ip_len: u32,
    /// Campo `dst_port`.
    pub dst_port: u16,
    /// Campo `src_port`.
    pub src_port: u16,
    /// Bytes de `data` (ate 1400).
    pub data: [u8; 1400],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl UdpSendRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Fatia valida de `dst_ip`.
    pub fn dst_ip(&self) -> &[u8] {
        &self.dst_ip[..(self.dst_ip_len as usize).min(4)]
    }
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(1400)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.dst_ip_len as usize;
        if n > 4 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.dst_ip[..n]);
        o += 4 + n;
        if o + 2 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 2].copy_from_slice(&self.dst_port.to_le_bytes());
        o += 2;
        if o + 2 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 2].copy_from_slice(&self.src_port.to_le_bytes());
        o += 2;
        let n = self.data_len as usize;
        if n > 1400 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.data[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut dst_ip = [0u8; 4];
        let dst_ip_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            dst_ip[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            dst_ip_len = l as u32;
            o += 4 + l;
        }
        if o + 2 > b.len() {
            return Err(ProtoError::Short);
        }
        let dst_port = u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
        o += 2;
        if o + 2 > b.len() {
            return Err(ProtoError::Short);
        }
        let src_port = u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
        o += 2;
        let mut data = [0u8; 1400];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 1400 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            data[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            data_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(UdpSendRequest {
            dst_ip,
            dst_ip_len,
            dst_port,
            src_port,
            data,
            data_len,
        })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: 0,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.udp_send` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UdpSendResponse {}

impl UdpSendResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(UdpSendResponse {})
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: FLAG_RESPONSE,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.udp_recv` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UdpRecvRequest {
    /// Campo `port`.
    pub port: u16,
}

impl UdpRecvRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 4;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 2 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 2].copy_from_slice(&self.port.to_le_bytes());
        o += 2;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 2 > b.len() {
            return Err(ProtoError::Short);
        }
        let port = u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
        o += 2;
        let _ = o;
        Ok(UdpRecvRequest { port })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: 0,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.udp_recv` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UdpRecvResponse {
    /// Bytes de `from_ip` (ate 4).
    pub from_ip: [u8; 4],
    /// Tamanho valido de `from_ip`.
    pub from_ip_len: u32,
    /// Campo `from_port`.
    pub from_port: u16,
    /// Bytes de `data` (ate 1400).
    pub data: [u8; 1400],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl UdpRecvResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 4;
    /// Fatia valida de `from_ip`.
    pub fn from_ip(&self) -> &[u8] {
        &self.from_ip[..(self.from_ip_len as usize).min(4)]
    }
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(1400)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.from_ip_len as usize;
        if n > 4 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.from_ip[..n]);
        o += 4 + n;
        if o + 2 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 2].copy_from_slice(&self.from_port.to_le_bytes());
        o += 2;
        let n = self.data_len as usize;
        if n > 1400 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.data[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut from_ip = [0u8; 4];
        let from_ip_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            from_ip[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            from_ip_len = l as u32;
            o += 4 + l;
        }
        if o + 2 > b.len() {
            return Err(ProtoError::Short);
        }
        let from_port = u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
        o += 2;
        let mut data = [0u8; 1400];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 1400 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            data[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            data_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(UdpRecvResponse {
            from_ip,
            from_ip_len,
            from_port,
            data,
            data_len,
        })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: FLAG_RESPONSE,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.tcp_connect` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpConnectRequest {
    /// Bytes de `dst_ip` (ate 4).
    pub dst_ip: [u8; 4],
    /// Tamanho valido de `dst_ip`.
    pub dst_ip_len: u32,
    /// Campo `dst_port`.
    pub dst_port: u16,
}

impl TcpConnectRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 5;
    /// Fatia valida de `dst_ip`.
    pub fn dst_ip(&self) -> &[u8] {
        &self.dst_ip[..(self.dst_ip_len as usize).min(4)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.dst_ip_len as usize;
        if n > 4 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.dst_ip[..n]);
        o += 4 + n;
        if o + 2 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 2].copy_from_slice(&self.dst_port.to_le_bytes());
        o += 2;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut dst_ip = [0u8; 4];
        let dst_ip_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            dst_ip[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            dst_ip_len = l as u32;
            o += 4 + l;
        }
        if o + 2 > b.len() {
            return Err(ProtoError::Short);
        }
        let dst_port = u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
        o += 2;
        let _ = o;
        Ok(TcpConnectRequest {
            dst_ip,
            dst_ip_len,
            dst_port,
        })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: 0,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.tcp_connect` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpConnectResponse {
    /// Campo `conn`.
    pub conn: u32,
}

impl TcpConnectResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 5;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.conn.to_le_bytes());
        o += 4;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let conn = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(TcpConnectResponse { conn })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: FLAG_RESPONSE,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.tcp_send` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpSendRequest {
    /// Campo `conn`.
    pub conn: u32,
    /// Bytes de `data` (ate 1400).
    pub data: [u8; 1400],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl TcpSendRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 6;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(1400)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.conn.to_le_bytes());
        o += 4;
        let n = self.data_len as usize;
        if n > 1400 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.data[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let conn = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let mut data = [0u8; 1400];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 1400 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            data[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            data_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(TcpSendRequest {
            conn,
            data,
            data_len,
        })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: 0,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.tcp_send` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpSendResponse {
    /// Campo `sent`.
    pub sent: u32,
}

impl TcpSendResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 6;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.sent.to_le_bytes());
        o += 4;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let sent = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(TcpSendResponse { sent })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: FLAG_RESPONSE,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.tcp_recv` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpRecvRequest {
    /// Campo `conn`.
    pub conn: u32,
}

impl TcpRecvRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 7;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.conn.to_le_bytes());
        o += 4;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let conn = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(TcpRecvRequest { conn })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: 0,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.tcp_recv` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpRecvResponse {
    /// Bytes de `data` (ate 1400).
    pub data: [u8; 1400],
    /// Tamanho valido de `data`.
    pub data_len: u32,
    /// Campo `closed`.
    pub closed: u8,
}

impl TcpRecvResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 7;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(1400)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.data_len as usize;
        if n > 1400 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.data[..n]);
        o += 4 + n;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.closed.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut data = [0u8; 1400];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 1400 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            data[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            data_len = l as u32;
            o += 4 + l;
        }
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let closed = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(TcpRecvResponse {
            data,
            data_len,
            closed,
        })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: FLAG_RESPONSE,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.tcp_close` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpCloseRequest {
    /// Campo `conn`.
    pub conn: u32,
}

impl TcpCloseRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 8;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.conn.to_le_bytes());
        o += 4;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let conn = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(TcpCloseRequest { conn })
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: 0,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// `nexo.sock.tcp_close` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpCloseResponse {}

impl TcpCloseResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 8;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(TcpCloseResponse {})
    }
    /// Codifica a mensagem completa (cabecalho NXIP + payload).
    pub fn encode_msg(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let plen = self.encode_payload(&mut out[HEADER_LEN..])?;
        let h = Header {
            protocol_id: PROTOCOL_ID,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            method_id: Self::METHOD_ID,
            flags: FLAG_RESPONSE,
            payload_len: plen as u32,
        };
        h.encode(out)?;
        Ok(HEADER_LEN + plen)
    }
}

/// Pedido decodificado.
#[derive(Clone, Debug, PartialEq, Eq)]
// Sem alocador no espaço de usuário: variantes grandes (buffers embutidos) são inerentes.
#[allow(clippy::large_enum_variant)]
pub enum Request {
    /// `info`.
    Info(InfoRequest),
    /// `resolve`.
    Resolve(ResolveRequest),
    /// `udp_send`.
    UdpSend(UdpSendRequest),
    /// `udp_recv`.
    UdpRecv(UdpRecvRequest),
    /// `tcp_connect`.
    TcpConnect(TcpConnectRequest),
    /// `tcp_send`.
    TcpSend(TcpSendRequest),
    /// `tcp_recv`.
    TcpRecv(TcpRecvRequest),
    /// `tcp_close`.
    TcpClose(TcpCloseRequest),
}

/// Decodifica um pedido completo (cabecalho + payload).
pub fn decode_request(msg: &[u8]) -> Result<Request, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.flags != 0 {
        return Err(ProtoError::Flags);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    match h.method_id {
        1 => Ok(Request::Info(InfoRequest::decode_payload(p)?)),
        2 => Ok(Request::Resolve(ResolveRequest::decode_payload(p)?)),
        3 => Ok(Request::UdpSend(UdpSendRequest::decode_payload(p)?)),
        4 => Ok(Request::UdpRecv(UdpRecvRequest::decode_payload(p)?)),
        5 => Ok(Request::TcpConnect(TcpConnectRequest::decode_payload(p)?)),
        6 => Ok(Request::TcpSend(TcpSendRequest::decode_payload(p)?)),
        7 => Ok(Request::TcpRecv(TcpRecvRequest::decode_payload(p)?)),
        8 => Ok(Request::TcpClose(TcpCloseRequest::decode_payload(p)?)),
        _ => Err(ProtoError::Method),
    }
}

/// Decodifica a resposta de `info` (erro remoto vira `ProtoError::Remote`).
pub fn decode_info_response(msg: &[u8]) -> Result<InfoResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 1 {
        return Err(ProtoError::Method);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    if h.flags & FLAG_ERROR != 0 {
        let code = if p.len() >= 4 {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            0
        };
        return Err(ProtoError::Remote(code));
    }
    if h.flags != FLAG_RESPONSE {
        return Err(ProtoError::Flags);
    }
    InfoResponse::decode_payload(p)
}

/// Decodifica a resposta de `resolve` (erro remoto vira `ProtoError::Remote`).
pub fn decode_resolve_response(msg: &[u8]) -> Result<ResolveResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 2 {
        return Err(ProtoError::Method);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    if h.flags & FLAG_ERROR != 0 {
        let code = if p.len() >= 4 {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            0
        };
        return Err(ProtoError::Remote(code));
    }
    if h.flags != FLAG_RESPONSE {
        return Err(ProtoError::Flags);
    }
    ResolveResponse::decode_payload(p)
}

/// Decodifica a resposta de `udp_send` (erro remoto vira `ProtoError::Remote`).
pub fn decode_udp_send_response(msg: &[u8]) -> Result<UdpSendResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 3 {
        return Err(ProtoError::Method);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    if h.flags & FLAG_ERROR != 0 {
        let code = if p.len() >= 4 {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            0
        };
        return Err(ProtoError::Remote(code));
    }
    if h.flags != FLAG_RESPONSE {
        return Err(ProtoError::Flags);
    }
    UdpSendResponse::decode_payload(p)
}

/// Decodifica a resposta de `udp_recv` (erro remoto vira `ProtoError::Remote`).
pub fn decode_udp_recv_response(msg: &[u8]) -> Result<UdpRecvResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 4 {
        return Err(ProtoError::Method);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    if h.flags & FLAG_ERROR != 0 {
        let code = if p.len() >= 4 {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            0
        };
        return Err(ProtoError::Remote(code));
    }
    if h.flags != FLAG_RESPONSE {
        return Err(ProtoError::Flags);
    }
    UdpRecvResponse::decode_payload(p)
}

/// Decodifica a resposta de `tcp_connect` (erro remoto vira `ProtoError::Remote`).
pub fn decode_tcp_connect_response(msg: &[u8]) -> Result<TcpConnectResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 5 {
        return Err(ProtoError::Method);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    if h.flags & FLAG_ERROR != 0 {
        let code = if p.len() >= 4 {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            0
        };
        return Err(ProtoError::Remote(code));
    }
    if h.flags != FLAG_RESPONSE {
        return Err(ProtoError::Flags);
    }
    TcpConnectResponse::decode_payload(p)
}

/// Decodifica a resposta de `tcp_send` (erro remoto vira `ProtoError::Remote`).
pub fn decode_tcp_send_response(msg: &[u8]) -> Result<TcpSendResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 6 {
        return Err(ProtoError::Method);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    if h.flags & FLAG_ERROR != 0 {
        let code = if p.len() >= 4 {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            0
        };
        return Err(ProtoError::Remote(code));
    }
    if h.flags != FLAG_RESPONSE {
        return Err(ProtoError::Flags);
    }
    TcpSendResponse::decode_payload(p)
}

/// Decodifica a resposta de `tcp_recv` (erro remoto vira `ProtoError::Remote`).
pub fn decode_tcp_recv_response(msg: &[u8]) -> Result<TcpRecvResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 7 {
        return Err(ProtoError::Method);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    if h.flags & FLAG_ERROR != 0 {
        let code = if p.len() >= 4 {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            0
        };
        return Err(ProtoError::Remote(code));
    }
    if h.flags != FLAG_RESPONSE {
        return Err(ProtoError::Flags);
    }
    TcpRecvResponse::decode_payload(p)
}

/// Decodifica a resposta de `tcp_close` (erro remoto vira `ProtoError::Remote`).
pub fn decode_tcp_close_response(msg: &[u8]) -> Result<TcpCloseResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 8 {
        return Err(ProtoError::Method);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    if h.flags & FLAG_ERROR != 0 {
        let code = if p.len() >= 4 {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            0
        };
        return Err(ProtoError::Remote(code));
    }
    if h.flags != FLAG_RESPONSE {
        return Err(ProtoError::Flags);
    }
    TcpCloseResponse::decode_payload(p)
}

/// Codifica uma resposta de erro para o metodo `method_id`.
pub fn encode_error(method_id: u32, code: u32, out: &mut [u8]) -> Result<usize, ProtoError> {
    if out.len() < HEADER_LEN + 4 {
        return Err(ProtoError::Short);
    }
    let h = Header {
        protocol_id: PROTOCOL_ID,
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        method_id,
        flags: FLAG_RESPONSE | FLAG_ERROR,
        payload_len: 4,
    };
    h.encode(out)?;
    out[HEADER_LEN..HEADER_LEN + 4].copy_from_slice(&code.to_le_bytes());
    Ok(HEADER_LEN + 4)
}
