//! Auto-testes executados no boot e verificados pelo CI via serial.
//!
//! Formato: `[TEST] nome ... ok|FAIL: motivo` e, ao final,
//! `[RESULT] PASS n/n` ou `[RESULT] FAIL k/n`.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::gdt::{KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR};
use nexo_arch_x86_64::paging::PageFlags;
use nexo_boot_abi::{KERNEL_STACK_BASE, KERNEL_STACK_TOP};
use nexo_mm::{PAGE_SIZE, PhysAddr, VirtAddr};

use crate::mm::heap::HeapStatsExt;
use crate::mm::{heap, phys, virt};
use crate::sched;
use crate::sync::IrqLock;
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
    ("acpi", test_acpi),
    ("apic_timer", test_apic_timer),
    ("tsc_clock", test_tsc_clock),
    ("ioapic", test_ioapic),
    ("ipi_self", test_ipi_self),
    ("smp", test_smp),
    ("threads_yield", test_threads_yield),
    ("threads_preempt", test_threads_preempt),
    ("threads_sleep_join", test_threads_sleep_join),
    ("threads_spawn_churn", test_threads_spawn_churn),
    ("threads_multi_cpu", test_threads_multi_cpu),
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
        "[TIME] ticks={} uptime_ms={} monotonic_ms={} tsc_hz={} apic_timer_hz={} excecoes={} trocas_de_contexto={} espurias={}\n",
        crate::time::ticks(),
        crate::time::uptime_ms(),
        crate::time::monotonic_ns() / 1_000_000,
        crate::time::tsc_hz(),
        crate::time::apic_timer_hz(),
        crate::x86::traps::exception_count(),
        sched::switches(),
        crate::x86::traps::spurious_count()
    );
    kprint!(
        "[SMP] cpus_online={} timer_irqs_total={} ipis_total={}\n",
        crate::x86::percpu::online_count(),
        crate::x86::traps::timer_irq_count(),
        crate::x86::traps::ipi_count()
    );
    let s = sched::stats();
    let cur = sched::current();
    kprint!(
        "[SCHED] threads={} prontas={} dormindo={} spawned={} reaped={} preempcoes={} atual={} estado={:?} pilha_propria={}\n",
        s.alive,
        s.ready,
        s.sleeping,
        s.spawned,
        s.reaped,
        s.preemptions,
        cur.as_ref().map_or("?", |t| t.name),
        cur.as_ref().map(|t| t.state()),
        cur.as_ref().is_some_and(|t| t.stack_bounds().is_some())
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
    let u0 = crate::time::uptime_ms();
    crate::time::sleep_ms(50);
    let t1 = crate::time::ticks();
    let u1 = crate::time::uptime_ms();
    check!(t1 > t0, "tick parado: {t0} -> {t1}");
    check!(t1 <= t0 + 500, "tick rapido demais: {t0} -> {t1}");
    check!(u1 >= u0 + 45, "sleep_ms(50) durou {} ms", u1 - u0);
    let tsc0 = cpu::rdtsc();
    crate::time::sleep_ms(10);
    check!(cpu::rdtsc() > tsc0, "TSC nao avanca");
    check!(crate::time::uptime_ms() >= 60, "uptime inconsistente");
    Ok(())
}

fn test_acpi() -> TestResult {
    let p = crate::acpi::info();
    check!(!p.cpus().is_empty(), "nenhuma CPU");
    check!(
        p.cpus()[0].apic_id == p.bsp_apic_id,
        "BSP nao e a primeira CPU"
    );
    let (msr_base, enabled, is_bsp) = nexo_arch_x86_64::apic::apic_base();
    check!(
        enabled && is_bsp,
        "MSR APIC_BASE: enable={enabled} bsp={is_bsp}"
    );
    if p.lapic_phys != 0 {
        check!(
            p.lapic_phys == msr_base,
            "LAPIC MADT {:#x} != MSR {:#x}",
            p.lapic_phys,
            msr_base
        );
    }
    check!(
        crate::x86::apic::lapic().id() == p.bsp_apic_id,
        "LAPIC id difere do CPUID"
    );
    Ok(())
}

fn test_apic_timer() -> TestResult {
    let hz = crate::time::apic_timer_hz();
    check!(hz > 1_000_000, "timer LAPIC lento demais: {hz} Hz");
    // Contador desta CPU (a BSP): o global soma os timers de todas as CPUs.
    let me = crate::x86::percpu::current();
    let irqs0 = me.timer_irqs.load(Ordering::Relaxed);
    let t0 = crate::time::ticks();
    let c0 = crate::x86::apic::lapic().timer_current();
    crate::time::sleep_ms(30);
    let irqs1 = me.timer_irqs.load(Ordering::Relaxed);
    // Em TCG as expiracoes coalescem durante `hlt`: exige-se progresso, nao contagem exata.
    check!(irqs1 > irqs0, "nenhuma interrupcao do timer em 30 ms");
    check!(
        irqs1 <= irqs0 + 300,
        "interrupcoes demais em 30 ms: {}",
        irqs1 - irqs0
    );
    check!(
        crate::time::ticks() - t0 == irqs1 - irqs0,
        "ticks != interrupcoes do timer"
    );
    let c1 = crate::x86::apic::lapic().timer_current();
    check!(
        c0 != c1 || c0 == 0,
        "contagem do timer LAPIC parada em {c0}"
    );
    let sp = crate::x86::traps::spurious_count();
    check!(sp == 0, "interrupcoes espurias: {sp}");
    Ok(())
}

fn test_tsc_clock() -> TestResult {
    let hz = crate::time::tsc_hz();
    check!(hz > 10_000_000, "TSC calibrado em {hz} Hz");
    let a = crate::time::monotonic_ns();
    let b = crate::time::monotonic_ns();
    check!(b >= a, "monotonic_ns retrocedeu: {a} -> {b}");
    let t0 = crate::time::ticks();
    let n0 = crate::time::monotonic_ns();
    crate::time::sleep_ms(100);
    let dt_ms = crate::time::ticks() - t0;
    let dn_ms = (crate::time::monotonic_ns() - n0) / 1_000_000;
    // Invariantes: o sleep durou >= 100 ms reais; ticks nunca correm mais que o tempo real.
    check!(
        (95..=2000).contains(&dn_ms),
        "sleep_ms(100) durou {dn_ms} ms pelo TSC"
    );
    check!(
        dt_ms <= dn_ms * 5 / 4 + 5,
        "ticks ({dt_ms}) mais rapidos que o TSC ({dn_ms} ms)"
    );
    check!(dt_ms >= 1, "nenhum tick em {dn_ms} ms");
    let d0 = crate::time::monotonic_ns();
    crate::time::delay_us(2000);
    let d = (crate::time::monotonic_ns() - d0) / 1000;
    check!((1500..=20_000).contains(&d), "delay_us(2000) durou {d} us");
    Ok(())
}

fn test_ioapic() -> TestResult {
    check!(crate::x86::apic::ioapic_count() >= 1, "nenhum I/O APIC");
    let bsp = crate::acpi::info().bsp_apic_id;
    let before = crate::x86::traps::ioapic_test_count();
    let gsi = crate::x86::apic::route_isa_irq(0, crate::x86::apic::vectors::IOAPIC_TEST, bsp)
        .map_err(String::from)?;
    // PIT canal 0 periodico a ~200 Hz -> IRQ0 -> GSI -> vetor de teste.
    // SAFETY: o canal 0 do PIT esta livre (o tick do sistema vem do LAPIC).
    let div = unsafe { nexo_arch_x86_64::pit::configure_periodic(200) };
    crate::time::sleep_ms(50);
    crate::x86::apic::mask_gsi(gsi);
    // SAFETY: encerra o canal 0.
    unsafe { nexo_arch_x86_64::pit::channel0_stop() };
    let got = crate::x86::traps::ioapic_test_count() - before;
    check!(
        got >= 5,
        "PIT via I/O APIC (gsi {gsi}, divisor {div}): {got} interrupcoes em 50 ms"
    );
    crate::time::sleep_ms(10);
    let late = crate::x86::traps::ioapic_test_count() - before - got;
    check!(late <= 1, "{late} interrupcoes apos mascarar o GSI");
    kprint!("(gsi {gsi}, {got} irqs) ");
    Ok(())
}

fn test_ipi_self() -> TestResult {
    let before = crate::x86::traps::ipi_count();
    crate::x86::apic::lapic().send_ipi_self(crate::x86::apic::vectors::RESCHED);
    crate::time::delay_us(500);
    check!(
        crate::x86::traps::ipi_count() == before + 1,
        "IPI RESCHED para si nao chegou"
    );
    crate::x86::apic::lapic().send_ipi_self(crate::x86::apic::vectors::TLB_FLUSH);
    crate::time::delay_us(500);
    check!(
        crate::x86::traps::ipi_count() == before + 2,
        "IPI TLB_FLUSH para si nao chegou"
    );
    Ok(())
}

fn test_smp() -> TestResult {
    let expected = crate::acpi::info().cpus().len();
    let online = crate::x86::percpu::online_count();
    check!(online == expected, "{online} de {expected} CPUs online");
    let me = crate::x86::percpu::current();
    check!(
        me.index == 0,
        "auto-teste nao esta na BSP (cpu{})",
        me.index
    );
    check!(
        me.apic_id == crate::acpi::info().bsp_apic_id,
        "apic_id da BSP"
    );
    if online > 1 {
        let before: Vec<u64> = (1..online)
            .map(|i| crate::x86::percpu::get(i).map_or(0, |c| c.ipis.load(Ordering::Relaxed)))
            .collect();
        crate::x86::smp::broadcast_resched();
        crate::time::sleep_ms(20);
        for i in 1..online {
            let c = crate::x86::percpu::get(i).ok_or("cpu ausente")?;
            check!(c.online.load(Ordering::Relaxed), "cpu{i} offline");
            let ipis = c.ipis.load(Ordering::Relaxed);
            check!(
                ipis > before[i - 1],
                "cpu{i} nao recebeu a IPI de broadcast"
            );
            check!(
                c.timer_irqs.load(Ordering::Relaxed) > 0,
                "timer local da cpu{i} parado"
            );
        }
        // TLB shootdown: unmap gera IPI para as outras CPUs.
        let v = VirtAddr::new(TEST_VIRT + 0x9000);
        virt::alloc_and_map(v, PageFlags::KERNEL_RW).map_err(|e| alloc::format!("{e}"))?;
        let sum = || -> u64 {
            (1..online)
                .map(|i| crate::x86::percpu::get(i).map_or(0, |c| c.ipis.load(Ordering::Relaxed)))
                .sum()
        };
        let before_flush = sum();
        virt::unmap_and_free(v).map_err(|e| alloc::format!("{e}"))?;
        crate::time::sleep_ms(5);
        check!(
            sum() >= before_flush + (online as u64 - 1),
            "shootdown de TLB nao chegou a todas as CPUs"
        );
    }
    kprint!("({online} CPUs online) ");
    Ok(())
}

static SEQUENCE: IrqLock<Vec<u8>> = IrqLock::new(Vec::new());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn worker(tag: usize) {
    for _ in 0..3 {
        SEQUENCE.lock().push(tag as u8);
        COUNTER.fetch_add(1, Ordering::Relaxed);
        sched::yield_now();
    }
}

fn test_threads_yield() -> TestResult {
    SEQUENCE.lock().clear();
    COUNTER.store(0, Ordering::Relaxed);
    let a = sched::spawn("worker-a", worker, b'A' as usize);
    let b = sched::spawn("worker-b", worker, b'B' as usize);
    check!(sched::join(a) && sched::join(b), "join falhou");
    let seq = SEQUENCE.lock().clone();
    check!(
        seq.len() == 6,
        "sequencia {:?}",
        core::str::from_utf8(&seq).unwrap_or("?")
    );
    check!(
        seq.iter().filter(|&&c| c == b'A').count() == 3,
        "A executou {} vezes",
        seq.iter().filter(|&&c| c == b'A').count()
    );
    check!(COUNTER.load(Ordering::Relaxed) == 6, "contador");
    let reaped = sched::reap();
    check!(reaped == 2, "recolhidas {reaped}");
    kprint!(
        "({}; {} trocas) ",
        core::str::from_utf8(&seq).unwrap_or("?"),
        sched::switches()
    );
    Ok(())
}

static SPIN_COUNTS: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];
static SPIN_STOP: AtomicBool = AtomicBool::new(false);

fn spinner(i: usize) {
    while !SPIN_STOP.load(Ordering::Relaxed) {
        SPIN_COUNTS[i].fetch_add(1, Ordering::Relaxed);
        core::hint::spin_loop();
    }
}

fn test_threads_preempt() -> TestResult {
    // Mais threads que CPUs, todas girando sem ceder: so a preempcao pelo timer as intercala.
    let n = (crate::x86::percpu::online_count() + 1).min(8);
    for c in &SPIN_COUNTS {
        c.store(0, Ordering::Relaxed);
    }
    SPIN_STOP.store(false, Ordering::Relaxed);
    let p0 = sched::stats().preemptions;
    let ids: Vec<_> = (0..n)
        .map(|i| sched::spawn("spinner", spinner, i))
        .collect();
    sched::sleep_ms(150);
    SPIN_STOP.store(true, Ordering::Relaxed);
    for id in &ids {
        check!(sched::join(*id), "join do spinner");
    }
    for (i, counter) in SPIN_COUNTS.iter().enumerate().take(n) {
        let c = counter.load(Ordering::Relaxed);
        check!(c > 0, "spinner {i} nunca executou (preempcao falhou)");
    }
    let p1 = sched::stats().preemptions;
    check!(p1 > p0, "nenhuma preempcao registrada");
    sched::reap();
    kprint!("({n} spinners, {} preempcoes) ", p1 - p0);
    Ok(())
}

static FLAG: AtomicBool = AtomicBool::new(false);

fn sleeper(ms: usize) {
    sched::sleep_ms(ms as u64);
    FLAG.store(true, Ordering::Release);
}

fn test_threads_sleep_join() -> TestResult {
    FLAG.store(false, Ordering::Relaxed);
    let t0 = crate::time::monotonic_ns();
    let id = sched::spawn("sleeper", sleeper, 30);
    check!(!sched::is_finished(id), "terminou cedo demais");
    check!(sched::join(id), "join");
    let dt = (crate::time::monotonic_ns() - t0) / 1_000_000;
    check!(FLAG.load(Ordering::Acquire), "flag nao setada");
    check!(dt >= 30, "join voltou apos {dt} ms (< 30)");
    check!(dt < 2000, "join demorou {dt} ms");
    check!(sched::join(id), "join repetido deve ser verdadeiro");
    check!(!sched::join(usize::MAX), "join de id inexistente");
    sched::reap();
    Ok(())
}

fn short_lived(arg: usize) {
    let v = alloc::vec![arg as u8; 128];
    core::hint::black_box(&v);
}

fn test_threads_spawn_churn() -> TestResult {
    sched::reap();
    let frames0 = phys::stats().free;
    let heap0 = heap::stats().used_bytes;
    let slots0 = sched::stack::in_use();
    for round in 0..10 {
        let ids: Vec<_> = (0..8)
            .map(|k| sched::spawn("churn", short_lived, round * 8 + k))
            .collect();
        for id in ids {
            check!(sched::join(id), "join na rodada {round}");
        }
        sched::reap();
    }
    let s = sched::stats();
    check!(s.reaped >= 80, "recolhidas {}", s.reaped);
    check!(
        sched::stack::in_use() == slots0,
        "slots de pilha vazando: {} -> {}",
        slots0,
        sched::stack::in_use()
    );
    let frames1 = phys::stats().free;
    check!(
        frames1 + 4 >= frames0,
        "quadros vazando: {frames0} -> {frames1}"
    );
    let heap1 = heap::stats().used_bytes;
    check!(heap1 <= heap0 + 4096, "heap vazando: {heap0} -> {heap1}");
    Ok(())
}

static CPU_SEEN: AtomicU64 = AtomicU64::new(0);

fn cpu_recorder(_: usize) {
    for _ in 0..50 {
        if let Some(c) = crate::x86::percpu::try_current() {
            CPU_SEEN.fetch_or(1 << c.index.min(63), Ordering::Relaxed);
        }
        sched::yield_now();
        crate::time::delay_us(200);
    }
}

fn test_threads_multi_cpu() -> TestResult {
    CPU_SEEN.store(0, Ordering::Relaxed);
    let n = crate::x86::percpu::online_count() * 2;
    let ids: Vec<_> = (0..n)
        .map(|i| sched::spawn("recorder", cpu_recorder, i))
        .collect();
    for id in ids {
        check!(sched::join(id), "join");
    }
    sched::reap();
    let mask = CPU_SEEN.load(Ordering::Relaxed);
    if crate::x86::percpu::online_count() > 1 {
        check!(
            mask.count_ones() >= 2,
            "threads executaram apenas em cpus {mask:#b}"
        );
    } else {
        check!(mask == 1, "mascara {mask:#b}");
    }
    kprint!("(cpus {mask:#b}) ");
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
