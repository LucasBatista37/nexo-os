//! Protocolo tipado `nexo.block` v1.0 — **gerado por `tools/idlgen` de `idl/block.idl`; nao editar**.

use crate::{FLAG_ERROR, FLAG_RESPONSE, HEADER_LEN, Header, ProtoError};

/// FNV-1a de `"nexo.block"`.
pub const PROTOCOL_ID: u32 = 0xda8aae52;
/// Versao maior (incompatibilidades).
pub const VERSION_MAJOR: u16 = 1;
/// Versao menor (adicoes compativeis).
pub const VERSION_MINOR: u16 = 0;

/// `nexo.block.read` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadRequest {
    /// Campo `sector`.
    pub sector: u64,
    /// Campo `count`.
    pub count: u32,
}

impl ReadRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.sector.to_le_bytes());
        o += 8;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.count.to_le_bytes());
        o += 4;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let sector = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let count = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(ReadRequest { sector, count })
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

/// `nexo.block.read` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadResponse {
    /// Bytes de `data` (ate 3584).
    pub data: [u8; 3584],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl ReadResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(3584)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.data_len as usize;
        if n > 3584 {
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
        let mut data = [0u8; 3584];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 3584 {
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
        Ok(ReadResponse { data, data_len })
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

/// `nexo.block.write` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteRequest {
    /// Campo `sector`.
    pub sector: u64,
    /// Campo `count`.
    pub count: u32,
    /// Bytes de `data` (ate 3584).
    pub data: [u8; 3584],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl WriteRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 2;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(3584)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.sector.to_le_bytes());
        o += 8;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.count.to_le_bytes());
        o += 4;
        let n = self.data_len as usize;
        if n > 3584 {
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
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let sector = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let count = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let mut data = [0u8; 3584];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 3584 {
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
        Ok(WriteRequest {
            sector,
            count,
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

/// `nexo.block.write` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteResponse {}

impl WriteResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 2;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(WriteResponse {})
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

/// `nexo.block.capacity` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapacityRequest {}

impl CapacityRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(CapacityRequest {})
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

/// `nexo.block.capacity` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapacityResponse {
    /// Campo `sectors`.
    pub sectors: u64,
}

impl CapacityResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.sectors.to_le_bytes());
        o += 8;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let sectors = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        let _ = o;
        Ok(CapacityResponse { sectors })
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

/// `nexo.block.identity` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityRequest {}

impl IdentityRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 4;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(IdentityRequest {})
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

/// `nexo.block.identity` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityResponse {
    /// Campo `read_only`.
    pub read_only: u8,
    /// Bytes de `serial` (ate 20).
    pub serial: [u8; 20],
    /// Tamanho valido de `serial`.
    pub serial_len: u32,
}

impl IdentityResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 4;
    /// Fatia valida de `serial`.
    pub fn serial(&self) -> &[u8] {
        &self.serial[..(self.serial_len as usize).min(20)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.read_only.to_le_bytes());
        o += 1;
        let n = self.serial_len as usize;
        if n > 20 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.serial[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let read_only = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let mut serial = [0u8; 20];
        let serial_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 20 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            serial[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            serial_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(IdentityResponse {
            read_only,
            serial,
            serial_len,
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

/// Pedido decodificado.
#[derive(Clone, Debug, PartialEq, Eq)]
// Sem alocador no espaço de usuário: variantes grandes (buffers embutidos) são inerentes.
#[allow(clippy::large_enum_variant)]
pub enum Request {
    /// `read`.
    Read(ReadRequest),
    /// `write`.
    Write(WriteRequest),
    /// `capacity`.
    Capacity(CapacityRequest),
    /// `identity`.
    Identity(IdentityRequest),
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
        1 => Ok(Request::Read(ReadRequest::decode_payload(p)?)),
        2 => Ok(Request::Write(WriteRequest::decode_payload(p)?)),
        3 => Ok(Request::Capacity(CapacityRequest::decode_payload(p)?)),
        4 => Ok(Request::Identity(IdentityRequest::decode_payload(p)?)),
        _ => Err(ProtoError::Method),
    }
}

/// Decodifica a resposta de `read` (erro remoto vira `ProtoError::Remote`).
pub fn decode_read_response(msg: &[u8]) -> Result<ReadResponse, ProtoError> {
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
    ReadResponse::decode_payload(p)
}

/// Decodifica a resposta de `write` (erro remoto vira `ProtoError::Remote`).
pub fn decode_write_response(msg: &[u8]) -> Result<WriteResponse, ProtoError> {
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
    WriteResponse::decode_payload(p)
}

/// Decodifica a resposta de `capacity` (erro remoto vira `ProtoError::Remote`).
pub fn decode_capacity_response(msg: &[u8]) -> Result<CapacityResponse, ProtoError> {
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
    CapacityResponse::decode_payload(p)
}

/// Decodifica a resposta de `identity` (erro remoto vira `ProtoError::Remote`).
pub fn decode_identity_response(msg: &[u8]) -> Result<IdentityResponse, ProtoError> {
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
    IdentityResponse::decode_payload(p)
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
