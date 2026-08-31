//! Protocolo tipado `nexo.wm` v1.13 — **gerado por `tools/idlgen` de `idl/wm.idl`; nao editar**.

#[allow(unused_imports)]
use crate::{FLAG_ERROR, FLAG_EVENT, FLAG_RESPONSE, HEADER_LEN, Header, ProtoError};

/// FNV-1a de `"nexo.wm"`.
pub const PROTOCOL_ID: u32 = 0x1b0edd71;
/// Versao maior (incompatibilidades).
pub const VERSION_MAJOR: u16 = 1;
/// Versao menor (adicoes compativeis).
pub const VERSION_MINOR: u16 = 13;

/// `nexo.wm.create_surface` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateSurfaceRequest {
    /// Campo `x`.
    pub x: i32,
    /// Campo `y`.
    pub y: i32,
    /// Campo `w`.
    pub w: i32,
    /// Campo `h`.
    pub h: i32,
    /// Campo `z`.
    pub z: i32,
    /// Campo `display`.
    pub display: u8,
}

impl CreateSurfaceRequest {
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
        out[o..o + 4].copy_from_slice(&self.x.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.y.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.w.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.h.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.z.to_le_bytes());
        o += 4;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.display.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let x = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let y = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let w = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let h = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let z = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let display = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(CreateSurfaceRequest {
            x,
            y,
            w,
            h,
            z,
            display,
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

/// `nexo.wm.create_surface` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateSurfaceResponse {
    /// Campo `id`.
    pub id: u32,
    /// Handle `mem` (viaja no vetor de handles, nunca no payload).
    pub mem: u32,
}

impl CreateSurfaceResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 1;
    /// Handles na ordem de declaracao (para passar ao `channel_send`).
    pub fn handles(&self) -> [u32; 1] {
        [self.mem]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let mem: u32 = 0; // injetado por decode_*_with_handles
        let _ = o;
        Ok(CreateSurfaceResponse { id, mem })
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

/// `nexo.wm.commit` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitRequest {
    /// Campo `id`.
    pub id: u32,
}

impl CommitRequest {
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
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(CommitRequest { id })
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

/// `nexo.wm.commit` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitResponse {}

impl CommitResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 2;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(CommitResponse {})
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

/// `nexo.wm.move` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveRequest {
    /// Campo `id`.
    pub id: u32,
    /// Campo `x`.
    pub x: i32,
    /// Campo `y`.
    pub y: i32,
}

impl MoveRequest {
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
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.x.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.y.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let x = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let y = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(MoveRequest { id, x, y })
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

/// `nexo.wm.move` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveResponse {}

impl MoveResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(MoveResponse {})
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

/// `nexo.wm.destroy` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DestroyRequest {
    /// Campo `id`.
    pub id: u32,
}

impl DestroyRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 4;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(DestroyRequest { id })
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

/// `nexo.wm.destroy` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DestroyResponse {}

impl DestroyResponse {
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
        Ok(DestroyResponse {})
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

/// `nexo.wm.output` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputRequest {
    /// Campo `display`.
    pub display: u8,
}

impl OutputRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 5;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.display.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let display = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(OutputRequest { display })
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

/// `nexo.wm.output` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputResponse {
    /// Campo `w`.
    pub w: i32,
    /// Campo `h`.
    pub h: i32,
    /// Handle `mem` (viaja no vetor de handles, nunca no payload).
    pub mem: u32,
}

impl OutputResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 5;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 1;
    /// Handles na ordem de declaracao (para passar ao `channel_send`).
    pub fn handles(&self) -> [u32; 1] {
        [self.mem]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.w.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.h.to_le_bytes());
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
        let w = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let h = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let mem: u32 = 0; // injetado por decode_*_with_handles
        let _ = o;
        Ok(OutputResponse { w, h, mem })
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

/// `nexo.wm.move_to_display` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveToDisplayRequest {
    /// Campo `id`.
    pub id: u32,
    /// Campo `display`.
    pub display: u8,
}

impl MoveToDisplayRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 18;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
        o += 4;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.display.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let display = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(MoveToDisplayRequest { id, display })
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

/// `nexo.wm.move_to_display` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveToDisplayResponse {}

impl MoveToDisplayResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 18;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(MoveToDisplayResponse {})
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

/// `nexo.wm.open` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    /// Handle `chan` (viaja no vetor de handles, nunca no payload).
    pub chan: u32,
}

impl OpenRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 6;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 1;
    /// Handles na ordem de declaracao (para passar ao `channel_send`).
    pub fn handles(&self) -> [u32; 1] {
        [self.chan]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        let chan: u32 = 0; // injetado por decode_*_with_handles
        Ok(OpenRequest { chan })
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

/// `nexo.wm.open` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenResponse {}

impl OpenResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 6;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(OpenResponse {})
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

/// `nexo.wm.raise` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaiseRequest {
    /// Campo `id`.
    pub id: u32,
}

impl RaiseRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 7;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(RaiseRequest { id })
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

/// `nexo.wm.raise` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaiseResponse {}

impl RaiseResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 7;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(RaiseResponse {})
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

/// `nexo.wm.lower` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LowerRequest {
    /// Campo `id`.
    pub id: u32,
}

impl LowerRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 8;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(LowerRequest { id })
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

/// `nexo.wm.lower` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LowerResponse {}

impl LowerResponse {
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
        Ok(LowerResponse {})
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

/// `nexo.wm.resize` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResizeRequest {
    /// Campo `id`.
    pub id: u32,
    /// Campo `w`.
    pub w: i32,
    /// Campo `h`.
    pub h: i32,
}

impl ResizeRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 9;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.w.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.h.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let w = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let h = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(ResizeRequest { id, w, h })
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

/// `nexo.wm.resize` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResizeResponse {
    /// Handle `mem` (viaja no vetor de handles, nunca no payload).
    pub mem: u32,
}

impl ResizeResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 9;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 1;
    /// Handles na ordem de declaracao (para passar ao `channel_send`).
    pub fn handles(&self) -> [u32; 1] {
        [self.mem]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        let mem: u32 = 0; // injetado por decode_*_with_handles
        Ok(ResizeResponse { mem })
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

/// `nexo.wm.set_input` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetInputRequest {
    /// Handle `chan` (viaja no vetor de handles, nunca no payload).
    pub chan: u32,
}

impl SetInputRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 10;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 1;
    /// Handles na ordem de declaracao (para passar ao `channel_send`).
    pub fn handles(&self) -> [u32; 1] {
        [self.chan]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        let chan: u32 = 0; // injetado por decode_*_with_handles
        Ok(SetInputRequest { chan })
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

/// `nexo.wm.set_input` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetInputResponse {}

impl SetInputResponse {
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
        Ok(SetInputResponse {})
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

/// `nexo.wm.maximize` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaximizeRequest {
    /// Campo `id`.
    pub id: u32,
}

impl MaximizeRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 13;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(MaximizeRequest { id })
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

/// `nexo.wm.maximize` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaximizeResponse {
    /// Handle `mem` (viaja no vetor de handles, nunca no payload).
    pub mem: u32,
}

impl MaximizeResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 13;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 1;
    /// Handles na ordem de declaracao (para passar ao `channel_send`).
    pub fn handles(&self) -> [u32; 1] {
        [self.mem]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        let mem: u32 = 0; // injetado por decode_*_with_handles
        Ok(MaximizeResponse { mem })
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

/// `nexo.wm.restore` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreRequest {
    /// Campo `id`.
    pub id: u32,
}

impl RestoreRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 14;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(RestoreRequest { id })
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

/// `nexo.wm.restore` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreResponse {
    /// Handle `mem` (viaja no vetor de handles, nunca no payload).
    pub mem: u32,
}

impl RestoreResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 14;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 1;
    /// Handles na ordem de declaracao (para passar ao `channel_send`).
    pub fn handles(&self) -> [u32; 1] {
        [self.mem]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        let mem: u32 = 0; // injetado por decode_*_with_handles
        Ok(RestoreResponse { mem })
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

/// `nexo.wm.tile` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileRequest {}

impl TileRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 15;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(TileRequest {})
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

/// `nexo.wm.tile` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileResponse {}

impl TileResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 15;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(TileResponse {})
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

/// `nexo.wm.grab` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrabRequest {
    /// Campo `id`.
    pub id: u32,
}

impl GrabRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 16;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(GrabRequest { id })
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

/// `nexo.wm.grab` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrabResponse {}

impl GrabResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 16;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(GrabResponse {})
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

/// `nexo.wm.ungrab` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UngrabRequest {
    /// Campo `id`.
    pub id: u32,
}

impl UngrabRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 17;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
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
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(UngrabRequest { id })
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

/// `nexo.wm.ungrab` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UngrabResponse {}

impl UngrabResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 17;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(UngrabResponse {})
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

/// `nexo.wm.set_context` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetContextRequest {
    /// Campo `id`.
    pub id: u32,
    /// Campo `context`.
    pub context: u8,
}

impl SetContextRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 19;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
        o += 4;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.context.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let context = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(SetContextRequest { id, context })
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

/// `nexo.wm.set_context` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetContextResponse {}

impl SetContextResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 19;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(SetContextResponse {})
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

/// `nexo.wm.switch_context` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchContextRequest {
    /// Campo `context`.
    pub context: u8,
}

impl SwitchContextRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 20;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.context.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let context = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(SwitchContextRequest { context })
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

/// `nexo.wm.switch_context` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchContextResponse {}

impl SwitchContextResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 20;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(SwitchContextResponse {})
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

/// `nexo.wm.clipboard_set` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardSetRequest {
    /// Bytes de `data` (ate 256).
    pub data: [u8; 256],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl ClipboardSetRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 21;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(256)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.data_len as usize;
        if n > 256 {
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
        let mut data = [0u8; 256];
        let data_len: u32;
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
            data[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            data_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(ClipboardSetRequest { data, data_len })
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

/// `nexo.wm.clipboard_set` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardSetResponse {}

impl ClipboardSetResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 21;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(ClipboardSetResponse {})
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

/// `nexo.wm.clipboard_get` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardGetRequest {}

impl ClipboardGetRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 22;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(ClipboardGetRequest {})
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

/// `nexo.wm.clipboard_get` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardGetResponse {
    /// Bytes de `data` (ate 256).
    pub data: [u8; 256],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl ClipboardGetResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 22;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(256)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.data_len as usize;
        if n > 256 {
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
        let mut data = [0u8; 256];
        let data_len: u32;
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
            data[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            data_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(ClipboardGetResponse { data, data_len })
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

/// `nexo.wm.clipboard_enable_history` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardEnableHistoryRequest {}

impl ClipboardEnableHistoryRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 23;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(ClipboardEnableHistoryRequest {})
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

/// `nexo.wm.clipboard_enable_history` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardEnableHistoryResponse {}

impl ClipboardEnableHistoryResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 23;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(ClipboardEnableHistoryResponse {})
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

/// `nexo.wm.clipboard_history` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardHistoryRequest {
    /// Campo `index`.
    pub index: u32,
}

impl ClipboardHistoryRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 24;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.index.to_le_bytes());
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
        let index = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(ClipboardHistoryRequest { index })
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

/// `nexo.wm.clipboard_history` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardHistoryResponse {
    /// Bytes de `data` (ate 256).
    pub data: [u8; 256],
    /// Tamanho valido de `data`.
    pub data_len: u32,
}

impl ClipboardHistoryResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 24;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `data`.
    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize).min(256)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.data_len as usize;
        if n > 256 {
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
        let mut data = [0u8; 256];
        let data_len: u32;
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
            data[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            data_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(ClipboardHistoryResponse { data, data_len })
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

/// `nexo.wm.notify` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifyRequest {
    /// Bytes de `title` (ate 64).
    pub title: [u8; 64],
    /// Tamanho valido de `title`.
    pub title_len: u32,
}

impl NotifyRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 25;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `title`.
    pub fn title(&self) -> &[u8] {
        &self.title[..(self.title_len as usize).min(64)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.title_len as usize;
        if n > 64 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.title[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut title = [0u8; 64];
        let title_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 64 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            title[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            title_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(NotifyRequest { title, title_len })
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

/// `nexo.wm.notify` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotifyResponse {}

impl NotifyResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 25;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(NotifyResponse {})
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

/// `nexo.wm.dismiss_notification` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DismissNotificationRequest {}

impl DismissNotificationRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 26;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(DismissNotificationRequest {})
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

/// `nexo.wm.dismiss_notification` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DismissNotificationResponse {}

impl DismissNotificationResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 26;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(DismissNotificationResponse {})
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

/// `nexo.wm.set_dnd` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetDndRequest {
    /// Campo `enabled`.
    pub enabled: u8,
}

impl SetDndRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 27;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.enabled.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let enabled = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(SetDndRequest { enabled })
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

/// `nexo.wm.set_dnd` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetDndResponse {}

impl SetDndResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 27;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(SetDndResponse {})
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

/// `nexo.wm.set_alpha` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetAlphaRequest {
    /// Campo `id`.
    pub id: u32,
    /// Campo `alpha`.
    pub alpha: u8,
}

impl SetAlphaRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 12;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.id.to_le_bytes());
        o += 4;
        if o + 1 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 1].copy_from_slice(&self.alpha.to_le_bytes());
        o += 1;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let id = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 1 > b.len() {
            return Err(ProtoError::Short);
        }
        let alpha = u8::from_le_bytes(b[o..o + 1].try_into().unwrap());
        o += 1;
        let _ = o;
        Ok(SetAlphaRequest { id, alpha })
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

/// `nexo.wm.set_alpha` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetAlphaResponse {}

impl SetAlphaResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 12;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(SetAlphaResponse {})
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

/// `nexo.wm.key` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// Campo `surface`.
    pub surface: u32,
    /// Campo `code`.
    pub code: u32,
    /// Campo `value`.
    pub value: u32,
}

impl KeyEvent {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 11;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.surface.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.code.to_le_bytes());
        o += 4;
        if o + 4 > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&self.value.to_le_bytes());
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
        let surface = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let code = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        if o + 4 > b.len() {
            return Err(ProtoError::Short);
        }
        let value = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        o += 4;
        let _ = o;
        Ok(KeyEvent {
            surface,
            code,
            value,
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
            flags: FLAG_EVENT,
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
    /// `create_surface`.
    CreateSurface(CreateSurfaceRequest),
    /// `commit`.
    Commit(CommitRequest),
    /// `move`.
    Move(MoveRequest),
    /// `destroy`.
    Destroy(DestroyRequest),
    /// `output`.
    Output(OutputRequest),
    /// `move_to_display`.
    MoveToDisplay(MoveToDisplayRequest),
    /// `open`.
    Open(OpenRequest),
    /// `raise`.
    Raise(RaiseRequest),
    /// `lower`.
    Lower(LowerRequest),
    /// `resize`.
    Resize(ResizeRequest),
    /// `set_input`.
    SetInput(SetInputRequest),
    /// `maximize`.
    Maximize(MaximizeRequest),
    /// `restore`.
    Restore(RestoreRequest),
    /// `tile`.
    Tile(TileRequest),
    /// `grab`.
    Grab(GrabRequest),
    /// `ungrab`.
    Ungrab(UngrabRequest),
    /// `set_context`.
    SetContext(SetContextRequest),
    /// `switch_context`.
    SwitchContext(SwitchContextRequest),
    /// `clipboard_set`.
    ClipboardSet(ClipboardSetRequest),
    /// `clipboard_get`.
    ClipboardGet(ClipboardGetRequest),
    /// `clipboard_enable_history`.
    ClipboardEnableHistory(ClipboardEnableHistoryRequest),
    /// `clipboard_history`.
    ClipboardHistory(ClipboardHistoryRequest),
    /// `notify`.
    Notify(NotifyRequest),
    /// `dismiss_notification`.
    DismissNotification(DismissNotificationRequest),
    /// `set_dnd`.
    SetDnd(SetDndRequest),
    /// `set_alpha`.
    SetAlpha(SetAlphaRequest),
}

/// Decodifica um pedido injetando os handles recebidos (ordem de declaracao por metodo).
pub fn decode_request_with_handles(msg: &[u8], hs: &[u32]) -> Result<Request, ProtoError> {
    let mut r = decode_request(msg)?;
    match &mut r {
        Request::CreateSurface(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Commit(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Move(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Destroy(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Output(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::MoveToDisplay(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Open(rq) => {
            if hs.len() != 1 {
                return Err(ProtoError::Length);
            }
            rq.chan = hs[0];
        }
        Request::Raise(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Lower(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Resize(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::SetInput(rq) => {
            if hs.len() != 1 {
                return Err(ProtoError::Length);
            }
            rq.chan = hs[0];
        }
        Request::Maximize(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Restore(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Tile(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Grab(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Ungrab(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::SetContext(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::SwitchContext(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::ClipboardSet(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::ClipboardGet(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::ClipboardEnableHistory(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::ClipboardHistory(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Notify(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::DismissNotification(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::SetDnd(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::SetAlpha(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
    }
    Ok(r)
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
        1 => Ok(Request::CreateSurface(
            CreateSurfaceRequest::decode_payload(p)?,
        )),
        2 => Ok(Request::Commit(CommitRequest::decode_payload(p)?)),
        3 => Ok(Request::Move(MoveRequest::decode_payload(p)?)),
        4 => Ok(Request::Destroy(DestroyRequest::decode_payload(p)?)),
        5 => Ok(Request::Output(OutputRequest::decode_payload(p)?)),
        18 => Ok(Request::MoveToDisplay(
            MoveToDisplayRequest::decode_payload(p)?,
        )),
        6 => Ok(Request::Open(OpenRequest::decode_payload(p)?)),
        7 => Ok(Request::Raise(RaiseRequest::decode_payload(p)?)),
        8 => Ok(Request::Lower(LowerRequest::decode_payload(p)?)),
        9 => Ok(Request::Resize(ResizeRequest::decode_payload(p)?)),
        10 => Ok(Request::SetInput(SetInputRequest::decode_payload(p)?)),
        13 => Ok(Request::Maximize(MaximizeRequest::decode_payload(p)?)),
        14 => Ok(Request::Restore(RestoreRequest::decode_payload(p)?)),
        15 => Ok(Request::Tile(TileRequest::decode_payload(p)?)),
        16 => Ok(Request::Grab(GrabRequest::decode_payload(p)?)),
        17 => Ok(Request::Ungrab(UngrabRequest::decode_payload(p)?)),
        19 => Ok(Request::SetContext(SetContextRequest::decode_payload(p)?)),
        20 => Ok(Request::SwitchContext(
            SwitchContextRequest::decode_payload(p)?,
        )),
        21 => Ok(Request::ClipboardSet(ClipboardSetRequest::decode_payload(
            p,
        )?)),
        22 => Ok(Request::ClipboardGet(ClipboardGetRequest::decode_payload(
            p,
        )?)),
        23 => Ok(Request::ClipboardEnableHistory(
            ClipboardEnableHistoryRequest::decode_payload(p)?,
        )),
        24 => Ok(Request::ClipboardHistory(
            ClipboardHistoryRequest::decode_payload(p)?,
        )),
        25 => Ok(Request::Notify(NotifyRequest::decode_payload(p)?)),
        26 => Ok(Request::DismissNotification(
            DismissNotificationRequest::decode_payload(p)?,
        )),
        27 => Ok(Request::SetDnd(SetDndRequest::decode_payload(p)?)),
        12 => Ok(Request::SetAlpha(SetAlphaRequest::decode_payload(p)?)),
        _ => Err(ProtoError::Method),
    }
}

/// Decodifica a resposta de `create_surface` (erro remoto vira `ProtoError::Remote`).
pub fn decode_create_surface_response(msg: &[u8]) -> Result<CreateSurfaceResponse, ProtoError> {
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
    CreateSurfaceResponse::decode_payload(p)
}

/// Decodifica a resposta de `commit` (erro remoto vira `ProtoError::Remote`).
pub fn decode_commit_response(msg: &[u8]) -> Result<CommitResponse, ProtoError> {
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
    CommitResponse::decode_payload(p)
}

/// Decodifica a resposta de `move` (erro remoto vira `ProtoError::Remote`).
pub fn decode_move_response(msg: &[u8]) -> Result<MoveResponse, ProtoError> {
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
    MoveResponse::decode_payload(p)
}

/// Decodifica a resposta de `destroy` (erro remoto vira `ProtoError::Remote`).
pub fn decode_destroy_response(msg: &[u8]) -> Result<DestroyResponse, ProtoError> {
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
    DestroyResponse::decode_payload(p)
}

/// Decodifica a resposta de `output` (erro remoto vira `ProtoError::Remote`).
pub fn decode_output_response(msg: &[u8]) -> Result<OutputResponse, ProtoError> {
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
    OutputResponse::decode_payload(p)
}

/// Decodifica a resposta de `move_to_display` (erro remoto vira `ProtoError::Remote`).
pub fn decode_move_to_display_response(msg: &[u8]) -> Result<MoveToDisplayResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 18 {
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
    MoveToDisplayResponse::decode_payload(p)
}

/// Decodifica a resposta de `open` (erro remoto vira `ProtoError::Remote`).
pub fn decode_open_response(msg: &[u8]) -> Result<OpenResponse, ProtoError> {
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
    OpenResponse::decode_payload(p)
}

/// Decodifica a resposta de `raise` (erro remoto vira `ProtoError::Remote`).
pub fn decode_raise_response(msg: &[u8]) -> Result<RaiseResponse, ProtoError> {
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
    RaiseResponse::decode_payload(p)
}

/// Decodifica a resposta de `lower` (erro remoto vira `ProtoError::Remote`).
pub fn decode_lower_response(msg: &[u8]) -> Result<LowerResponse, ProtoError> {
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
    LowerResponse::decode_payload(p)
}

/// Decodifica a resposta de `resize` (erro remoto vira `ProtoError::Remote`).
pub fn decode_resize_response(msg: &[u8]) -> Result<ResizeResponse, ProtoError> {
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
    ResizeResponse::decode_payload(p)
}

/// Decodifica a resposta de `set_input` (erro remoto vira `ProtoError::Remote`).
pub fn decode_set_input_response(msg: &[u8]) -> Result<SetInputResponse, ProtoError> {
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
    SetInputResponse::decode_payload(p)
}

/// Decodifica a resposta de `maximize` (erro remoto vira `ProtoError::Remote`).
pub fn decode_maximize_response(msg: &[u8]) -> Result<MaximizeResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 13 {
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
    MaximizeResponse::decode_payload(p)
}

/// Decodifica a resposta de `restore` (erro remoto vira `ProtoError::Remote`).
pub fn decode_restore_response(msg: &[u8]) -> Result<RestoreResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 14 {
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
    RestoreResponse::decode_payload(p)
}

/// Decodifica a resposta de `tile` (erro remoto vira `ProtoError::Remote`).
pub fn decode_tile_response(msg: &[u8]) -> Result<TileResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 15 {
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
    TileResponse::decode_payload(p)
}

/// Decodifica a resposta de `grab` (erro remoto vira `ProtoError::Remote`).
pub fn decode_grab_response(msg: &[u8]) -> Result<GrabResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 16 {
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
    GrabResponse::decode_payload(p)
}

/// Decodifica a resposta de `ungrab` (erro remoto vira `ProtoError::Remote`).
pub fn decode_ungrab_response(msg: &[u8]) -> Result<UngrabResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 17 {
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
    UngrabResponse::decode_payload(p)
}

/// Decodifica a resposta de `set_context` (erro remoto vira `ProtoError::Remote`).
pub fn decode_set_context_response(msg: &[u8]) -> Result<SetContextResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 19 {
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
    SetContextResponse::decode_payload(p)
}

/// Decodifica a resposta de `switch_context` (erro remoto vira `ProtoError::Remote`).
pub fn decode_switch_context_response(msg: &[u8]) -> Result<SwitchContextResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 20 {
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
    SwitchContextResponse::decode_payload(p)
}

/// Decodifica a resposta de `clipboard_set` (erro remoto vira `ProtoError::Remote`).
pub fn decode_clipboard_set_response(msg: &[u8]) -> Result<ClipboardSetResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 21 {
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
    ClipboardSetResponse::decode_payload(p)
}

/// Decodifica a resposta de `clipboard_get` (erro remoto vira `ProtoError::Remote`).
pub fn decode_clipboard_get_response(msg: &[u8]) -> Result<ClipboardGetResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 22 {
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
    ClipboardGetResponse::decode_payload(p)
}

/// Decodifica a resposta de `clipboard_enable_history` (erro remoto vira `ProtoError::Remote`).
pub fn decode_clipboard_enable_history_response(
    msg: &[u8],
) -> Result<ClipboardEnableHistoryResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 23 {
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
    ClipboardEnableHistoryResponse::decode_payload(p)
}

/// Decodifica a resposta de `clipboard_history` (erro remoto vira `ProtoError::Remote`).
pub fn decode_clipboard_history_response(
    msg: &[u8],
) -> Result<ClipboardHistoryResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 24 {
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
    ClipboardHistoryResponse::decode_payload(p)
}

/// Decodifica a resposta de `notify` (erro remoto vira `ProtoError::Remote`).
pub fn decode_notify_response(msg: &[u8]) -> Result<NotifyResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 25 {
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
    NotifyResponse::decode_payload(p)
}

/// Decodifica a resposta de `dismiss_notification` (erro remoto vira `ProtoError::Remote`).
pub fn decode_dismiss_notification_response(
    msg: &[u8],
) -> Result<DismissNotificationResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 26 {
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
    DismissNotificationResponse::decode_payload(p)
}

/// Decodifica a resposta de `set_dnd` (erro remoto vira `ProtoError::Remote`).
pub fn decode_set_dnd_response(msg: &[u8]) -> Result<SetDndResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 27 {
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
    SetDndResponse::decode_payload(p)
}

/// Decodifica a resposta de `set_alpha` (erro remoto vira `ProtoError::Remote`).
pub fn decode_set_alpha_response(msg: &[u8]) -> Result<SetAlphaResponse, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 12 {
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
    SetAlphaResponse::decode_payload(p)
}

/// Decodifica um evento `key` (mensagem sem resposta).
pub fn decode_key_event(msg: &[u8]) -> Result<KeyEvent, ProtoError> {
    let h = Header::decode(msg)?;
    if h.protocol_id != PROTOCOL_ID {
        return Err(ProtoError::Protocol);
    }
    if h.version_major != VERSION_MAJOR {
        return Err(ProtoError::Version);
    }
    if h.method_id != 11 {
        return Err(ProtoError::Method);
    }
    if h.flags != FLAG_EVENT {
        return Err(ProtoError::Flags);
    }
    let p = &msg[HEADER_LEN..HEADER_LEN + h.payload_len as usize];
    KeyEvent::decode_payload(p)
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
