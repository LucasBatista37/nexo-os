//! Auto-testes executados no boot e verificados pelo CI via serial.
//!
//! Formato: `[TEST] nome ... ok|FAIL: motivo` e, ao final,
//! `[RESULT] PASS n/n` ou `[RESULT] FAIL k/n`.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::gdt::{KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR};
use nexo_arch_x86_64::paging::PageFlags;
use nexo_boot_abi::{KERNEL_STACK_BASE, KERNEL_STACK_TOP};
use nexo_mm::{PAGE_SIZE, PhysAddr, VirtAddr};
use nexo_sync::SpinLock;

use crate::mm::heap::HeapStatsExt;
use crate::mm::{heap, phys, virt};
use crate::x86::traps::{ProbeKind, probe};

type TestResult = Result<(), String>;
type TestFn = fn() -> TestResult;

macro_rules! check {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond {
            return Err(alloc::format!($($arg)*));
        }
    };
}

const TESTS: &[(&str, TestFn)] = &[
    ("boot_info", test_boot_info),
    ("segments", test_segments),
    ("breakpoint", test_breakpoint),
    ("frame_alloc", test_frame_alloc),
    ("paging_map_unmap", test_paging),
    ("section_permissions", test_section_permissions),
    ("page_fault_recovery", test_page_fault_recovery),
    ("guard_page", test_guard_page),
    ("write_protect", test_write_protect),
    ("nx", test_nx),
    ("heap", test_heap),
    ("heap_grow", test_heap_grow),
    ("timer", test_timer),
    ("coop_tasks", test_coop_tasks),
    ("symbols", test_symbols),
];

/// Executa todos os testes. Devolve `true` se todos passaram.
pub fn run() -> bool {
    kprint!("[SELFTEST] iniciando {} testes\n", TESTS.len());
    let mut passed = 0;
    for (name, f) in TESTS {
        kprint!("[TEST] {name} ... ");
        match f() {
            Ok(()) => {
                passed += 1;
                kprint!("ok\n");
            }
            Err(e) => kprint!("FAIL: {e}\n"),
        }
    }
    report_memory();
    if passed == TESTS.len() {
        kprint!("[RESULT] PASS {passed}/{}\n", TESTS.len());
        true
    } else {
        kprint!("[RESULT] FAIL {}/{}\n", TESTS.len() - passed, TESTS.len());
        false
    }
}

fn report_memory() {
    let f = phys::stats();
    let h = heap::stats();
    let s = phys::summary();
    kprint!(
        "[MEMORY] ram_utilizavel_kib={} quadros_total={} quadros_livres={} quadros_usados={} alocacoes={} liberacoes={} falhas={}\n",
        s.usable_bytes >> 10,
        f.total_usable,
        f.free,
        f.used(),
        f.allocations,
        f.frees,
        f.failures
    );
    kprint!(
        "[HEAP] mapeado_kib={} total_kib={} em_uso={} pico={} alocacoes={} liberacoes={} tentativas_falhas={} oom={} blocos_livres={}\n",
        heap::mapped_bytes() >> 10,
        h.total_bytes >> 10,
        h.used_bytes,
        h.peak_bytes,
        h.allocations,
        h.frees,
        h.failures,
        heap::oom_count(),
        h.free_blocks
    );
    kprint!("[MAP] regioes_normalizadas={}\n", phys::regions().len());
    kprint!(
        "[TIME] ticks={} uptime_ms={} excecoes={} trocas_de_contexto={}\n",
        crate::time::ticks(),
        crate::time::uptime_ms(),
        crate::x86::traps::exception_count(),
        crate::task::switches()
    );
}

fn test_boot_info() -> TestResult {
    let bi = crate::boot::info();
    bi.validate().map_err(|e| alloc::format!("{e}"))?;
    check!(bi.memory_map_len > 0, "mapa vazio");
    check!(
        bi.page_table_root == cpu::read_cr3() & !0xfff,
        "CR3 difere do BootInfo"
    );
    Ok(())
}

fn test_segments() -> TestResult {
    let cs = cpu::read_cs();
    check!(cs == KERNEL_CODE_SELECTOR, "cs={cs:#x}");
    check!(KERNEL_DATA_SELECTOR == 0x10, "seletor de dados inesperado");
    check!(
        cpu::interrupts_enabled(),
        "interrupcoes deveriam estar habilitadas"
    );
    check!(cpu::nx_enabled(), "EFER.NXE desligado");
    check!(cpu::read_cr0() & cpu::CR0_WP != 0, "CR0.WP desligado");
    Ok(())
}

fn test_breakpoint() -> TestResult {
    let before = crate::x86::traps::breakpoint_count();
    // SAFETY: #BP é tratado pelo handler, que apenas conta e retorna.
    unsafe { core::arch::asm!("int3", options(nomem, nostack)) };
    let after = crate::x86::traps::breakpoint_count();
    check!(after == before + 1, "contador de #BP {before} -> {after}");
    Ok(())
}

fn test_frame_alloc() -> TestResult {
    let before = phys::stats();
    let mut frames = Vec::new();
    for _ in 0..1000 {
        let f = phys::allocate_frame().ok_or("sem quadros")?;
        check!(f.is_aligned(PAGE_SIZE), "quadro desalinhado {f}");
        check!(f.as_u64() >= 0x10_0000, "quadro abaixo de 1 MiB: {f}");
        frames.push(f);
    }
    let mut sorted = frames.clone();
    sorted.sort();
    sorted.dedup();
    check!(sorted.len() == 1000, "quadros duplicados");
    // Escreve e lê cada quadro pelo physmap.
    for (i, f) in frames.iter().enumerate() {
        let p = virt::phys_to_virt(*f).as_mut_ptr::<u64>();
        // SAFETY: quadro alocado, dentro do physmap.
        unsafe {
            p.write_volatile(0xC0FFEE00 + i as u64);
            check!(
                p.read_volatile() == 0xC0FFEE00 + i as u64,
                "quadro {f} nao retem dados"
            );
        }
    }
    for f in &frames {
        phys::free_frame(*f).map_err(|e| alloc::format!("{e:?}"))?;
    }
    check!(
        phys::free_frame(frames[0]).is_err(),
        "double free nao detectado"
    );
    check!(
        phys::free_frame(PhysAddr::new(0)).is_err(),
        "liberacao de quadro reservado aceita"
    );
    let after = phys::stats();
    check!(
        after.free == before.free,
        "vazamento: {} -> {}",
        before.free,
        after.free
    );
    Ok(())
}

const TEST_VIRT: u64 = 0xffff_ffff_d000_0000;

fn test_paging() -> TestResult {
    let v = VirtAddr::new(TEST_VIRT);
    check!(virt::translate(v).is_none(), "endereco de teste ja mapeado");
    let frame = virt::alloc_and_map(v, PageFlags::KERNEL_RW).map_err(|e| alloc::format!("{e}"))?;
    let t = virt::translate(v.add(0x10)).ok_or("translate falhou")?;
    check!(
        t.phys == frame.add(0x10),
        "traducao errada: {} != {}",
        t.phys,
        frame.add(0x10)
    );
    check!(
        t.flags
            .contains(PageFlags::WRITABLE | PageFlags::NO_EXECUTE),
        "flags {:?}",
        t.flags
    );
    // Escreve pelo endereço mapeado, lê pelo physmap.
    // SAFETY: página recém-mapeada RW; mesmo quadro visto pelo physmap.
    unsafe {
        (v.as_mut_ptr::<u64>()).write_volatile(0x1234_5678_9abc_def0);
        let via_physmap = virt::phys_to_virt(frame).as_ptr::<u64>().read_volatile();
        check!(
            via_physmap == 0x1234_5678_9abc_def0,
            "physmap nao ve a escrita"
        );
    }
    check!(
        virt::map_page(v, frame, PageFlags::KERNEL_RW).is_err(),
        "remapeamento aceito"
    );
    virt::update_flags(v, PageFlags::KERNEL_RO).map_err(|e| alloc::format!("{e}"))?;
    let t = virt::translate(v).ok_or("translate falhou")?;
    check!(
        !t.flags.contains(PageFlags::WRITABLE),
        "flags nao atualizadas"
    );
    virt::unmap_and_free(v).map_err(|e| alloc::format!("{e}"))?;
    check!(virt::translate(v).is_none(), "ainda mapeado apos unmap");
    check!(virt::unmap_page(v).is_err(), "unmap duplo aceito");
    Ok(())
}

fn test_section_permissions() -> TestResult {
    let expect = |name: &str, w: bool, x: bool| -> TestResult {
        let (s, _) = virt::section(name).ok_or("secao desconhecida")?;
        let t = virt::translate(VirtAddr::new(s)).ok_or(alloc::format!(".{name} nao mapeada"))?;
        check!(
            t.flags.contains(PageFlags::WRITABLE) == w,
            ".{name}: WRITABLE={} esperado {}",
            !w,
            w
        );
        check!(
            t.flags.contains(PageFlags::NO_EXECUTE) == !x,
            ".{name}: NX={} esperado {}",
            x,
            !x
        );
        Ok(())
    };
    expect("text", false, true)?;
    expect("rodata", false, false)?;
    expect("data", true, false)?;
    let t = virt::translate(VirtAddr::new(KERNEL_STACK_TOP - 8)).ok_or("pilha nao mapeada")?;
    check!(
        t.flags
            .contains(PageFlags::WRITABLE | PageFlags::NO_EXECUTE),
        "pilha: {:?}",
        t.flags
    );
    Ok(())
}

fn test_page_fault_recovery() -> TestResult {
    let v = VirtAddr::new(TEST_VIRT + 0x1000);
    let r = probe(ProbeKind::Read, v.as_u64());
    check!(r.faulted, "leitura de pagina nao mapeada nao falhou");
    check!(r.cr2 == v.as_u64(), "cr2={:#x}", r.cr2);
    check!(
        !r.error.present() && !r.error.write(),
        "erro inesperado {:#x}",
        r.error.0
    );
    check!(r.fault_rip != 0, "rip da falta nao registrado");
    // Recupera: mapeia e repete o acesso, que agora deve funcionar.
    virt::alloc_and_map(v, PageFlags::KERNEL_RW).map_err(|e| alloc::format!("{e}"))?;
    let r = probe(ProbeKind::Read, v.as_u64());
    check!(!r.faulted, "leitura apos mapear falhou");
    virt::unmap_and_free(v).map_err(|e| alloc::format!("{e}"))?;
    let r = probe(ProbeKind::Read, v.as_u64());
    check!(
        r.faulted,
        "leitura apos desmapear nao falhou (TLB obsoleto?)"
    );
    Ok(())
}

fn test_guard_page() -> TestResult {
    let guard = KERNEL_STACK_BASE - 8;
    let r = probe(ProbeKind::Read, guard);
    check!(r.faulted, "guard page da pilha esta mapeada");
    check!(r.cr2 == guard, "cr2={:#x}", r.cr2);
    let heap_guard = nexo_boot_abi::KERNEL_HEAP_BASE - 8;
    let r = probe(ProbeKind::Read, heap_guard);
    check!(r.faulted, "guard page do heap esta mapeada");
    let inside = KERNEL_STACK_BASE + 8;
    let r = probe(ProbeKind::Read, inside);
    check!(!r.faulted, "base da pilha deveria estar mapeada");
    Ok(())
}

static RO_DATA: [u64; 8] = [0x5a5a_5a5a_5a5a_5a5a; 8];

fn test_write_protect() -> TestResult {
    let addr = RO_DATA.as_ptr() as u64;
    let (s, e) = virt::section("rodata").unwrap();
    check!(
        (s..e).contains(&addr),
        "RO_DATA nao esta em .rodata ({addr:#x})"
    );
    let r = probe(ProbeKind::Write, addr);
    check!(r.faulted, "escrita em .rodata nao falhou (CR0.WP?)");
    check!(
        r.error.present() && r.error.write(),
        "erro {:#x}",
        r.error.0
    );
    check!(RO_DATA[0] == 0x5a5a_5a5a_5a5a_5a5a, "dado alterado");
    let r = probe(ProbeKind::Read, addr);
    check!(!r.faulted, "leitura de .rodata falhou");
    Ok(())
}

fn test_nx() -> TestResult {
    // Página do heap (RW, NX) contendo `ret`.
    let code: Box<[u8]> = alloc::vec![0xc3u8; 64].into_boxed_slice();
    let addr = code.as_ptr() as u64;
    let r = probe(ProbeKind::Exec, addr);
    check!(r.faulted, "execucao em pagina NX nao falhou");
    check!(
        r.error.instruction_fetch() && r.error.present(),
        "erro {:#x}",
        r.error.0
    );
    check!(r.cr2 == addr, "cr2={:#x}", r.cr2);
    // Pilha também é NX.
    let mut on_stack = [0xc3u8; 16];
    let r = probe(ProbeKind::Exec, on_stack.as_mut_ptr() as u64);
    check!(r.faulted, "execucao na pilha nao falhou");
    Ok(())
}

fn test_heap() -> TestResult {
    let before = heap::stats();
    let mut v: Vec<u64> = Vec::new();
    for i in 0..10_000 {
        v.push(i);
    }
    check!(v.iter().sum::<u64>() == 49_995_000, "soma errada");
    let s = String::from("nexo") + "-" + crate::VERSION;
    check!(s == "nexo-0.0.1-boot", "string {s}");
    let boxes: Vec<Box<[u8; 4096]>> = (0..32).map(|i| Box::new([i as u8; 4096])).collect();
    check!(
        boxes.iter().enumerate().all(|(i, b)| b[4095] == i as u8),
        "conteudo de box"
    );
    let big = alloc::vec![7u8; 300 * 1024];
    check!(big[299 * 1024] == 7, "alocacao grande");
    #[repr(align(4096))]
    struct Aligned([u8; 8]);
    let a = Box::new(Aligned([1; 8]));
    check!(
        (&*a as *const Aligned as usize).is_multiple_of(4096),
        "alinhamento 4096"
    );
    check!(a.0[7] == 1, "conteudo alinhado");
    drop((v, s, boxes, big, a));
    let after = heap::stats();
    check!(
        after.used_bytes == before.used_bytes,
        "vazamento: {} -> {}",
        before.used_bytes,
        after.used_bytes
    );
    check!(
        after.allocations > before.allocations + 30,
        "contagem de alocacoes"
    );
    Ok(())
}

fn test_heap_grow() -> TestResult {
    let mapped_before = heap::mapped_bytes();
    let big = alloc::vec![1u8; 3 * 1024 * 1024];
    check!(heap::mapped_bytes() > mapped_before, "heap nao cresceu");
    check!(big.iter().all(|&b| b == 1), "conteudo apos crescimento");
    drop(big);
    check!(heap::oom_count() == 0, "OOM registrado");
    // Após liberar, o heap crescido continua mapeado e coalescido.
    check!(
        heap::stats().largest_free_kib() >= 3 * 1024,
        "heap nao coalesceu apos crescimento"
    );
    Ok(())
}

fn test_timer() -> TestResult {
    let t0 = crate::time::ticks();
    crate::time::sleep_ms(50);
    let t1 = crate::time::ticks();
    check!(t1 >= t0 + 50, "timer lento: {t0} -> {t1}");
    check!(t1 < t0 + 500, "timer rapido demais: {t0} -> {t1}");
    let tsc0 = cpu::rdtsc();
    crate::time::sleep_ms(10);
    check!(cpu::rdtsc() > tsc0, "TSC nao avanca");
    check!(crate::time::uptime_ms() >= 60, "uptime inconsistente");
    Ok(())
}

static SEQUENCE: SpinLock<Vec<u8>> = SpinLock::new(Vec::new());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn worker(tag: usize) {
    for _ in 0..3 {
        SEQUENCE.lock().push(tag as u8);
        COUNTER.fetch_add(1, Ordering::Relaxed);
        crate::task::yield_now();
    }
}

fn test_coop_tasks() -> TestResult {
    SEQUENCE.lock().clear();
    let a = crate::task::spawn("worker-a", worker, b'A' as usize);
    let b = crate::task::spawn("worker-b", worker, b'B' as usize);
    let mut rounds = 0;
    while !(crate::task::is_finished(a) && crate::task::is_finished(b)) {
        crate::task::yield_now();
        rounds += 1;
        check!(rounds < 100, "tarefas nao terminaram");
    }
    let seq = SEQUENCE.lock().clone();
    check!(
        seq == b"ABABAB",
        "sequencia {:?}",
        core::str::from_utf8(&seq).unwrap_or("?")
    );
    check!(COUNTER.load(Ordering::Relaxed) == 6, "contador");
    let reaped = crate::task::reap();
    check!(reaped == 2, "recolhidas {reaped}");
    kprint!(
        "(intercalacao {}; {} trocas) ",
        core::str::from_utf8(&seq).unwrap(),
        crate::task::switches()
    );
    Ok(())
}

fn test_symbols() -> TestResult {
    let addr = test_symbols as fn() -> TestResult as usize as u64;
    let s = crate::symbols::lookup(addr + 4).ok_or("simbolo nao encontrado")?;
    let name = alloc::format!("{}", s.demangled());
    check!(name.contains("test_symbols"), "nome {name}");
    check!(s.start == addr, "inicio {:#x} != {addr:#x}", s.start);
    let k = crate::symbols::lookup(
        crate::kmain as fn(&'static nexo_boot_abi::BootInfo) -> ! as usize as u64 + 1,
    )
    .ok_or("kmain nao encontrado")?;
    check!(
        alloc::format!("{}", k.demangled()).contains("kmain"),
        "kmain"
    );
    Ok(())
}

/// Cenário `test=fault`: leitura de endereço não mapeado sem sonda (fatal).
pub fn deliberate_fault() -> ! {
    let p = (TEST_VIRT + 0x7000) as *const u64;
    // SAFETY: deliberadamente inválido — o objetivo é exercitar o caminho fatal de #PF.
    let v = unsafe { p.read_volatile() };
    panic!("leitura invalida nao falhou: {v}");
}

/// Cenário `test=overflow`: recursão infinita até a guard page (→ #DF em IST1).
pub fn deliberate_stack_overflow() -> ! {
    #[allow(unconditional_recursion)]
    fn recurse(depth: u64, sink: &mut [u64; 64]) -> u64 {
        sink[(depth % 64) as usize] = depth;
        let mut next = [0u64; 64];
        core::hint::black_box(&mut next);
        depth + recurse(depth + 1, &mut next)
    }
    let mut s = [0u64; 64];
    let r = recurse(0, &mut s);
    panic!("recursao terminou: {r}");
}
