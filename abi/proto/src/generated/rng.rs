//! Protocolo tipado `nexo.rng` v1.0 — **gerado por `tools/idlgen` de `idl/rng.idl`; nao editar**.

#[allow(unused_imports)]
use crate::{FLAG_ERROR, FLAG_EVENT, FLAG_RESPONSE, HEADER_LEN, Header, ProtoError};

/// FNV-1a de `"nexo.rng"`.
pub const PROTOCOL_ID: u32 = 0xcd0e9a82;
/// Versao maior (incompatibilidades).
pub const VERSION_MAJOR: u16 = 1;
/// Versao menor (adicoes compativeis).
pub const VERSION_MINOR: u16 = 0;

/// `nexo.rng.fill` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillRequest {
    /// Campo `len`.
    pub len: u32,
}

impl FillRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.len.to_le_bytes());
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
        let len = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(FillRequest { len })
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

/// `nexo.rng.fill` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FillResponse {
    /// Bytes de `data` (ate 1024).
    pub data: [u8; 1024],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl FillResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(1024)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.data_len as usize;
        if n > 1024 {
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
        let mut data = [0u8; 1024];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 1024 {
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
        Ok(FillResponse { data, data_len })
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
    /// `fill`.
    Fill(FillRequest),
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
        1 => Ok(Request::Fill(FillRequest::decode_payload(p)?)),
        _ => Err(ProtoError::Method),
    }
}

/// Decodifica a resposta de `fill` (erro remoto vira `ProtoError::Remote`).
pub fn decode_fill_response(msg: &[u8]) -> Result<FillResponse, ProtoError> {
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
    FillResponse::decode_payload(p)
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
