//! `ahcidev` — driver SATA/AHCI em modo usuário (Plano §Fase 7: "implementar NVMe ou AHCI
//! conforme o computador de referência"; QEMU `ich9-ahci` como referência). Mesmo modelo dos
//! demais drivers: concessão de UMA função PCI, ABAR (BAR5) por `mmio_map`, DMA por páginas
//! concedidas — servindo o MESMO `nexo.block` v0, então cliente e pilha não mudam uma linha.
//! MVP síncrono: uma porta, um slot de comando, READ/WRITE DMA EXT (LBA48), *polling*.
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

// Registradores globais do HBA (offsets no ABAR).
const R_CAP: u64 = 0x00;
const R_GHC: u64 = 0x04;
const R_PI: u64 = 0x0c;
const PORT_BASE: u64 = 0x100;
const PORT_SIZE: u64 = 0x80;
// Registradores por porta.
const P_CLB: u64 = 0x00;
const P_CLBU: u64 = 0x04;
const P_FB: u64 = 0x08;
const P_FBU: u64 = 0x0c;
const P_IS: u64 = 0x10;
const P_CMD: u64 = 0x18;
const P_TFD: u64 = 0x20;
const P_SSTS: u64 = 0x28;
const P_SCTL: u64 = 0x2c;
const P_SERR: u64 = 0x30;
const P_CI: u64 = 0x38;

fn fail(code: i64, what: &str) -> ! {
    log!("ahcidev: falha: {}", what);
    nexo_sys::exit(code)
}

struct Mmio(u64);
impl Mmio {
    fn r32(&self, off: u64) -> u32 {
        // SAFETY: off dentro do ABAR mapeado por mmio_map; acesso volátil alinhado.
        unsafe { core::ptr::read_volatile((self.0 + off) as *const u32) }
    }
    fn w32(&self, off: u64, v: u32) {
        // SAFETY: idem; escrita volátil alinhada.
        unsafe { core::ptr::write_volatile((self.0 + off) as *mut u32, v) }
    }
}

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

/// Espera `mask` ficar zerado em `off` (com teto de giros).
fn wait_clear(m: &Mmio, off: u64, mask: u32, code: i64) {
    let mut spins = 0u64;
    while m.r32(off) & mask != 0 {
        spins += 1;
        if spins > 200_000_000 {
            fail(code, "tempo esgotado em registrador");
        }
        if spins.is_multiple_of(1024) {
            nexo_sys::yield_now();
        }
    }
}

struct PortIo {
    m: Mmio,   // base do ABAR
    p: u64,    // offset da porta
    cl: Dma,   // command list (1 KiB usado)
    ct: Dma,   // command table (FIS 64B + PRDT)
    fis: Dma,  // FIS receive (256 B)
    data: Dma, // página de dados
}

impl PortIo {
    fn preg(&self, off: u64) -> u64 {
        self.p + off
    }

    /// Emite um comando ATA no slot 0 e espera concluir; devolve `false` em erro do device.
    fn ata(&self, cmd: u8, lba: u64, count: u16, write: bool, bytes: u32) -> bool {
        // FIS H2D no início da command table
        // SAFETY: página de DMA exclusiva; layout do FIS de 20 bytes + zeros.
        unsafe {
            core::ptr::write_bytes(self.ct.virt as *mut u8, 0, 128);
            let f = self.ct.virt as *mut u8;
            f.write_volatile(0x27); // H2D
            f.add(1).write_volatile(0x80); // C=1
            f.add(2).write_volatile(cmd);
            f.add(4).write_volatile(lba as u8);
            f.add(5).write_volatile((lba >> 8) as u8);
            f.add(6).write_volatile((lba >> 16) as u8);
            f.add(7).write_volatile(0x40); // device: LBA
            f.add(8).write_volatile((lba >> 24) as u8);
            f.add(9).write_volatile((lba >> 32) as u8);
            f.add(10).write_volatile((lba >> 40) as u8);
            f.add(12).write_volatile(count as u8);
            f.add(13).write_volatile((count >> 8) as u8);
            // PRDT[0] em ct+0x80: DBA + DBC (len-1)
            let prdt = (self.ct.virt + 0x80) as *mut u32;
            prdt.write_volatile(self.data.phys as u32);
            prdt.add(1).write_volatile((self.data.phys >> 32) as u32);
            prdt.add(3).write_volatile(bytes - 1);
            // cabeçalho do slot 0 na command list: CFL=5 dwords, W, PRDTL=1, CTBA
            let hdr = self.cl.virt as *mut u32;
            hdr.write_volatile(5 | ((write as u32) << 6) | (1 << 16));
            hdr.add(1).write_volatile(0); // PRDBC
            hdr.add(2).write_volatile(self.ct.phys as u32);
            hdr.add(3).write_volatile((self.ct.phys >> 32) as u32);
        }
        self.m.w32(self.preg(P_IS), 0xffff_ffff);
        self.m.w32(self.preg(P_SERR), 0xffff_ffff);
        self.m.w32(self.preg(P_CI), 1);
        let mut spins = 0u64;
        while self.m.r32(self.preg(P_CI)) & 1 != 0 {
            spins += 1;
            if spins > 200_000_000 {
                fail(81, "comando ATA nao concluiu");
            }
            if spins.is_multiple_of(1024) {
                nexo_sys::yield_now();
            }
        }
        self.m.r32(self.preg(P_TFD)) & 0x01 == 0 // bit ERR do task file
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    // acha a função AHCI na concessão (classe 01h/06h, prog-if 01h)
    let mut devs = [PciInfo::default(); 8];
    let n = nexo_sys::pci_enum(DEV, &mut devs)
        .unwrap_or_else(|_| fail(82, "pci_enum"))
        .min(8);
    let info = *devs[..n]
        .iter()
        .find(|d| d.class == 0x01 && d.subclass == 0x06 && d.prog_if == 0x01)
        .unwrap_or_else(|| fail(83, "AHCI nao encontrado na concessao"));
    let cmd = nexo_sys::pci_cfg_read(DEV, info.bdf, 4).unwrap_or(0);
    let _ = nexo_sys::pci_cfg_write(DEV, info.bdf, 4, cmd | 0x6);
    let b5 = info.bars[5];
    if b5.size == 0 || b5.flags & 1 != 0 {
        fail(84, "ABAR (BAR5) invalido");
    }
    let abar = nexo_sys::mmio_map(DEV, b5.base, b5.size).unwrap_or_else(|_| fail(85, "mmio_map"));
    let m = Mmio(abar);

    // Habilita o modo AHCI e escolhe a primeira porta com dispositivo SATA presente.
    // O bring-up da porta é NOSSO (spin-up + COMRESET): não se pode depender do firmware
    // ter conectado o controlador — com `bootindex`, o OVMF só inicializa o dispositivo de
    // boot, e uma porta nunca tocada fica sem detecção (e sem assinatura, que só aparece
    // após o primeiro FIS D2H; por isso a validação do dispositivo fica com o IDENTIFY).
    m.w32(R_GHC, m.r32(R_GHC) | (1 << 31)); // AE
    let pi = m.r32(R_PI);
    let cap = m.r32(R_CAP);
    let mut port = None;
    for p in 0..32u64 {
        if pi & (1 << p) == 0 {
            continue;
        }
        let base = PORT_BASE + p * PORT_SIZE;
        // para a porta e liga alimentação/spin-up antes do reset do enlace
        m.w32(base + P_CMD, m.r32(base + P_CMD) & !(1 | (1 << 4)));
        m.w32(base + P_CMD, m.r32(base + P_CMD) | (1 << 1) | (1 << 2)); // SUD | POD
        // COMRESET: DET=1 por >= 1 ms, depois solta e espera o enlace (DET=3)
        m.w32(base + P_SCTL, (m.r32(base + P_SCTL) & !0xf) | 1);
        nexo_sys::sleep_ns(2_000_000);
        m.w32(base + P_SCTL, m.r32(base + P_SCTL) & !0xf);
        let mut det = 0;
        for _ in 0..100 {
            det = m.r32(base + P_SSTS) & 0xf;
            if det == 3 {
                break;
            }
            nexo_sys::sleep_ns(1_000_000);
        }
        m.w32(base + P_SERR, 0xffff_ffff);
        if det == 3 {
            port = Some(base);
            break;
        }
    }
    let p = port.unwrap_or_else(|| fail(86, "nenhuma porta com disco SATA"));

    // para a porta (ST/FRE), programa CLB/FB e religa
    let pm = Mmio(abar);
    let cmdreg = pm.r32(p + P_CMD);
    pm.w32(p + P_CMD, cmdreg & !(1 | (1 << 4)));
    wait_clear(&pm, p + P_CMD, (1 << 15) | (1 << 14), 87); // CR e FR
    let cl = dma();
    let ct = dma();
    let fis = dma();
    let data = dma();
    pm.w32(p + P_CLB, cl.phys as u32);
    pm.w32(p + P_CLBU, (cl.phys >> 32) as u32);
    pm.w32(p + P_FB, fis.phys as u32);
    pm.w32(p + P_FBU, (fis.phys >> 32) as u32);
    pm.w32(p + P_CMD, pm.r32(p + P_CMD) | (1 << 4)); // FRE
    pm.w32(p + P_CMD, pm.r32(p + P_CMD) | 1); // ST
    let io = PortIo {
        m: Mmio(abar),
        p,
        cl,
        ct,
        fis,
        data,
    };
    let _ = &io.fis; // FIS receive fica com o dispositivo

    // IDENTIFY DEVICE (0xEC): capacidade LBA48 (words 100..103) e serial (words 10..19)
    if !io.ata(0xec, 0, 0, false, 512) {
        fail(88, "identify");
    }
    // SAFETY: página de dados preenchida pelo IDENTIFY.
    let ident = unsafe { core::slice::from_raw_parts(io.data.virt as *const u8, 512) };
    let sectors = u64::from_le_bytes(ident[200..208].try_into().unwrap());
    let mut serial = [0u8; 20];
    for i in 0..10 {
        // strings ATA vêm com os bytes trocados dentro de cada word
        serial[i * 2] = ident[20 + i * 2 + 1];
        serial[i * 2 + 1] = ident[20 + i * 2];
    }
    let serial_txt = core::str::from_utf8(&serial).unwrap_or("?").trim();
    log!(
        "ahcidev: AHCI bdf {:#06x} pronto (porta {}, {} setores de 512 B, cap {:#x}, serial '{}', polling)",
        info.bdf,
        (p - PORT_BASE) / PORT_SIZE,
        sectors,
        cap,
        serial_txt
    );

    // serve nexo.block v0 (síncrono)
    let mut buf = [0u8; 4096];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut served = 0u64;
    loop {
        let n = match nexo_sys::channel_recv(CHAN, &mut buf, &mut hs) {
            Ok((n, _)) => n,
            Err(Status::PeerClosed) => {
                log!("ahcidev: canal fechado apos {} pedidos", served);
                nexo_sys::exit(0)
            }
            Err(_) => fail(89, "recv"),
        };
        served += 1;
        let out_len = match block::decode_request(&buf[..n]) {
            Ok(Request::Capacity(_)) => CapacityResponse { sectors }
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
                if count == 0 || count > MAX_SECTORS || rq.sector + rq.count as u64 > sectors {
                    block::encode_error(block::ReadRequest::METHOD_ID, 2, &mut reply).unwrap_or(0)
                } else if !io.ata(0x25, rq.sector, count as u16, false, (count * 512) as u32) {
                    block::encode_error(block::ReadRequest::METHOD_ID, 0x10, &mut reply)
                        .unwrap_or(0)
                } else {
                    let bytes = count * 512;
                    let mut r = ReadResponse {
                        data: [0; 3584],
                        data_len: bytes as u32,
                    };
                    // SAFETY: página de DMA exclusiva; bytes <= 3584.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            io.data.virt as *const u8,
                            r.data.as_mut_ptr(),
                            bytes,
                        )
                    };
                    r.encode_msg(&mut reply).unwrap_or(0)
                }
            }
            Ok(Request::Write(rq)) => {
                let count = rq.count as usize;
                let bytes = rq.data().len();
                if count == 0
                    || count > MAX_SECTORS
                    || bytes != count * 512
                    || rq.sector + rq.count as u64 > sectors
                {
                    block::encode_error(block::WriteRequest::METHOD_ID, 2, &mut reply).unwrap_or(0)
                } else {
                    // SAFETY: página de DMA exclusiva; bytes <= 3584.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            rq.data().as_ptr(),
                            io.data.virt as *mut u8,
                            bytes,
                        )
                    };
                    if io.ata(0x35, rq.sector, count as u16, true, bytes as u32) {
                        WriteResponse {}.encode_msg(&mut reply).unwrap_or(0)
                    } else {
                        block::encode_error(block::WriteRequest::METHOD_ID, 0x10, &mut reply)
                            .unwrap_or(0)
                    }
                }
            }
            Err(_) => block::encode_error(0, 1, &mut reply).unwrap_or(0),
        };
        let _ = nexo_sys::channel_send(CHAN, &reply[..out_len], &[]);
    }
}
