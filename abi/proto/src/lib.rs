//! Protocolos IPC tipados (ADR-0005, `docs/spec/ipc-compat.md`): o cabeçalho comum `NXIP`
//! (§2) e os módulos gerados por `tools/idlgen` a partir de `idl/*.idl` (`make idl`).
#![no_std]
#![forbid(unsafe_code)]

/// Assinatura `"NXIP"`.
pub const MAGIC: u32 = 0x4E58_4950;
/// Tamanho do cabeçalho.
pub const HEADER_LEN: usize = 24;
/// Flag: mensagem de resposta.
pub const FLAG_RESPONSE: u32 = 1;
/// Flag: resposta de erro (payload = código `u32`).
pub const FLAG_ERROR: u32 = 2;
/// Flag: evento (sem resposta).
pub const FLAG_EVENT: u32 = 4;
const KNOWN_FLAGS: u32 = FLAG_RESPONSE | FLAG_ERROR | FLAG_EVENT;

/// Erros de codificação/decodificação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtoError {
    /// Buffer curto demais (mensagem truncada ou espaço insuficiente).
    Short,
    /// Campo maior que o máximo da IDL.
    TooBig,
    /// Assinatura `NXIP` ausente.
    Magic,
    /// `protocol_id` desconhecido.
    Protocol,
    /// `version_major` não suportada.
    Version,
    /// `method_id` desconhecido.
    Method,
    /// Flags reservadas ou incoerentes.
    Flags,
    /// `payload_len` inconsistente com o tamanho recebido.
    Length,
    /// O serviço devolveu um erro (código do protocolo).
    Remote(u32),
}

/// Cabeçalho NXIP (24 bytes, little-endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Hash FNV-1a de `"nexo.<nome>"`.
    pub protocol_id: u32,
    /// Versão maior.
    pub version_major: u16,
    /// Versão menor.
    pub version_minor: u16,
    /// Número do método (estável para sempre).
    pub method_id: u32,
    /// Bit 0 resposta, bit 1 erro, bit 2 evento.
    pub flags: u32,
    /// Bytes após o cabeçalho.
    pub payload_len: u32,
}

impl Header {
    /// Codifica no início de `out`.
    pub fn encode(&self, out: &mut [u8]) -> Result<(), ProtoError> {
        if out.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        out[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        out[4..8].copy_from_slice(&self.protocol_id.to_le_bytes());
        out[8..10].copy_from_slice(&self.version_major.to_le_bytes());
        out[10..12].copy_from_slice(&self.version_minor.to_le_bytes());
        out[12..16].copy_from_slice(&self.method_id.to_le_bytes());
        out[16..20].copy_from_slice(&self.flags.to_le_bytes());
        out[20..24].copy_from_slice(&self.payload_len.to_le_bytes());
        Ok(())
    }

    /// Decodifica e valida (§2): magic, flags reservadas e `payload_len` exato.
    pub fn decode(msg: &[u8]) -> Result<Self, ProtoError> {
        if msg.len() < HEADER_LEN {
            return Err(ProtoError::Short);
        }
        let u32at = |o: usize| u32::from_le_bytes([msg[o], msg[o + 1], msg[o + 2], msg[o + 3]]);
        if u32at(0) != MAGIC {
            return Err(ProtoError::Magic);
        }
        let h = Header {
            protocol_id: u32at(4),
            version_major: u16::from_le_bytes([msg[8], msg[9]]),
            version_minor: u16::from_le_bytes([msg[10], msg[11]]),
            method_id: u32at(12),
            flags: u32at(16),
            payload_len: u32at(20),
        };
        if h.flags & !KNOWN_FLAGS != 0 {
            return Err(ProtoError::Flags);
        }
        if msg.len() != HEADER_LEN + h.payload_len as usize {
            return Err(ProtoError::Length);
        }
        Ok(h)
    }
}

pub mod generated;
pub use generated::*;

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn header_layout_and_roundtrip() {
        let h = Header {
            protocol_id: 0x1234_5678,
            version_major: 1,
            version_minor: 2,
            method_id: 7,
            flags: FLAG_RESPONSE,
            payload_len: 3,
        };
        let mut buf = [0u8; 27];
        h.encode(&mut buf).unwrap();
        assert_eq!(&buf[0..4], b"PIXN"); // "NXIP" (0x4E58_4950) em little-endian
        assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), MAGIC);
        assert_eq!(buf[12], 7);
        assert_eq!(Header::decode(&buf).unwrap(), h);
        // payload_len inconsistente
        assert_eq!(Header::decode(&buf[..26]), Err(ProtoError::Length));
        // flag reservada
        let mut bad = buf;
        bad[17] = 0x80;
        assert_eq!(Header::decode(&bad), Err(ProtoError::Flags));
        // magic
        let mut bad = buf;
        bad[0] = 0;
        assert_eq!(Header::decode(&bad), Err(ProtoError::Magic));
    }

    #[test]
    fn rng_roundtrip_and_compat() {
        use generated::rng::*;
        let req = FillRequest { len: 64 };
        let mut buf = [0u8; 128];
        let n = req.encode_msg(&mut buf).unwrap();
        assert_eq!(n, HEADER_LEN + 4);
        assert_eq!(
            decode_request(&buf[..n]).unwrap(),
            Request::Fill(req.clone())
        );
        // protocolo errado
        let mut bad = buf;
        bad[4] ^= 1;
        assert_eq!(decode_request(&bad[..n]), Err(ProtoError::Protocol));
        // versao maior desconhecida
        let mut bad = buf;
        bad[8] = 9;
        assert_eq!(decode_request(&bad[..n]), Err(ProtoError::Version));
        // metodo desconhecido
        let mut bad = buf;
        bad[12] = 99;
        assert_eq!(decode_request(&bad[..n]), Err(ProtoError::Method));
        // resposta com dados
        let mut resp = FillResponse {
            data: [0; 1024],
            data_len: 5,
        };
        resp.data[..5].copy_from_slice(b"abcde");
        let mut rbuf = [0u8; 2048];
        let rn = resp.encode_msg(&mut rbuf).unwrap();
        let got = decode_fill_response(&rbuf[..rn]).unwrap();
        assert_eq!(got.data(), b"abcde");
        // bytes extras no payload sao ignorados (leitor antigo <- escritor novo)
        let mut extended = std::vec::Vec::from(&rbuf[..rn]);
        extended.extend_from_slice(&7u64.to_le_bytes());
        let plen = (rn - HEADER_LEN + 8) as u32;
        extended[20..24].copy_from_slice(&plen.to_le_bytes());
        assert_eq!(decode_fill_response(&extended).unwrap().data(), b"abcde");
        // erro remoto
        let en = encode_error(FillRequest::METHOD_ID, 42, &mut rbuf).unwrap();
        assert_eq!(
            decode_fill_response(&rbuf[..en]),
            Err(ProtoError::Remote(42))
        );
    }

    #[test]
    fn fuzz_lite_decoders_never_panic() {
        use generated::rng::*;
        let mut resp = FillResponse {
            data: [0; 1024],
            data_len: 100,
        };
        resp.data[..100].fill(0xab);
        let mut buf = [0u8; 2048];
        let n = resp.encode_msg(&mut buf).unwrap();
        let mut seed = 0xdead_beef_cafe_f00du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20_000 {
            let mut m = buf;
            for _ in 0..1 + next() % 4 {
                let i = (next() % n as u64) as usize;
                m[i] ^= (next() % 255 + 1) as u8;
            }
            let cut = (next() % (n as u64 + 8)) as usize;
            let _ = decode_fill_response(&m[..cut.min(2048)]);
            let _ = decode_request(&m[..n]);
            let _ = Header::decode(&m[..n]);
        }
    }
}
