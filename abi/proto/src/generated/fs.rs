//! Protocolo tipado `nexo.fs` v1.0 — **gerado por `tools/idlgen` de `idl/fs.idl`; nao editar**.

#[allow(unused_imports)]
use crate::{FLAG_ERROR, FLAG_EVENT, FLAG_RESPONSE, HEADER_LEN, Header, ProtoError};

/// FNV-1a de `"nexo.fs"`.
pub const PROTOCOL_ID: u32 = 0x2d3847ce;
/// Versao maior (incompatibilidades).
pub const VERSION_MAJOR: u16 = 1;
/// Versao menor (adicoes compativeis).
pub const VERSION_MINOR: u16 = 0;

/// `nexo.fs.stat` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatRequest {
    /// Bytes de `path` (ate 256).
    pub path: [u8; 256],
    /// Tamanho valido de `path`.
    pub path_len: u32,
}

impl StatRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `path`.
    pub fn path(&self) -> &[u8] {
        &self.path[..(self.path_len as usize).min(256)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.path_len as usize;
        if n > 256 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.path[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut path = [0u8; 256];
        let path_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 256 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            path[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            path_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(StatRequest { path, path_len })
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

/// `nexo.fs.stat` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatResponse {
    /// Campo `ino`.
    pub ino: u32,
    /// Campo `kind`.
    pub kind: u8,
    /// Campo `size`.
    pub size: u64,
}

impl StatResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.ino.to_le_bytes());
        o += 4;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.kind.to_le_bytes());
        o += 1;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.size.to_le_bytes());
        o += 8;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let ino = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let kind = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let size = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        let _ = o;
        Ok(StatResponse { ino, kind, size })
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

/// `nexo.fs.create` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateRequest {
    /// Bytes de `path` (ate 256).
    pub path: [u8; 256],
    /// Tamanho valido de `path`.
    pub path_len: u32,
}

impl CreateRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 2;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `path`.
    pub fn path(&self) -> &[u8] {
        &self.path[..(self.path_len as usize).min(256)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.path_len as usize;
        if n > 256 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.path[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut path = [0u8; 256];
        let path_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 256 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            path[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            path_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(CreateRequest { path, path_len })
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

/// `nexo.fs.create` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateResponse {
    /// Campo `ino`.
    pub ino: u32,
}

impl CreateResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 2;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.ino.to_le_bytes());
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
        let ino = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(CreateResponse { ino })
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

/// `nexo.fs.mkdir` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MkdirRequest {
    /// Bytes de `path` (ate 256).
    pub path: [u8; 256],
    /// Tamanho valido de `path`.
    pub path_len: u32,
}

impl MkdirRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `path`.
    pub fn path(&self) -> &[u8] {
        &self.path[..(self.path_len as usize).min(256)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.path_len as usize;
        if n > 256 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.path[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut path = [0u8; 256];
        let path_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 256 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            path[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            path_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(MkdirRequest { path, path_len })
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

/// `nexo.fs.mkdir` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MkdirResponse {
    /// Campo `ino`.
    pub ino: u32,
}

impl MkdirResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.ino.to_le_bytes());
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
        let ino = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(MkdirResponse { ino })
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

/// `nexo.fs.unlink` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnlinkRequest {
    /// Bytes de `path` (ate 256).
    pub path: [u8; 256],
    /// Tamanho valido de `path`.
    pub path_len: u32,
}

impl UnlinkRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 4;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `path`.
    pub fn path(&self) -> &[u8] {
        &self.path[..(self.path_len as usize).min(256)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.path_len as usize;
        if n > 256 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.path[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut path = [0u8; 256];
        let path_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 256 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            path[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            path_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(UnlinkRequest { path, path_len })
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

/// `nexo.fs.unlink` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnlinkResponse {}

impl UnlinkResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 4;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(UnlinkResponse {})
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

/// `nexo.fs.read` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadRequest {
    /// Campo `ino`.
    pub ino: u32,
    /// Campo `offset`.
    pub offset: u64,
    /// Campo `len`.
    pub len: u32,
}

impl ReadRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 5;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.ino.to_le_bytes());
        o += 4;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.offset.to_le_bytes());
        o += 8;
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
        let ino = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let offset = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let len = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(ReadRequest { ino, offset, len })
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

/// `nexo.fs.read` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadResponse {
    /// Bytes de `data` (ate 4000).
    pub data: [u8; 4000],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl ReadResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 5;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(4000)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.data_len as usize;
        if n > 4000 {
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
        let mut data = [0u8; 4000];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 4000 {
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

/// `nexo.fs.write` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteRequest {
    /// Campo `ino`.
    pub ino: u32,
    /// Campo `offset`.
    pub offset: u64,
    /// Bytes de `data` (ate 3900).
    pub data: [u8; 3900],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl WriteRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 6;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(3900)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.ino.to_le_bytes());
        o += 4;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.offset.to_le_bytes());
        o += 8;
        let n = self.data_len as usize;
        if n > 3900 {
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
        let ino = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let offset = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        let mut data = [0u8; 3900];
        let data_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 3900 {
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
            ino,
            offset,
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

/// `nexo.fs.write` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteResponse {
    /// Campo `written`.
    pub written: u32,
}

impl WriteResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 6;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.written.to_le_bytes());
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
        let written = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(WriteResponse { written })
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

/// `nexo.fs.list` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRequest {
    /// Bytes de `path` (ate 256).
    pub path: [u8; 256],
    /// Tamanho valido de `path`.
    pub path_len: u32,
}

impl ListRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 7;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `path`.
    pub fn path(&self) -> &[u8] {
        &self.path[..(self.path_len as usize).min(256)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.path_len as usize;
        if n > 256 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.path[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut path = [0u8; 256];
        let path_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 256 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            path[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            path_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(ListRequest { path, path_len })
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

/// `nexo.fs.list` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListResponse {
    /// Campo `count`.
    pub count: u32,
    /// Bytes de `entries` (ate 3900).
    pub entries: [u8; 3900],
    /// Tamanho valido de `entries`.
    pub entries_len: u32,
}

impl ListResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 7;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `entries`.
    pub fn entries(&self) -> &[u8] {
        &self.entries[..(self.entries_len as usize).min(3900)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.count.to_le_bytes());
        o += 4;
        let n = self.entries_len as usize;
        if n > 3900 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.entries[..n]);
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
        let count = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let mut entries = [0u8; 3900];
        let entries_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 3900 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            entries[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            entries_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(ListResponse {
            count,
            entries,
            entries_len,
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

/// `nexo.fs.sync` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncRequest {}

impl SyncRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 8;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(SyncRequest {})
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

/// `nexo.fs.sync` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncResponse {}

impl SyncResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 8;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(SyncResponse {})
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

/// `nexo.fs.info` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoRequest {}

impl InfoRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 9;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
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

/// `nexo.fs.info` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoResponse {
    /// Campo `total_blocks`.
    pub total_blocks: u64,
    /// Campo `free_blocks`.
    pub free_blocks: u64,
    /// Campo `repairs`.
    pub repairs: u64,
    /// Campo `generation`.
    pub generation: u64,
}

impl InfoResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 9;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.total_blocks.to_le_bytes());
        o += 8;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.free_blocks.to_le_bytes());
        o += 8;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.repairs.to_le_bytes());
        o += 8;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.generation.to_le_bytes());
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
        let total_blocks = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let free_blocks = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let repairs = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let generation = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        let _ = o;
        Ok(InfoResponse {
            total_blocks,
            free_blocks,
            repairs,
            generation,
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

/// `nexo.fs.truncate` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TruncateRequest {
    /// Campo `ino`.
    pub ino: u32,
    /// Campo `size`.
    pub size: u64,
}

impl TruncateRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 10;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.ino.to_le_bytes());
        o += 4;
        if o + 8 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 8].copy_from_slice(&self.size.to_le_bytes());
        o += 8;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let ino = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 8 > b.len() {
            return Err(ProtoError::Short);
        }
        let size = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        o += 8;
        let _ = o;
        Ok(TruncateRequest { ino, size })
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

/// `nexo.fs.truncate` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TruncateResponse {}

impl TruncateResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 10;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(TruncateResponse {})
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
    /// `stat`.
    Stat(StatRequest),
    /// `create`.
    Create(CreateRequest),
    /// `mkdir`.
    Mkdir(MkdirRequest),
    /// `unlink`.
    Unlink(UnlinkRequest),
    /// `read`.
    Read(ReadRequest),
    /// `write`.
    Write(WriteRequest),
    /// `list`.
    List(ListRequest),
    /// `sync`.
    Sync(SyncRequest),
    /// `info`.
    Info(InfoRequest),
    /// `truncate`.
    Truncate(TruncateRequest),
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
        1 => Ok(Request::Stat(StatRequest::decode_payload(p)?)),
        2 => Ok(Request::Create(CreateRequest::decode_payload(p)?)),
        3 => Ok(Request::Mkdir(MkdirRequest::decode_payload(p)?)),
        4 => Ok(Request::Unlink(UnlinkRequest::decode_payload(p)?)),
        5 => Ok(Request::Read(ReadRequest::decode_payload(p)?)),
        6 => Ok(Request::Write(WriteRequest::decode_payload(p)?)),
        7 => Ok(Request::List(ListRequest::decode_payload(p)?)),
        8 => Ok(Request::Sync(SyncRequest::decode_payload(p)?)),
        9 => Ok(Request::Info(InfoRequest::decode_payload(p)?)),
        10 => Ok(Request::Truncate(TruncateRequest::decode_payload(p)?)),
        _ => Err(ProtoError::Method),
    }
}

/// Decodifica a resposta de `stat` (erro remoto vira `ProtoError::Remote`).
pub fn decode_stat_response(msg: &[u8]) -> Result<StatResponse, ProtoError> {
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
    StatResponse::decode_payload(p)
}

/// Decodifica a resposta de `create` (erro remoto vira `ProtoError::Remote`).
pub fn decode_create_response(msg: &[u8]) -> Result<CreateResponse, ProtoError> {
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
    CreateResponse::decode_payload(p)
}

/// Decodifica a resposta de `mkdir` (erro remoto vira `ProtoError::Remote`).
pub fn decode_mkdir_response(msg: &[u8]) -> Result<MkdirResponse, ProtoError> {
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
    MkdirResponse::decode_payload(p)
}

/// Decodifica a resposta de `unlink` (erro remoto vira `ProtoError::Remote`).
pub fn decode_unlink_response(msg: &[u8]) -> Result<UnlinkResponse, ProtoError> {
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
    UnlinkResponse::decode_payload(p)
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
    WriteResponse::decode_payload(p)
}

/// Decodifica a resposta de `list` (erro remoto vira `ProtoError::Remote`).
pub fn decode_list_response(msg: &[u8]) -> Result<ListResponse, ProtoError> {
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
    ListResponse::decode_payload(p)
}

/// Decodifica a resposta de `sync` (erro remoto vira `ProtoError::Remote`).
pub fn decode_sync_response(msg: &[u8]) -> Result<SyncResponse, ProtoError> {
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
    SyncResponse::decode_payload(p)
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
    if h.method_id != 9 {
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

/// Decodifica a resposta de `truncate` (erro remoto vira `ProtoError::Remote`).
pub fn decode_truncate_response(msg: &[u8]) -> Result<TruncateResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 10 {
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
    TruncateResponse::decode_payload(p)
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
