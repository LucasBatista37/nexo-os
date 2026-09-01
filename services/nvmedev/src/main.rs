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
    let data = dma(); // pagina unica de dados (PRP1) — cobre os 3584 B do nexo.block
    log!(
        "nvmedev: nvme bdf {:#06x} pronto ({} setores de 512 B, timeout {} ms, serial {}, {})",
        info.bdf,
        nsze,
        timeout_ms,
        core::str::from_utf8(&serial).unwrap_or("?").trim(),
        if irq.is_some() { "MSI-X" } else { "polling" }
    );

    // serve nexo.block v0 (sincrono: um pedido em voo)
    let mut buf = [0u8; 4096];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut served = 0u64;
    loop {
        let n = match nexo_sys::channel_recv(CHAN, &mut buf, &mut hs) {
            Ok((n, _)) => n,
            Err(Status::PeerClosed) => {
                log!("nvmedev: canal fechado apos {} pedidos", served);
                nexo_sys::exit(0)
            }
            Err(_) => fail(92, "recv"),
        };
        served += 1;
        let out_len = match block::decode_request(&buf[..n]) {
            Ok(Request::Capacity(_)) => CapacityResponse { sectors: nsze }
                .encode_msg(&mut reply)
                .unwrap_or(0),
            Ok(Request::Identity(_)) => {
                let mut r = IdentityResponse {
                    read_only: 0,
                    serial: [0; 20],
                    serial_len: 20,
                };
                r.serial.copy_from_slice(&serial);
                r.encode_msg(&mut reply).unwrap_or(0)
            }
            Ok(Request::Read(rq)) => {
                let count = rq.count as usize;
                if count == 0 || count > MAX_SECTORS || rq.sector + rq.count as u64 > nsze {
                    block::encode_error(block::ReadRequest::METHOD_ID, 2, &mut reply).unwrap_or(0)
                } else {
                    let mut e = sqe(0x02, 1, data.phys);
                    e[10] = rq.sector as u32;
                    e[11] = (rq.sector >> 32) as u32;
                    e[12] = (count - 1) as u32;
                    let (st, _) = io.command(&m, &e, irq.as_mut());
                    if st != 0 {
                        block::encode_error(
                            block::ReadRequest::METHOD_ID,
                            0x10 | (st & 0xf) as u32,
                            &mut reply,
                        )
                        .unwrap_or(0)
                    } else {
                        let bytes = count * 512;
                        let mut r = ReadResponse {
                            data: [0; 3584],
                            data_len: bytes as u32,
                        };
                        // SAFETY: pagina de DMA exclusiva; bytes <= 3584.
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                data.virt as *const u8,
                                r.data.as_mut_ptr(),
                                bytes,
                            )
                        };
                        r.encode_msg(&mut reply).unwrap_or(0)
                    }
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
                    block::encode_error(block::WriteRequest::METHOD_ID, 2, &mut reply).unwrap_or(0)
                } else {
                    // SAFETY: pagina de DMA exclusiva; bytes <= 3584.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            rq.data().as_ptr(),
                            data.virt as *mut u8,
                            bytes,
                        )
                    };
                    let mut e = sqe(0x01, 1, data.phys);
                    e[10] = rq.sector as u32;
                    e[11] = (rq.sector >> 32) as u32;
                    e[12] = (count - 1) as u32;
                    let (st, _) = io.command(&m, &e, irq.as_mut());
                    if st != 0 {
                        block::encode_error(
                            block::WriteRequest::METHOD_ID,
                            0x10 | (st & 0xf) as u32,
                            &mut reply,
                        )
                        .unwrap_or(0)
                    } else {
                        WriteResponse {}.encode_msg(&mut reply).unwrap_or(0)
                    }
                }
            }
            Err(_) => block::encode_error(0, 1, &mut reply).unwrap_or(0),
        };
        let _ = nexo_sys::channel_send(CHAN, &reply[..out_len], &[]);
    }
}
