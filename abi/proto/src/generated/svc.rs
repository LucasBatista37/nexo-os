//! Protocolo tipado `nexo.svc` v1.0 — **gerado por `tools/idlgen` de `idl/svc.idl`; nao editar**.

#[allow(unused_imports)]
use crate::{FLAG_ERROR, FLAG_EVENT, FLAG_RESPONSE, HEADER_LEN, Header, ProtoError};

/// FNV-1a de `"nexo.svc"`.
pub const PROTOCOL_ID: u32 = 0x549552dd;
/// Versao maior (incompatibilidades).
pub const VERSION_MAJOR: u16 = 1;
/// Versao menor (adicoes compativeis).
pub const VERSION_MINOR: u16 = 0;

/// `nexo.svc.serve` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServeRequest {
    /// Handle `chan` (viaja no vetor de handles, nunca no payload).
    pub chan: u32,
}

impl ServeRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
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
        Ok(ServeRequest { chan })
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

/// `nexo.svc.serve` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServeResponse {}

impl ServeResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 1;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, _out: &mut [u8]) -> Result<usize, ProtoError> {
        Ok(0)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(_b: &[u8]) -> Result<Self, ProtoError> {
        Ok(ServeResponse {})
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

/// `nexo.svc.connect` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectRequest {}

impl ConnectRequest {
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
        Ok(ConnectRequest {})
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

/// `nexo.svc.connect` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectResponse {
    /// Handle `chan` (viaja no vetor de handles, nunca no payload).
    pub chan: u32,
}

impl ConnectResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 2;
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
        Ok(ConnectResponse { chan })
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

/// `nexo.svc.echo` — pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EchoRequest {
    /// Bytes de `text` (ate 64).
    pub text: [u8; 64],
    /// Tamanho valido de `text`.
    pub text_len: u32,
}

impl EchoRequest {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `text`.
    pub fn text(&self) -> &[u8] {
        &self.text[..(self.text_len as usize).min(64)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.text_len as usize;
        if n > 64 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.text[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut text = [0u8; 64];
        let text_len: u32;
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
            text[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            text_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(EchoRequest { text, text_len })
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

/// `nexo.svc.echo` — resposta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EchoResponse {
    /// Bytes de `text` (ate 96).
    pub text: [u8; 96],
    /// Tamanho valido de `text`.
    pub text_len: u32,
}

impl EchoResponse {
    /// Numero do metodo.
    pub const METHOD_ID: u32 = 3;
    /// Handles que esta mensagem carrega no vetor de handles da mensagem.
    pub const HANDLE_COUNT: usize = 0;
    /// Fatia valida de `text`.
    pub fn text(&self) -> &[u8] {
        &self.text[..(self.text_len as usize).min(96)]
    }
    /// Codifica o payload; devolve o tamanho.
    pub fn encode_payload(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let mut o = 0usize;
        let n = self.text_len as usize;
        if n > 96 {
            return Err(ProtoError::TooBig);
        }
        if o + 4 + n > out.len() {
            return Err(ProtoError::Short);
        }
        out[o..o + 4].copy_from_slice(&(n as u32).to_le_bytes());
        out[o + 4..o + 4 + n].copy_from_slice(&self.text[..n]);
        o += 4 + n;
        Ok(o)
    }
    /// Decodifica o payload (bytes extras ao final sao ignorados; campos com padrao
    /// ausentes assumem o padrao — ipc-compat §3).
    pub fn decode_payload(b: &[u8]) -> Result<Self, ProtoError> {
        let mut o = 0usize;
        let mut text = [0u8; 96];
        let text_len: u32;
        {
            if o + 4 > b.len() {
                return Err(ProtoError::Short);
            }
            let l = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
            if l > 96 {
                return Err(ProtoError::TooBig);
            }
            if o + 4 + l > b.len() {
                return Err(ProtoError::Short);
            }
            text[..l].copy_from_slice(&b[o + 4..o + 4 + l]);
            text_len = l as u32;
            o += 4 + l;
        }
        let _ = o;
        Ok(EchoResponse { text, text_len })
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
    /// `serve`.
    Serve(ServeRequest),
    /// `connect`.
    Connect(ConnectRequest),
    /// `echo`.
    Echo(EchoRequest),
}

/// Decodifica um pedido injetando os handles recebidos (ordem de declaracao por metodo).
pub fn decode_request_with_handles(msg: &[u8], hs: &[u32]) -> Result<Request, ProtoError> {
    let mut r = decode_request(msg)?;
    match &mut r {
        Request::Serve(rq) => {
            if hs.len() != 1 {
                return Err(ProtoError::Length);
            }
            rq.chan = hs[0];
        }
        Request::Connect(_) => {
            if !hs.is_empty() {
                return Err(ProtoError::Length);
            }
        }
        Request::Echo(_) => {
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
        1 => Ok(Request::Serve(ServeRequest::decode_payload(p)?)),
        2 => Ok(Request::Connect(ConnectRequest::decode_payload(p)?)),
        3 => Ok(Request::Echo(EchoRequest::decode_payload(p)?)),
        _ => Err(ProtoError::Method),
    }
}

/// Decodifica a resposta de `serve` (erro remoto vira `ProtoError::Remote`).
pub fn decode_serve_response(msg: &[u8]) -> Result<ServeResponse, ProtoError> {
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
    ServeResponse::decode_payload(p)
}

/// Decodifica a resposta de `connect` (erro remoto vira `ProtoError::Remote`).
pub fn decode_connect_response(msg: &[u8]) -> Result<ConnectResponse, ProtoError> {
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
    ConnectResponse::decode_payload(p)
}

/// Decodifica a resposta de `echo` (erro remoto vira `ProtoError::Remote`).
pub fn decode_echo_response(msg: &[u8]) -> Result<EchoResponse, ProtoError> {
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
    EchoResponse::decode_payload(p)
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
