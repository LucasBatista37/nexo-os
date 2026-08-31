//! Máquina de estados TCP (`docs/spec/tcp-states.md`), **testável no host**: não sabe de
//! quadros nem de tempo — recebe segmentos decodificados ([`crate::TcpSegment`]) e o relógio
//! em nanossegundos, e devolve segmentos a emitir ([`TxSeg`]). Lados ativo e passivo, com
//! **retransmissão e janela deslizante**: até [`TX_SLOTS`] segmentos em voo (dados, SYN ou
//! FIN), confirmados por ACKs cumulativos; o mais antigo vencido é reenviado a cada
//! [`RTO_NS`] até [`MAX_RETRIES`]; esgotando, a conexão é considerada reiniciada.

use crate::{TCP_ACK, TCP_FIN, TCP_PSH, TCP_RST, TCP_SYN, TcpSegment};

/// Estados implementados (subconjunto da RFC 9293; lados ativo e passivo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Sem conexão.
    Closed,
    /// Aguardando um SYN de entrada (lado passivo).
    Listen,
    /// SYN recebido; SYN-ACK enviado, aguardando o ACK final.
    SynRcvd,
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

/// Tempo até reenviar o segmento pendente mais antigo.
pub const RTO_NS: u64 = 500_000_000;
/// Reenvios de um mesmo segmento antes de desistir (conexão vira `Closed` com `reset`).
pub const MAX_RETRIES: u32 = 5;
/// Segmentos em voo simultâneos (janela de transmissão).
pub const TX_SLOTS: usize = 4;
/// Capacidade do buffer de recepção.
pub const RX_CAP: usize = 4096;
/// Maior payload por segmento.
pub const MSS: usize = 1400;

/// Segmento a emitir (o chamador monta o quadro com [`crate::tcp_write`]; o payload vem de
/// [`TcpSocket::slot_payload`] com o `slot` indicado).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxSeg {
    /// Número de sequência.
    pub seq: u32,
    /// Número de confirmação.
    pub ack: u32,
    /// Flags.
    pub flags: u8,
    /// Bytes de payload.
    pub payload_len: usize,
    /// Slot de transmissão dono do payload (`usize::MAX` = sem payload, ex.: ACK puro).
    pub slot: usize,
}

#[derive(Clone, Copy)]
struct TxPend {
    used: bool,
    seq: u32,
    len: u16,
    flags: u8,
    deadline: u64,
    retries: u32,
}

const PEND_FREE: TxPend = TxPend {
    used: false,
    seq: 0,
    len: 0,
    flags: 0,
    deadline: 0,
    retries: 0,
};

/// Uma conexão TCP (ativa ou passiva).
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
    tx: [[u8; MSS]; TX_SLOTS],
    pend: [TxPend; TX_SLOTS],
    rx: [u8; RX_CAP],
    rx_len: usize,
}

impl TcpSocket {
    fn blank(local_port: u16, iss: u32) -> Self {
        TcpSocket {
            state: State::Closed,
            reset: false,
            peer_closed: false,
            local_port,
            remote_ip: [0; 4],
            remote_port: 0,
            snd_nxt: iss.wrapping_add(1),
            snd_una: iss,
            rcv_nxt: 0,
            tx: [[0; MSS]; TX_SLOTS],
            pend: [PEND_FREE; TX_SLOTS],
            rx: [0; RX_CAP],
            rx_len: 0,
        }
    }

    /// Abre a conexão (lado ativo): devolve o socket em `SynSent` e o SYN a emitir.
    pub fn connect(
        local_port: u16,
        remote_ip: [u8; 4],
        remote_port: u16,
        iss: u32,
        now: u64,
    ) -> (Self, TxSeg) {
        let mut s = Self::blank(local_port, iss);
        s.state = State::SynSent;
        s.remote_ip = remote_ip;
        s.remote_port = remote_port;
        s.pend[0] = TxPend {
            used: true,
            seq: iss,
            len: 0,
            flags: TCP_SYN,
            deadline: now + RTO_NS,
            retries: 0,
        };
        (
            s,
            TxSeg {
                seq: iss,
                ack: 0,
                flags: TCP_SYN,
                payload_len: 0,
                slot: 0,
            },
        )
    }

    /// Escuta em `local_port` (lado passivo): devolve o socket em `Listen`.
    pub fn listen(local_port: u16, iss: u32) -> Self {
        let mut s = Self::blank(local_port, iss);
        s.state = State::Listen;
        s
    }

    /// Em `Listen`, aceita o SYN de `(src_ip, seg)`: devolve o SYN-ACK a emitir.
    pub fn on_syn(&mut self, src_ip: [u8; 4], seg: &TcpSegment<'_>, now: u64) -> Option<TxSeg> {
        if self.state != State::Listen || seg.flags & TCP_SYN == 0 || seg.flags & TCP_ACK != 0 {
            return None;
        }
        self.remote_ip = src_ip;
        self.remote_port = seg.src_port;
        self.rcv_nxt = seg.seq.wrapping_add(1);
        self.state = State::SynRcvd;
        // SYN-ACK é a pendência retransmissível
        self.pend[0] = TxPend {
            used: true,
            seq: self.snd_una,
            len: 0,
            flags: TCP_SYN | TCP_ACK,
            deadline: now + RTO_NS,
            retries: 0,
        };
        Some(TxSeg {
            seq: self.snd_una,
            ack: self.rcv_nxt,
            flags: TCP_SYN | TCP_ACK,
            payload_len: 0,
            slot: 0,
        })
    }

    /// Janela a anunciar.
    pub fn window(&self) -> u16 {
        (RX_CAP - self.rx_len) as u16
    }

    /// Payload do slot de transmissão (para emissão/reemissão).
    pub fn slot_payload(&self, slot: usize, len: usize) -> &[u8] {
        if slot >= TX_SLOTS {
            return &[];
        }
        &self.tx[slot][..len.min(MSS)]
    }

    /// Há segmentos em voo aguardando ACK?
    pub fn inflight(&self) -> bool {
        self.pend.iter().any(|p| p.used)
    }

    fn free_slot(&self) -> Option<usize> {
        self.pend.iter().position(|p| !p.used)
    }

    /// Processa um segmento recebido desta conexão; devolve o segmento de resposta, se houver.
    pub fn on_segment(&mut self, seg: &TcpSegment<'_>, _now: u64) -> Option<TxSeg> {
        if seg.flags & TCP_RST != 0 {
            self.state = State::Closed;
            self.reset = true;
            self.pend = [PEND_FREE; TX_SLOTS];
            return None;
        }
        // ACKs cumulativos: liberam todos os slots totalmente confirmados.
        if seg.flags & TCP_ACK != 0 {
            let adv = seg.ack.wrapping_sub(self.snd_una);
            if adv <= self.snd_nxt.wrapping_sub(self.snd_una) {
                for p in self.pend.iter_mut() {
                    if !p.used {
                        continue;
                    }
                    let end = p.seq.wrapping_add(p.len as u32).wrapping_add(
                        if p.flags & (TCP_SYN | TCP_FIN) != 0 {
                            1
                        } else {
                            0
                        },
                    );
                    if end.wrapping_sub(self.snd_una) <= adv {
                        p.used = false;
                    }
                }
                self.snd_una = seg.ack;
            }
        }
        match self.state {
            State::SynRcvd => {
                if seg.flags & TCP_ACK != 0 && seg.ack == self.snd_nxt {
                    self.state = State::Established;
                    // dados podem vir já neste segmento
                    if !seg.payload.is_empty() && seg.seq == self.rcv_nxt {
                        let take = seg.payload.len().min(RX_CAP - self.rx_len);
                        if take == seg.payload.len() {
                            self.rx[self.rx_len..self.rx_len + take].copy_from_slice(seg.payload);
                            self.rx_len += take;
                            self.rcv_nxt = self.rcv_nxt.wrapping_add(take as u32);
                            return Some(self.ack_seg());
                        }
                    }
                }
                None
            }
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
                    // sem espaço: não confirma; o par retransmite
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
                if self.state == State::FinWait1 && !self.inflight() {
                    self.state = if self.peer_closed {
                        State::Closed
                    } else {
                        State::FinWait2
                    };
                }
                if advanced { Some(self.ack_seg()) } else { None }
            }
            State::LastAck => {
                if !self.inflight() {
                    self.state = State::Closed;
                }
                None
            }
            State::CloseWait | State::Closed | State::Listen => None,
        }
    }

    /// Envia dados (até [`TX_SLOTS`] segmentos em voo). `Err(true)` = janela cheia,
    /// `Err(false)` = não conectado.
    pub fn send(&mut self, data: &[u8], now: u64) -> Result<TxSeg, bool> {
        if self.state != State::Established && self.state != State::CloseWait {
            return Err(false);
        }
        if data.is_empty() || data.len() > MSS {
            return Err(true);
        }
        let Some(slot) = self.free_slot() else {
            return Err(true);
        };
        self.tx[slot][..data.len()].copy_from_slice(data);
        let seq = self.snd_nxt;
        self.pend[slot] = TxPend {
            used: true,
            seq,
            len: data.len() as u16,
            flags: TCP_ACK | TCP_PSH,
            deadline: now + RTO_NS,
            retries: 0,
        };
        self.snd_nxt = self.snd_nxt.wrapping_add(data.len() as u32);
        Ok(TxSeg {
            seq,
            ack: self.rcv_nxt,
            flags: TCP_ACK | TCP_PSH,
            payload_len: data.len(),
            slot,
        })
    }

    /// Inicia o fecho; devolve o FIN a emitir (`None` = janela cheia ou estado não aplicável;
    /// com a janela cheia, espere ACKs e chame de novo).
    pub fn close(&mut self, now: u64) -> Option<TxSeg> {
        match self.state {
            State::Established | State::CloseWait => {
                let slot = self.free_slot()?;
                let seq = self.snd_nxt;
                self.pend[slot] = TxPend {
                    used: true,
                    seq,
                    len: 0,
                    flags: TCP_FIN | TCP_ACK,
                    deadline: now + RTO_NS,
                    retries: 0,
                };
                self.snd_nxt = self.snd_nxt.wrapping_add(1);
                self.state = if self.state == State::Established {
                    State::FinWait1
                } else {
                    State::LastAck
                };
                Some(TxSeg {
                    seq,
                    ack: self.rcv_nxt,
                    flags: TCP_FIN | TCP_ACK,
                    payload_len: 0,
                    slot,
                })
            }
            _ => None,
        }
    }

    /// Temporizador: devolve a retransmissão do slot mais antigo vencido (aplica [`MAX_RETRIES`]).
    pub fn poll(&mut self, now: u64) -> Option<TxSeg> {
        if self.state == State::Closed {
            return None;
        }
        let mut oldest: Option<usize> = None;
        for (i, p) in self.pend.iter().enumerate() {
            if !p.used || now < p.deadline {
                continue;
            }
            oldest = match oldest {
                None => Some(i),
                Some(o)
                    if p.seq.wrapping_sub(self.snd_una)
                        < self.pend[o].seq.wrapping_sub(self.snd_una) =>
                {
                    Some(i)
                }
                keep => keep,
            };
        }
        let i = oldest?;
        if self.pend[i].retries >= MAX_RETRIES {
            self.state = State::Closed;
            self.reset = true;
            self.pend = [PEND_FREE; TX_SLOTS];
            return None;
        }
        self.pend[i].retries += 1;
        self.pend[i].deadline = now + RTO_NS;
        Some(TxSeg {
            seq: self.pend[i].seq,
            ack: self.rcv_nxt,
            flags: self.pend[i].flags,
            payload_len: self.pend[i].len as usize,
            slot: i,
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
            slot: usize::MAX,
        }
    }
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
        assert!(!s.inflight());
        s
    }

    #[test]
    fn handshake_and_data_both_ways() {
        let mut s = established(0);
        let tx = s.send(b"ola", 0).unwrap();
        assert_eq!((tx.seq, tx.payload_len), (1001, 3));
        assert_eq!(s.slot_payload(tx.slot, tx.payload_len), b"ola");
        // ACK libera
        assert!(s.on_segment(&seg(5001, 1004, TCP_ACK, b""), 0).is_none());
        assert!(!s.inflight());
        // dados do par em ordem
        let ack = s
            .on_segment(&seg(5001, 1004, TCP_ACK | TCP_PSH, b"resp"), 0)
            .unwrap();
        assert_eq!(ack.ack, 5005);
        let mut buf = [0u8; 16];
        assert_eq!(s.take_rx(&mut buf), 4);
        assert_eq!(&buf[..4], b"resp");
        // fora de ordem: reenvia o ACK corrente, sem avancar
        let dup = s
            .on_segment(&seg(9999, 1004, TCP_ACK | TCP_PSH, b"zzz"), 0)
            .unwrap();
        assert_eq!(dup.ack, 5005);
        assert_eq!(s.take_rx(&mut buf), 0);
    }

    #[test]
    fn sliding_window_pipelines_and_partial_acks() {
        let mut s = established(0);
        // enche a janela: 4 segmentos em voo
        let t1 = s.send(b"aa", 0).unwrap();
        let t2 = s.send(b"bbb", 0).unwrap();
        let t3 = s.send(b"cccc", 0).unwrap();
        let t4 = s.send(b"d", 0).unwrap();
        assert_eq!((t1.seq, t2.seq, t3.seq, t4.seq), (1001, 1003, 1006, 1010));
        assert_eq!(s.send(b"x", 0), Err(true)); // janela cheia
        // ACK parcial cobre t1 e t2
        assert!(s.on_segment(&seg(5001, 1006, TCP_ACK, b""), 0).is_none());
        assert!(s.send(b"e", 0).is_ok()); // abriu espaco
        // ACK cumulativo cobre o resto
        assert!(s.on_segment(&seg(5001, 1012, TCP_ACK, b""), 0).is_none());
        assert!(!s.inflight());
    }

    #[test]
    fn retransmission_oldest_until_reset() {
        let mut s = established(0);
        let t1 = s.send(b"dado", 0).unwrap();
        let _t2 = s.send(b"mais", 0).unwrap();
        // sem ACK: o MAIS ANTIGO retransmite a cada RTO
        for i in 1..=MAX_RETRIES as u64 {
            let r = s.poll(i * RTO_NS).unwrap();
            assert_eq!(
                (r.seq, r.payload_len, r.flags),
                (t1.seq, 4, TCP_ACK | TCP_PSH)
            );
            assert_eq!(s.slot_payload(r.slot, r.payload_len), b"dado");
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
    fn data_then_fin_in_flight_together() {
        let mut s = established(0);
        let d = s.send(b"tchau", 0).unwrap();
        let fin = s.close(0).unwrap(); // FIN na janela junto com os dados
        assert_eq!(fin.seq, d.seq.wrapping_add(5));
        assert_eq!(s.state, State::FinWait1);
        // ACK cumulativo cobre dados + FIN de uma vez
        assert!(
            s.on_segment(&seg(5001, fin.seq.wrapping_add(1), TCP_ACK, b""), 0)
                .is_none()
        );
        assert_eq!(s.state, State::FinWait2);
    }

    #[test]
    fn passive_close_close_wait_last_ack() {
        let mut s = established(0);
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
    fn passive_open_listen_syn_rcvd() {
        let mut s = TcpSocket::listen(8080, 9000);
        assert_eq!(s.state, State::Listen);
        let syn = TcpSegment {
            src_port: 51000,
            dst_port: 8080,
            seq: 300,
            ack: 0,
            flags: TCP_SYN,
            window: 8192,
            payload: b"",
        };
        let sa = s.on_syn([10, 0, 2, 2], &syn, 0).unwrap();
        assert_eq!((sa.flags, sa.seq, sa.ack), (TCP_SYN | TCP_ACK, 9000, 301));
        assert_eq!(s.state, State::SynRcvd);
        // sem ACK: SYN-ACK retransmitido
        let r = s.poll(RTO_NS).unwrap();
        assert_eq!(r.flags, TCP_SYN | TCP_ACK);
        // ACK final com dados juntos
        let ack = s.on_segment(
            &TcpSegment {
                src_port: 51000,
                dst_port: 8080,
                seq: 301,
                ack: 9001,
                flags: TCP_ACK | TCP_PSH,
                window: 8192,
                payload: b"oi",
            },
            0,
        );
        assert_eq!(s.state, State::Established);
        assert_eq!(ack.unwrap().ack, 303);
        let mut buf = [0u8; 8];
        assert_eq!(s.take_rx(&mut buf), 2);
        assert_eq!(&buf[..2], b"oi");
        let tx = s.send(b"resp", 0).unwrap();
        assert_eq!(tx.seq, 9001);
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
        let big = [7u8; MSS];
        let mut seqn = 5001u32;
        let mut acked = 0;
        while acked + MSS <= RX_CAP {
            let a = s.on_segment(&seg(seqn, 1001, TCP_ACK, &big), 0).unwrap();
            seqn = seqn.wrapping_add(MSS as u32);
            acked += MSS;
            assert_eq!(a.ack, seqn);
        }
        assert!(s.on_segment(&seg(seqn, 1001, TCP_ACK, &big), 0).is_none());
        let mut sink = [0u8; RX_CAP];
        assert_eq!(s.take_rx(&mut sink), acked);
    }

    /// Fuzz de estados: sequências aleatórias de segmentos/ações contra a máquina, checando que
    /// ela nunca entra em pânico e mantém invariantes (Plano §Fase 4: "fuzzar estados de protocolo").
    #[test]
    fn fuzz_lite_state_machine_holds_invariants() {
        let mut seed = 0x00c0_ffee_1234_5678u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let payloads: [&[u8]; 4] = [b"", b"a", b"abcd", &[7u8; 200]];
        for _ in 0..3000 {
            // metade das conexões abre ativa, metade passiva
            let mut s = if next() & 1 == 0 {
                let (s, _) = TcpSocket::connect(41000, IP, 80, 1000, 0);
                s
            } else {
                TcpSocket::listen(41000, 2000)
            };
            let mut now = 0u64;
            for _ in 0..24 {
                match next() % 6 {
                    0 => {
                        let seq = (next() % 8000) as u32;
                        let ack = (next() % 8000) as u32;
                        let flags = (next() & 0x3f) as u8;
                        let p = payloads[(next() % 4) as usize];
                        // NS de entrada quando em Listen
                        if s.state == State::Listen && flags & TCP_SYN != 0 {
                            let seg = seg(seq, ack, TCP_SYN, p);
                            let _ = s.on_syn(IP, &seg, now);
                        } else {
                            let _ = s.on_segment(&seg(seq, ack, flags, p), now);
                        }
                    }
                    1 => {
                        let p = payloads[(next() % 4) as usize];
                        let _ = s.send(p, now);
                    }
                    2 => {
                        let _ = s.close(now);
                    }
                    3 => {
                        now += (next() % 3) * RTO_NS;
                        let _ = s.poll(now);
                    }
                    4 => {
                        let mut buf = [0u8; 64];
                        let _ = s.take_rx(&mut buf);
                    }
                    _ => {
                        let _ = s.window();
                        let _ = s.inflight();
                    }
                }
                // Invariantes: snd_una <= snd_nxt (janela); rx_len <= capacidade.
                assert!(
                    s.snd_nxt.wrapping_sub(s.snd_una) <= 0x8000_0000,
                    "snd_una passou de snd_nxt"
                );
                assert!(s.rx_len <= RX_CAP, "rx buffer estourou");
                let used = s.pend.iter().filter(|p| p.used).count();
                assert!(used <= TX_SLOTS, "mais pendencias que slots");
            }
        }
    }
}
