//! Máquina de estados TCP do lado cliente (`docs/spec/tcp-states.md`), **testável no host**:
//! não sabe de quadros nem de tempo — recebe segmentos decodificados ([`crate::TcpSegment`])
//! e o relógio em nanossegundos, e devolve segmentos a emitir ([`TxSeg`]). Com
//! **retransmissão**: um segmento pendente por vez (dados ou FIN), reenviado a cada
//! [`RTO_NS`] até [`MAX_RETRIES`]; depois a conexão é considerada reiniciada.

use crate::{TCP_ACK, TCP_FIN, TCP_PSH, TCP_RST, TCP_SYN, TcpSegment};

/// Estados implementados (subconjunto da RFC 9293; só o lado ativo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Sem conexão.
    Closed,
    /// SYN enviado; aguardando SYN-ACK.
    SynSent,
    /// Conexão aberta.
    Established,
    /// Nosso FIN enviado; aguardando o ACK dele.
    FinWait1,
    /// FIN confirmado; aguardando o FIN do par.
    FinWait2,
    /// Par fechou; podemos ainda enviar.
    CloseWait,
    /// FIN enviado depois do fecho do par; aguardando o último ACK.
    LastAck,
}

/// Tempo até reenviar o segmento pendente.
pub const RTO_NS: u64 = 500_000_000;
/// Reenvios antes de desistir (conexão vira `Closed` com `reset`).
pub const MAX_RETRIES: u32 = 5;
/// Capacidade do buffer de recepção.
pub const RX_CAP: usize = 4096;
/// Maior payload por segmento.
pub const MSS: usize = 1400;

/// Segmento a emitir (o chamador monta o quadro com [`crate::tcp_write`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxSeg {
    /// Número de sequência.
    pub seq: u32,
    /// Número de confirmação.
    pub ack: u32,
    /// Flags.
    pub flags: u8,
    /// Bytes de payload (fatia de [`TcpSocket::tx_payload`]).
    pub payload_len: usize,
}

/// Uma conexão TCP de saída.
pub struct TcpSocket {
    /// Estado corrente.
    pub state: State,
    /// A conexão foi reiniciada (RST ou retransmissões esgotadas).
    pub reset: bool,
    /// O par já enviou FIN.
    pub peer_closed: bool,
    /// Porta local.
    pub local_port: u16,
    /// IP remoto.
    pub remote_ip: [u8; 4],
    /// Porta remota.
    pub remote_port: u16,
    snd_nxt: u32,
    snd_una: u32,
    rcv_nxt: u32,
    // pendência de transmissão (dados ou FIN)
    tx: [u8; MSS],
    tx_len: usize,
    tx_seq: u32,
    tx_flags: u8,
    tx_deadline: u64,
    tx_retries: u32,
    tx_inflight: bool,
    rx: [u8; RX_CAP],
    rx_len: usize,
}

impl TcpSocket {
    /// Abre a conexão: devolve o socket em `SynSent` e o SYN a emitir.
    pub fn connect(
        local_port: u16,
        remote_ip: [u8; 4],
        remote_port: u16,
        iss: u32,
        now: u64,
    ) -> (Self, TxSeg) {
        let mut s = TcpSocket {
            state: State::SynSent,
            reset: false,
            peer_closed: false,
            local_port,
            remote_ip,
            remote_port,
            snd_nxt: iss.wrapping_add(1),
            snd_una: iss,
            rcv_nxt: 0,
            tx: [0; MSS],
            tx_len: 0,
            tx_seq: iss,
            tx_flags: TCP_SYN,
            tx_deadline: now + RTO_NS,
            tx_retries: 0,
            tx_inflight: true,
            rx: [0; RX_CAP],
            rx_len: 0,
        };
        s.tx_inflight = true;
        (
            s,
            TxSeg {
                seq: iss,
                ack: 0,
                flags: TCP_SYN,
                payload_len: 0,
            },
        )
    }

    /// Janela a anunciar.
    pub fn window(&self) -> u16 {
        (RX_CAP - self.rx_len) as u16
    }

    /// Payload do segmento pendente (para reemissão/emissão).
    pub fn tx_payload(&self) -> &[u8] {
        &self.tx[..self.tx_len]
    }

    /// Processa um segmento recebido desta conexão; devolve o segmento de resposta, se houver.
    pub fn on_segment(&mut self, seg: &TcpSegment<'_>, _now: u64) -> Option<TxSeg> {
        if seg.flags & TCP_RST != 0 {
            self.state = State::Closed;
            self.reset = true;
            self.tx_inflight = false;
            return None;
        }
        // Confirmações liberam a pendência.
        if seg.flags & TCP_ACK != 0 && self.tx_inflight {
            let end = self.tx_seq.wrapping_add(self.tx_len as u32).wrapping_add(
                if self.tx_flags & (TCP_SYN | TCP_FIN) != 0 {
                    1
                } else {
                    0
                },
            );
            if ack_covers(seg.ack, end, self.snd_una) {
                self.tx_inflight = false;
                self.snd_una = seg.ack;
                self.tx_retries = 0;
            }
        }
        match self.state {
            State::SynSent => {
                if seg.flags & (TCP_SYN | TCP_ACK) == TCP_SYN | TCP_ACK && seg.ack == self.snd_nxt {
                    self.rcv_nxt = seg.seq.wrapping_add(1);
                    self.state = State::Established;
                    return Some(self.ack_seg());
                }
                None
            }
            State::Established | State::FinWait1 | State::FinWait2 => {
                let mut advanced = false;
                if !seg.payload.is_empty() && seg.seq == self.rcv_nxt {
                    let take = seg.payload.len().min(RX_CAP - self.rx_len);
                    if take == seg.payload.len() {
                        self.rx[self.rx_len..self.rx_len + take].copy_from_slice(seg.payload);
                        self.rx_len += take;
                        self.rcv_nxt = self.rcv_nxt.wrapping_add(take as u32);
                        advanced = true;
                    }
                    // sem espaço ou fora de ordem: não confirma; o par retransmite
                } else if !seg.payload.is_empty() && seg.seq != self.rcv_nxt {
                    // duplicado/fora de ordem: reenvia o ACK corrente
                    return Some(self.ack_seg());
                }
                if seg.flags & TCP_FIN != 0
                    && seg.seq.wrapping_add(seg.payload.len() as u32) == self.rcv_nxt
                {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
                    self.peer_closed = true;
                    advanced = true;
                    self.state = match self.state {
                        State::Established => State::CloseWait,
                        _ => State::Closed, // FIN do par após o nosso: fecho completo (TIME_WAIT imediato)
                    };
                }
                if self.state == State::FinWait1 && !self.tx_inflight {
                    self.state = if self.peer_closed {
                        State::Closed
                    } else {
                        State::FinWait2
                    };
                }
                if advanced { Some(self.ack_seg()) } else { None }
            }
            State::LastAck => {
                if !self.tx_inflight {
                    self.state = State::Closed;
                }
                None
            }
            State::CloseWait | State::Closed => None,
        }
    }

    /// Envia dados (um segmento pendente por vez). `Err(true)` = ocupado, `Err(false)` = não conectado.
    pub fn send(&mut self, data: &[u8], now: u64) -> Result<TxSeg, bool> {
        if self.state != State::Established && self.state != State::CloseWait {
            return Err(false);
        }
        if self.tx_inflight || data.is_empty() || data.len() > MSS {
            return Err(true);
        }
        self.tx[..data.len()].copy_from_slice(data);
        self.tx_len = data.len();
        self.tx_seq = self.snd_nxt;
        self.tx_flags = TCP_ACK | TCP_PSH;
        self.tx_deadline = now + RTO_NS;
        self.tx_retries = 0;
        self.tx_inflight = true;
        self.snd_nxt = self.snd_nxt.wrapping_add(data.len() as u32);
        Ok(TxSeg {
            seq: self.tx_seq,
            ack: self.rcv_nxt,
            flags: self.tx_flags,
            payload_len: self.tx_len,
        })
    }

    /// Inicia o fecho; devolve o FIN a emitir (ou `None` se não aplicável agora).
    pub fn close(&mut self, now: u64) -> Option<TxSeg> {
        match self.state {
            State::Established | State::CloseWait => {
                if self.tx_inflight {
                    return None; // espere a pendência ser confirmada e chame de novo
                }
                self.tx_len = 0;
                self.tx_seq = self.snd_nxt;
                self.tx_flags = TCP_FIN | TCP_ACK;
                self.tx_deadline = now + RTO_NS;
                self.tx_retries = 0;
                self.tx_inflight = true;
                self.snd_nxt = self.snd_nxt.wrapping_add(1);
                self.state = if self.state == State::Established {
                    State::FinWait1
                } else {
                    State::LastAck
                };
                Some(TxSeg {
                    seq: self.tx_seq,
                    ack: self.rcv_nxt,
                    flags: self.tx_flags,
                    payload_len: 0,
                })
            }
            _ => None,
        }
    }

    /// Temporizador: devolve a retransmissão devida, se houver (aplica [`MAX_RETRIES`]).
    pub fn poll(&mut self, now: u64) -> Option<TxSeg> {
        if !self.tx_inflight || self.state == State::Closed || now < self.tx_deadline {
            return None;
        }
        if self.tx_retries >= MAX_RETRIES {
            self.state = State::Closed;
            self.reset = true;
            self.tx_inflight = false;
            return None;
        }
        self.tx_retries += 1;
        self.tx_deadline = now + RTO_NS;
        Some(TxSeg {
            seq: self.tx_seq,
            ack: self.rcv_nxt,
            flags: self.tx_flags,
            payload_len: self.tx_len,
        })
    }

    /// Retira dados recebidos; devolve quantos bytes copiou.
    pub fn take_rx(&mut self, out: &mut [u8]) -> usize {
        let take = self.rx_len.min(out.len());
        out[..take].copy_from_slice(&self.rx[..take]);
        self.rx.copy_within(take..self.rx_len, 0);
        self.rx_len -= take;
        take
    }

    fn ack_seg(&self) -> TxSeg {
        TxSeg {
            seq: self.snd_nxt,
            ack: self.rcv_nxt,
            flags: TCP_ACK,
            payload_len: 0,
        }
    }
}

/// `ack` confirma `end` partindo de `una` (aritmética modular).
fn ack_covers(ack: u32, end: u32, una: u32) -> bool {
    ack.wrapping_sub(una) >= end.wrapping_sub(una) && ack.wrapping_sub(una) <= 0x7fff_ffff
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    const IP: [u8; 4] = [10, 0, 2, 2];

    fn seg(seq: u32, ack: u32, flags: u8, payload: &[u8]) -> TcpSegment<'_> {
        TcpSegment {
            src_port: 80,
            dst_port: 41000,
            seq,
            ack,
            flags,
            window: 8192,
            payload,
        }
    }

    fn established(now: u64) -> TcpSocket {
        let (mut s, syn) = TcpSocket::connect(41000, IP, 80, 1000, now);
        assert_eq!(syn.flags, TCP_SYN);
        assert_eq!(syn.seq, 1000);
        let ack = s
            .on_segment(&seg(5000, 1001, TCP_SYN | TCP_ACK, b""), now)
            .unwrap();
        assert_eq!(s.state, State::Established);
        assert_eq!(ack.flags, TCP_ACK);
        assert_eq!(ack.ack, 5001);
        s
    }

    #[test]
    fn handshake_and_data_both_ways() {
        let mut s = established(0);
        // envia dados
        let tx = s.send(b"ola", 0).unwrap();
        assert_eq!((tx.seq, tx.payload_len), (1001, 3));
        assert_eq!(s.tx_payload(), b"ola");
        // segundo envio antes do ACK: ocupado
        assert_eq!(s.send(b"x", 0), Err(true));
        // ACK libera
        assert!(s.on_segment(&seg(5001, 1004, TCP_ACK, b""), 0).is_none());
        assert!(s.send(b"x", 0).is_ok());
        // dados do par em ordem
        let ack = s
            .on_segment(&seg(5001, 1005, TCP_ACK | TCP_PSH, b"resp"), 0)
            .unwrap();
        assert_eq!(ack.ack, 5005);
        let mut buf = [0u8; 16];
        assert_eq!(s.take_rx(&mut buf), 4);
        assert_eq!(&buf[..4], b"resp");
        // fora de ordem: reenvia o ACK corrente, sem avancar
        let dup = s
            .on_segment(&seg(9999, 1005, TCP_ACK | TCP_PSH, b"zzz"), 0)
            .unwrap();
        assert_eq!(dup.ack, 5005);
        assert_eq!(s.take_rx(&mut buf), 0);
    }

    #[test]
    fn retransmission_until_reset() {
        let mut s = established(0);
        let tx = s.send(b"dado", 0).unwrap();
        // sem ACK: retransmite o mesmo seq a cada RTO
        for i in 1..=MAX_RETRIES as u64 {
            let r = s.poll(i * RTO_NS).unwrap();
            assert_eq!(
                (r.seq, r.payload_len, r.flags),
                (tx.seq, 4, TCP_ACK | TCP_PSH)
            );
        }
        // esgotou: conexao reiniciada
        assert!(s.poll((MAX_RETRIES as u64 + 1) * RTO_NS).is_none());
        assert_eq!(s.state, State::Closed);
        assert!(s.reset);
    }

    #[test]
    fn retransmitted_syn_then_success() {
        let (mut s, _) = TcpSocket::connect(41000, IP, 80, 1000, 0);
        let r = s.poll(RTO_NS).unwrap();
        assert_eq!((r.seq, r.flags), (1000, TCP_SYN));
        let _ = s
            .on_segment(&seg(7000, 1001, TCP_SYN | TCP_ACK, b""), RTO_NS)
            .unwrap();
        assert_eq!(s.state, State::Established);
        assert!(s.poll(10 * RTO_NS).is_none()); // nada pendente
    }

    #[test]
    fn active_close_fin_wait() {
        let mut s = established(0);
        let fin = s.close(0).unwrap();
        assert_eq!((fin.flags, fin.seq), (TCP_FIN | TCP_ACK, 1001));
        assert_eq!(s.state, State::FinWait1);
        // ACK do FIN -> FIN_WAIT_2
        assert!(s.on_segment(&seg(5001, 1002, TCP_ACK, b""), 0).is_none());
        assert_eq!(s.state, State::FinWait2);
        // FIN do par -> ACK e CLOSED (TIME_WAIT imediato)
        let ack = s
            .on_segment(&seg(5001, 1002, TCP_FIN | TCP_ACK, b""), 0)
            .unwrap();
        assert_eq!(ack.ack, 5002);
        assert_eq!(s.state, State::Closed);
        assert!(!s.reset);
    }

    #[test]
    fn passive_close_close_wait_last_ack() {
        let mut s = established(0);
        // FIN do par com dados
        let ack = s
            .on_segment(&seg(5001, 1001, TCP_FIN | TCP_ACK | TCP_PSH, b"fim"), 0)
            .unwrap();
        assert_eq!(ack.ack, 5005);
        assert_eq!(s.state, State::CloseWait);
        assert!(s.peer_closed);
        // ainda podemos enviar em CLOSE_WAIT
        let tx = s.send(b"tchau", 0).unwrap();
        assert!(
            s.on_segment(&seg(5005, tx.seq.wrapping_add(5), TCP_ACK, b""), 0)
                .is_none()
        );
        // nosso FIN -> LAST_ACK -> ACK -> CLOSED
        let fin = s.close(0).unwrap();
        assert_eq!(s.state, State::LastAck);
        assert!(
            s.on_segment(&seg(5005, fin.seq.wrapping_add(1), TCP_ACK, b""), 0)
                .is_none()
        );
        assert_eq!(s.state, State::Closed);
    }

    #[test]
    fn rst_resets_any_state() {
        let mut s = established(0);
        assert!(s.on_segment(&seg(5001, 1001, TCP_RST, b""), 0).is_none());
        assert_eq!(s.state, State::Closed);
        assert!(s.reset);
        assert_eq!(s.send(b"x", 0), Err(false));
        assert!(s.close(0).is_none());
    }

    #[test]
    fn rx_backpressure_never_acks_what_it_dropped() {
        let mut s = established(0);
        // enche o buffer
        let big = [7u8; MSS];
        let mut seq = 5001u32;
        let mut acked = 0;
        while acked + MSS <= RX_CAP {
            let a = s.on_segment(&seg(seq, 1001, TCP_ACK, &big), 0).unwrap();
            seq = seq.wrapping_add(MSS as u32);
            acked += MSS;
            assert_eq!(a.ack, seq);
        }
        // proximo segmento nao cabe: sem ACK, rcv_nxt parado
        assert!(s.on_segment(&seg(seq, 1001, TCP_ACK, &big), 0).is_none());
        let mut sink = [0u8; RX_CAP];
        assert_eq!(s.take_rx(&mut sink), acked);
    }
}
