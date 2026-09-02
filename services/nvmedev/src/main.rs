//! `nvmedev` — driver NVMe em modo usuário (Plano §Fase 7: "implementar NVMe ou AHCI conforme o
//! computador de referência"; o QEMU `-device nvme` é a referência por ora). Mesmo modelo de
//! segurança dos demais drivers: concessão de UMA função PCI, BAR mapeado por `mmio_map`, DMA
//! por páginas concedidas — e o mesmo protocolo `nexo.block` v0 do `blockdev`, então a pilha
//! (fs, instalador, testes) roda sobre NVMe **sem mudar uma linha** — substituibilidade por
//! protocolo. MVP síncrono: fila de admin + um par de filas de E/S, um pedido em voo, PRP1
//! único (transfere ≤ 1 página por pedido — o `nexo.block` pede no máximo 3584 B).
//! Handle 0 = concessão do dispositivo; handle 1 = canal servindo `nexo.block`.
#![no_std]
#![no_main]

use nexo_proto::block::{
    self, CapacityResponse, IdentityResponse, ReadResponse, Request, WriteResponse,
};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};

const DEV: Handle = 0;
const CHAN: Handle = 1;
const MAX_SECTORS: usize = 7; // 3584 B por pedido, como no nexo.block

// Registradores NVMe (offsets no BAR0).
const R_CAP: u64 = 0x00;
const R_CC: u64 = 0x14;
const R_CSTS: u64 = 0x1c;
const R_AQA: u64 = 0x24;
const R_ASQ: u64 = 0x28;
const R_ACQ: u64 = 0x30;
const DOORBELLS: u64 = 0x1000;

const QD: usize = 64; // entradas por fila (SQE 64 B × 64 = 4096; CQE 16 B × 64 = 1024)

fn fail(code: i64, what: &str) -> ! {
    log!("nvmedev: falha: {}", what);
    nexo_sys::exit(code)
}

struct Mmio(u64);
impl Mmio {
    fn r32(&self, off: u64) -> u32 {
        // SAFETY: off fica dentro do BAR0 mapeado por mmio_map; acesso volátil alinhado.
        unsafe { core::ptr::read_volatile((self.0 + off) as *const u32) }
    }
    fn w32(&self, off: u64, v: u32) {
        // SAFETY: idem; escrita volátil alinhada.
        unsafe { core::ptr::write_volatile((self.0 + off) as *mut u32, v) }
    }
    fn r64(&self, off: u64) -> u64 {
        self.r32(off) as u64 | ((self.r32(off + 4) as u64) << 32)
    }
    fn w64(&self, off: u64, v: u64) {
        self.w32(off, v as u32);
        self.w32(off + 4, (v >> 32) as u32);
    }
}

/// Uma página de DMA (4 KiB) concedida.
struct Dma {
    virt: u64,
    phys: u64,
}

fn dma() -> Dma {
    let b = nexo_sys::dma_alloc(DEV).unwrap_or_else(|_| fail(80, "dma_alloc"));
    Dma {
        virt: b.virt,
        phys: b.phys,
    }
}

/// Fila de submissão + conclusão (par SQ/CQ do mesmo qid), modo síncrono.
struct QueuePair {
    sq: Dma,
    cq: Dma,
    qid: u16,
    tail: u16,
    head: u16,
    phase: u8,
    dstrd: u32,
    cid: u16,
}

impl QueuePair {
    fn sq_doorbell(&self) -> u64 {
        DOORBELLS + (2 * self.qid as u64) * (4u64 << self.dstrd)
    }
    fn cq_doorbell(&self) -> u64 {
        DOORBELLS + (2 * self.qid as u64 + 1) * (4u64 << self.dstrd)
    }

    /// Submete um comando de 64 B com o CID dado, sem esperar a conclusão.
    fn submit(&mut self, m: &Mmio, sqe: &[u32; 16], cid: u16) {
        let mut sqe = *sqe;
        sqe[0] |= (cid as u32) << 16;
        // SAFETY: página de DMA exclusiva da SQ; slot < QD.
        unsafe {
            core::ptr::copy_nonoverlapping(
                sqe.as_ptr() as *const u8,
                (self.sq.virt + self.tail as u64 * 64) as *mut u8,
                64,
            )
        };
        self.tail = (self.tail + 1) % QD as u16;
        m.w32(self.sq_doorbell(), self.tail as u32);
    }

    /// Colhe UMA conclusão, se houver: devolve `(cid, status)`.
    fn poll_completion(&mut self, m: &Mmio) -> Option<(u16, u16)> {
        let base = self.cq.virt + self.head as u64 * 16;
        // SAFETY: página de DMA exclusiva da CQ; leitura volátil da entrada corrente.
        let dw3 = unsafe { core::ptr::read_volatile((base + 12) as *const u32) };
        if (dw3 >> 16) & 1 != self.phase as u32 {
            return None;
        }
        let cid = (dw3 & 0xffff) as u16;
        let status = (dw3 >> 17) as u16;
        self.head = (self.head + 1) % QD as u16;
        if self.head == 0 {
            self.phase ^= 1;
        }
        m.w32(self.cq_doorbell(), self.head as u32);
        Some((cid, status))
    }

    /// Submete um comando de 64 B e espera a conclusão; devolve (status, dw0). Com `irq`,
    /// dorme em `irq_wait` entre verificações (MSI-X); sem, faz *polling* puro.
    fn command(&mut self, m: &Mmio, sqe: &[u32; 16], mut irq: Option<&mut IrqWait>) -> (u16, u32) {
        let mut sqe = *sqe;
        let cid = self.cid;
        self.cid = self.cid.wrapping_add(1);
        sqe[0] |= (cid as u32) << 16;
        // SAFETY: página de DMA exclusiva da SQ; slot < QD.
        unsafe {
            core::ptr::copy_nonoverlapping(
                sqe.as_ptr() as *const u8,
                (self.sq.virt + self.tail as u64 * 64) as *mut u8,
                64,
            )
        };
        self.tail = (self.tail + 1) % QD as u16;
        m.w32(self.sq_doorbell(), self.tail as u32);
        // espera a CQE com a fase corrente
        let mut spins = 0u64;
        loop {
            let base = self.cq.virt + self.head as u64 * 16;
            // SAFETY: página de DMA exclusiva da CQ; leitura volátil da entrada corrente.
            let dw3 = unsafe { core::ptr::read_volatile((base + 12) as *const u32) };
            if (dw3 >> 16) & 1 == self.phase as u32 {
                // SAFETY: mesma entrada; o dispositivo já a preencheu (fase confere).
                let dw0 = unsafe { core::ptr::read_volatile(base as *const u32) };
                let status = (dw3 >> 17) as u16;
                self.head = (self.head + 1) % QD as u16;
                if self.head == 0 {
                    self.phase ^= 1;
                }
                m.w32(self.cq_doorbell(), self.head as u32);
                return (status, dw0);
            }
            spins += 1;
            if spins > 200_000_000 {
                fail(81, "conclusao NVMe nao chegou");
            }
            if let Some(w) = irq.as_deref_mut() {
                // ja ha um doorbell batido: dorme ate a proxima interrupcao (coalescida)
                if spins > 64
                    && let Ok(c) = nexo_sys::irq_wait(DEV, w.vector, w.seen)
                {
                    w.seen = c;
                }
            } else if spins.is_multiple_of(1024) {
                nexo_sys::yield_now();
            }
        }
    }
}

/// Estado da espera por interrupção (vetor MSI-X 0 + contagem já vista).
struct IrqWait {
    vector: u32,
    seen: u64,
}

/// Programa a entrada 0 da tabela MSI-X da função (cap 0x11): endereço/dado da mensagem e
/// habilita MSI-X no controle. Devolve `false` se a função não tem a capability.
fn msix_setup(bdf: u16, bars: &nexo_sys::abi::PciInfo, addr: u64, data: u32) -> bool {
    let read = |off: u16| nexo_sys::pci_cfg_read(DEV, bdf, off).unwrap_or(0);
    let mut cap = (read(0x34) & 0xfc) as u16;
    let mut guard = 0;
    while cap != 0 && guard < 32 {
        let hdr = read(cap);
        if hdr & 0xff == 0x11 {
            let table = read(cap + 4);
            let (tbir, toff) = ((table & 7) as usize, (table & !7) as u64);
            let b = bars.bars[tbir];
            if b.size == 0 || b.flags & 1 != 0 {
                return false;
            }
            let Ok(base) = nexo_sys::mmio_map(DEV, b.base, b.size) else {
                return false;
            };
            let t = Mmio(base + toff);
            t.w32(0, addr as u32);
            t.w32(4, (addr >> 32) as u32);
            t.w32(8, data);
            t.w32(12, 0); // desmascara a entrada 0
            // MSI-X enable (bit 15 do message control), sem function mask (bit 14)
            let ctrl = read(cap);
            let mc = ((ctrl >> 16) as u16 | 0x8000) & !0x4000;
            let _ = nexo_sys::pci_cfg_write(DEV, bdf, cap, (ctrl & 0xffff) | ((mc as u32) << 16));
            return true;
        }
        cap = ((hdr >> 8) & 0xfc) as u16;
        guard += 1;
    }
    false
}

/// Monta uma SQE zerada com opcode, nsid e PRP1.
fn sqe(opc: u8, nsid: u32, prp1: u64) -> [u32; 16] {
    let mut e = [0u32; 16];
    e[0] = opc as u32;
    e[1] = nsid;
    e[6] = prp1 as u32;
    e[7] = (prp1 >> 32) as u32;
    e
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    // acha a função NVMe na concessão (classe 01h/08h, prog-if 02h)
    let mut devs = [PciInfo::default(); 8];
    let n = nexo_sys::pci_enum(DEV, &mut devs)
        .unwrap_or_else(|_| fail(82, "pci_enum"))
        .min(8);
    let info = *devs[..n]
        .iter()
        .find(|d| d.class == 0x01 && d.subclass == 0x08 && d.prog_if == 0x02)
        .unwrap_or_else(|| fail(83, "NVMe nao encontrado na concessao"));
    // memoria + bus master
    let cmd = nexo_sys::pci_cfg_read(DEV, info.bdf, 4).unwrap_or(0);
    let _ = nexo_sys::pci_cfg_write(DEV, info.bdf, 4, cmd | 0x6);
    let b0 = info.bars[0];
    if b0.size == 0 || b0.flags & 1 != 0 {
        fail(93, "BAR0 invalido");
    }
    let bar =
        nexo_sys::mmio_map(DEV, b0.base, b0.size).unwrap_or_else(|_| fail(84, "mmio_map BAR0"));
    let m = Mmio(bar);

    let cap = m.r64(R_CAP);
    let dstrd = ((cap >> 32) & 0xf) as u32;
    let timeout_ms = (((cap >> 24) & 0xff) + 1) * 500;

    // desabilita, programa as filas de admin e habilita
    m.w32(R_CC, 0);
    let mut spins = 0u64;
    while m.r32(R_CSTS) & 1 != 0 {
        spins += 1;
        if spins > 100_000_000 {
            fail(85, "controlador nao desabilitou");
        }
    }
    let asq = dma();
    let acq = dma();
    m.w32(R_AQA, (QD as u32 - 1) | ((QD as u32 - 1) << 16));
    m.w64(R_ASQ, asq.phys);
    m.w64(R_ACQ, acq.phys);
    // EN | IOSQES=6 (64 B) | IOCQES=4 (16 B); MPS=0 (4 KiB), CSS=0 (NVM)
    m.w32(R_CC, 1 | (6 << 16) | (4 << 20));
    let mut spins = 0u64;
    while m.r32(R_CSTS) & 1 == 0 {
        spins += 1;
        if spins > 400_000_000 {
            fail(86, "controlador nao habilitou");
        }
    }
    let mut admin = QueuePair {
        sq: asq,
        cq: acq,
        qid: 0,
        tail: 0,
        head: 0,
        phase: 1,
        dstrd,
        cid: 0,
    };

    // Identify Controller (CNS=1): serial em [4..24]
    let idbuf = dma();
    let mut e = sqe(0x06, 0, idbuf.phys);
    e[10] = 1; // CNS
    let (st, _) = admin.command(&m, &e, None);
    if st != 0 {
        fail(87, "identify controller");
    }
    let mut serial = [0u8; 20];
    // SAFETY: página de DMA exclusiva preenchida pelo identify.
    unsafe {
        core::ptr::copy_nonoverlapping((idbuf.virt + 4) as *const u8, serial.as_mut_ptr(), 20)
    };

    // Identify Namespace 1 (CNS=0): NSZE em [0..8]; tamanho de bloco do LBAF ativo
    let mut e = sqe(0x06, 1, idbuf.phys);
    e[10] = 0;
    let (st, _) = admin.command(&m, &e, None);
    if st != 0 {
        fail(88, "identify namespace");
    }
    // SAFETY: página de DMA exclusiva preenchida pelo identify namespace.
    let (nsze, flbas, lbaf0) = unsafe {
        (
            core::ptr::read_volatile(idbuf.virt as *const u64),
            core::ptr::read_volatile((idbuf.virt + 26) as *const u8),
            idbuf.virt + 128,
        )
    };
    let idx = (flbas & 0xf) as u64;
    // SAFETY: tabela LBAF dentro da mesma página do identify.
    let lbaf = unsafe { core::ptr::read_volatile((lbaf0 + idx * 4) as *const u32) };
    let lbads = (lbaf >> 16) & 0xff;
    if lbads != 9 {
        fail(89, "so blocos de 512 B no MVP");
    }

    // MSI-X (vetor 0) para a fila de E/S; sem a capability, cai para polling
    let mut irq = nexo_sys::irq_alloc(DEV)
        .ok()
        .filter(|i| msix_setup(info.bdf, &info, i.msi_address, i.msi_data))
        .map(|i| IrqWait {
            vector: i.vector,
            seen: 0,
        });

    // par de filas de E/S (qid 1): primeiro a CQ, depois a SQ apontando para ela
    let iosq = dma();
    let iocq = dma();
    let mut e = sqe(0x05, 0, iocq.phys); // Create IO CQ
    e[10] = 1 | ((QD as u32 - 1) << 16);
    e[11] = if irq.is_some() { 1 | 2 } else { 1 }; // PC (+IEN com IV=0 sob MSI-X)
    let (st, _) = admin.command(&m, &e, None);
    if st != 0 {
        fail(90, "create io cq");
    }
    let mut e = sqe(0x01, 0, iosq.phys); // Create IO SQ
    e[10] = 1 | ((QD as u32 - 1) << 16);
    e[11] = 1 | (1 << 16); // PC | CQID=1
    let (st, _) = admin.command(&m, &e, None);
    if st != 0 {
        fail(91, "create io sq");
    }
    let mut io = QueuePair {
        sq: iosq,
        cq: iocq,
        qid: 1,
        tail: 0,
        head: 0,
        phase: 1,
        dstrd,
        cid: 0,
    };
    // Fila assincrona no espirito do blockdev: ate SLOTS pedidos em voo, cada um com a sua
    // pagina de dados (PRP1); as respostas saem na ORDEM DE CHEGADA dos pedidos (Ready entra
    // na mesma fila para nao furar a ordem).
    const SLOTS: usize = 4;
    let slot_data: [Dma; SLOTS] = [dma(), dma(), dma(), dma()];
    log!(
        "nvmedev: nvme bdf {:#06x} pronto ({} setores de 512 B, timeout {} ms, serial {}, {}, ate {} em voo)",
        info.bdf,
        nsze,
        timeout_ms,
        core::str::from_utf8(&serial).unwrap_or("?").trim(),
        if irq.is_some() { "MSI-X" } else { "polling" },
        SLOTS
    );

    enum Pending {
        Ready {
            len: usize,
        },
        Io {
            slot: usize,
            write: bool,
            bytes: usize,
        },
    }
    let mut buf = [0u8; 4096];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut served = 0u64;
    let mut max_in_flight = 0usize;
    let mut pending: [Option<Pending>; SLOTS] = [const { None }; SLOTS];
    let mut order: [usize; SLOTS] = [0; SLOTS];
    let mut order_len = 0usize;
    let mut ready_buf = [[0u8; 4096]; SLOTS];
    let mut slot_free = [true; SLOTS];
    let mut done: [Option<u16>; SLOTS] = [None; SLOTS];
    let mut closing = false;
    loop {
        let mut worked = false;
        // 1. colhe conclusoes (CID = indice do slot)
        while let Some((cid, st)) = io.poll_completion(&m) {
            if (cid as usize) < SLOTS {
                done[cid as usize] = Some(st);
            }
            worked = true;
        }
        // 2. entrega respostas prontas na ordem de chegada
        while order_len > 0 {
            let idx = order[0];
            let (len, ready, from_ready) = match &pending[idx] {
                Some(Pending::Ready { len }) => (*len, true, true),
                Some(Pending::Io { slot, write, bytes }) => match done[*slot] {
                    Some(st) => {
                        let len = if st != 0 {
                            let method = if *write {
                                block::WriteRequest::METHOD_ID
                            } else {
                                block::ReadRequest::METHOD_ID
                            };
                            block::encode_error(method, 0x10 | (st & 0xf) as u32, &mut reply)
                                .unwrap_or(0)
                        } else if *write {
                            WriteResponse {}.encode_msg(&mut reply).unwrap_or(0)
                        } else {
                            let mut r = ReadResponse {
                                data: [0; 3584],
                                data_len: *bytes as u32,
                            };
                            // SAFETY: pagina de DMA exclusiva do slot; bytes <= 3584.
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    slot_data[*slot].virt as *const u8,
                                    r.data.as_mut_ptr(),
                                    *bytes,
                                )
                            };
                            r.encode_msg(&mut reply).unwrap_or(0)
                        };
                        done[*slot] = None;
                        slot_free[*slot] = true;
                        (len, true, false)
                    }
                    None => (0, false, false),
                },
                None => (0, false, false),
            };
            if !ready {
                break;
            }
            let out: &[u8] = if from_ready {
                &ready_buf[idx][..len]
            } else {
                &reply[..len]
            };
            if !closing {
                let _ = nexo_sys::channel_send(CHAN, out, &[]);
            }
            pending[idx] = None;
            order.copy_within(1..order_len, 0);
            order_len -= 1;
            worked = true;
        }
        if closing && order_len == 0 {
            log!(
                "nvmedev: canal fechado apos {} pedidos (max {} em voo)",
                served,
                max_in_flight
            );
            nexo_sys::exit(0)
        }
        // 3. recebe pedidos: bloqueia so quando nao ha nada em voo nem pendente
        let has_room = !closing && order_len < SLOTS;
        let r = if order_len == 0 && !closing {
            nexo_sys::channel_recv(CHAN, &mut buf, &mut hs).map(Some)
        } else if has_room {
            match nexo_sys::channel_try_recv(CHAN, &mut buf, &mut hs) {
                Ok(v) => Ok(Some(v)),
                Err(Status::WouldBlock) => Ok(None),
                Err(e) => Err(e),
            }
        } else {
            Ok(None)
        };
        let msg = match r {
            Ok(v) => v,
            Err(Status::PeerClosed) => {
                closing = true;
                continue;
            }
            Err(_) => fail(92, "recv"),
        };
        if let Some((n, _)) = msg {
            served += 1;
            worked = true;
            let free_idx = (0..SLOTS)
                .find(|&i| pending[i].is_none() && !order[..order_len].contains(&i))
                .unwrap_or(0);
            let push = |p: Pending,
                        pending: &mut [Option<Pending>; SLOTS],
                        order: &mut [usize; SLOTS],
                        order_len: &mut usize| {
                pending[free_idx] = Some(p);
                order[*order_len] = free_idx;
                *order_len += 1;
            };
            match block::decode_request(&buf[..n]) {
                Ok(Request::Capacity(_)) => {
                    let len = CapacityResponse { sectors: nsze }
                        .encode_msg(&mut ready_buf[free_idx])
                        .unwrap_or(0);
                    push(
                        Pending::Ready { len },
                        &mut pending,
                        &mut order,
                        &mut order_len,
                    );
                }
                Ok(Request::Identity(_)) => {
                    let mut r = IdentityResponse {
                        read_only: 0,
                        serial: [0; 20],
                        serial_len: 20,
                    };
                    r.serial.copy_from_slice(&serial);
                    let len = r.encode_msg(&mut ready_buf[free_idx]).unwrap_or(0);
                    push(
                        Pending::Ready { len },
                        &mut pending,
                        &mut order,
                        &mut order_len,
                    );
                }
                Ok(Request::Read(rq)) => {
                    let count = rq.count as usize;
                    if count == 0 || count > MAX_SECTORS || rq.sector + rq.count as u64 > nsze {
                        let len = block::encode_error(
                            block::ReadRequest::METHOD_ID,
                            2,
                            &mut ready_buf[free_idx],
                        )
                        .unwrap_or(0);
                        push(
                            Pending::Ready { len },
                            &mut pending,
                            &mut order,
                            &mut order_len,
                        );
                    } else {
                        let slot = (0..SLOTS)
                            .find(|&i| slot_free[i])
                            .unwrap_or_else(|| fail(93, "sem slot livre"));
                        slot_free[slot] = false;
                        let mut e = sqe(0x02, 1, slot_data[slot].phys);
                        e[10] = rq.sector as u32;
                        e[11] = (rq.sector >> 32) as u32;
                        e[12] = (count - 1) as u32;
                        io.submit(&m, &e, slot as u16);
                        push(
                            Pending::Io {
                                slot,
                                write: false,
                                bytes: count * 512,
                            },
                            &mut pending,
                            &mut order,
                            &mut order_len,
                        );
                    }
                }
                Ok(Request::Write(rq)) => {
                    let count = rq.count as usize;
                    let bytes = rq.data().len();
                    if count == 0
                        || count > MAX_SECTORS
                        || bytes != count * 512
                        || rq.sector + rq.count as u64 > nsze
                    {
                        let len = block::encode_error(
                            block::WriteRequest::METHOD_ID,
                            2,
                            &mut ready_buf[free_idx],
                        )
                        .unwrap_or(0);
                        push(
                            Pending::Ready { len },
                            &mut pending,
                            &mut order,
                            &mut order_len,
                        );
                    } else {
                        let slot = (0..SLOTS)
                            .find(|&i| slot_free[i])
                            .unwrap_or_else(|| fail(94, "sem slot livre"));
                        slot_free[slot] = false;
                        // SAFETY: pagina de DMA exclusiva do slot; bytes <= 3584.
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                rq.data().as_ptr(),
                                slot_data[slot].virt as *mut u8,
                                bytes,
                            )
                        };
                        let mut e = sqe(0x01, 1, slot_data[slot].phys);
                        e[10] = rq.sector as u32;
                        e[11] = (rq.sector >> 32) as u32;
                        e[12] = (count - 1) as u32;
                        io.submit(&m, &e, slot as u16);
                        push(
                            Pending::Io {
                                slot,
                                write: true,
                                bytes,
                            },
                            &mut pending,
                            &mut order,
                            &mut order_len,
                        );
                    }
                }
                Err(_) => {
                    let len = block::encode_error(0, 1, &mut ready_buf[free_idx]).unwrap_or(0);
                    push(
                        Pending::Ready { len },
                        &mut pending,
                        &mut order,
                        &mut order_len,
                    );
                }
            }
            let in_flight = order_len;
            if in_flight > max_in_flight {
                max_in_flight = in_flight;
            }
        }
        if !worked && order_len > 0 {
            // ha E/S em voo e nada novo: dorme ate a proxima interrupcao (ou cede a CPU)
            if let Some(w) = irq.as_mut() {
                if let Ok(c) = nexo_sys::irq_wait(DEV, w.vector, w.seen) {
                    w.seen = c;
                }
            } else {
                nexo_sys::yield_now();
            }
        }
    }
}
