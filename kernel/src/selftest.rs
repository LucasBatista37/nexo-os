//! Auto-testes executados no boot e verificados pelo CI via serial.
//!
//! Formato: `[TEST] nome ... ok|FAIL: motivo` e, ao final,
//! `[RESULT] PASS n/n` ou `[RESULT] FAIL k/n`.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
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
    ("timers", test_timers),
    ("threads_affinity", test_threads_affinity),
    ("user_process", test_user_process),
    ("user_isolation", test_user_isolation),
    ("user_syscall_error", test_user_syscall_error),
    ("user_ipc", test_user_ipc),
    ("user_services", test_user_services),
    ("user_syscall_fuzz", test_user_syscall_fuzz),
    ("pci", test_pci),
    ("user_block", test_user_block),
    ("user_block_crash", test_user_block_crash),
    ("user_fs", test_user_fs),
    ("user_devmgr", test_user_devmgr),
    ("user_vfs", test_user_vfs),
    ("user_wait_any", test_user_wait_any),
    ("user_shmem", test_user_shmem),
    ("user_wm", test_user_wm),
    ("user_wm_multi", test_user_wm_multi),
    ("user_wm_restack", test_user_wm_restack),
    ("user_wm_resize", test_user_wm_resize),
    ("user_wm_input", test_user_wm_input),
    ("user_wm_keyboard", test_user_wm_keyboard),
    ("user_wm_alpha", test_user_wm_alpha),
    ("user_wm_ui", test_user_wm_ui),
    ("user_wm_maximize", test_user_wm_maximize),
    ("user_wm_shortcut", test_user_wm_shortcut),
    ("user_wm_present", test_user_wm_present),
    ("user_wm_tile", test_user_wm_tile),
    ("user_wm_grab", test_user_wm_grab),
    ("user_wm_displays", test_user_wm_displays),
    ("user_greeter", test_user_greeter),
    ("user_wm_context", test_user_wm_context),
    ("user_wm_clipboard", test_user_wm_clipboard),
    ("user_wm_notify", test_user_wm_notify),
    ("user_wm_dnd", test_user_wm_dnd),
    ("user_wm_a11y", test_user_wm_a11y),
    ("user_wm_shell", test_user_wm_shell),
    ("user_wm_scale", test_user_wm_scale),
    ("user_wm_center", test_user_wm_center),
    ("user_shellui", test_user_shellui),
    ("user_shellcenter", test_user_shellcenter),
    ("user_calc", test_user_calc),
    ("user_install", test_user_install),
    ("user_spawn_mem", test_user_spawn_mem),
    ("user_launcher", test_user_launcher),
    ("user_launch_gui", test_user_launch_gui),
    ("user_consent", test_user_consent),
    ("user_config", test_user_config),
    ("user_monitor", test_user_monitor),
    ("user_agenda", test_user_agenda),
    ("user_term", test_user_term),
    ("user_visor", test_user_visor),
    ("user_editor", test_user_editor),
    ("gfx", test_gfx),
    ("symbols", test_symbols),
];

/// Executa todos os testes. Devolve `true` se todos passaram.
pub fn run() -> bool {
    kprint!("[SELFTEST] iniciando {} testes\n", TESTS.len());
    let mut passed = 0;
    for (name, f) in TESTS {
        kdebug!("selftest: iniciando {name}");
        // O resultado sai em uma única linha ao final, para que logs do kernel
        // emitidos durante o teste não quebrem o marcador lido pelo CI.
        match f() {
            Ok(()) => {
                passed += 1;
                kprint!("[TEST] {name} ... ok\n");
            }
            Err(e) => kprint!("[TEST] {name} ... FAIL: {e}\n"),
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

static AFFINITY_SEEN: [AtomicUsize; 64] = [const { AtomicUsize::new(usize::MAX) }; 64];

fn pinned(i: usize) {
    for _ in 0..20 {
        let here = crate::x86::percpu::current().index;
        let prev = AFFINITY_SEEN[i].load(Ordering::Relaxed);
        if prev != usize::MAX && prev != here {
            AFFINITY_SEEN[i].store(usize::MAX - 1, Ordering::Relaxed); // migrou: erro
            return;
        }
        AFFINITY_SEEN[i].store(here, Ordering::Relaxed);
        sched::yield_now();
        crate::time::delay_us(100);
    }
}

fn test_threads_affinity() -> TestResult {
    let n = crate::x86::percpu::online_count().min(64);
    for s in AFFINITY_SEEN.iter().take(n) {
        s.store(usize::MAX, Ordering::Relaxed);
    }
    let me = crate::x86::percpu::current().index;
    check!(
        me == 0,
        "thread principal deveria estar presa a cpu0 (esta em cpu{me})"
    );
    let ids: Vec<_> = (0..n)
        .map(|i| sched::spawn_on("pinned", pinned, i, i))
        .collect();
    for id in &ids {
        check!(sched::join(*id), "join");
    }
    for (i, s) in AFFINITY_SEEN.iter().enumerate().take(n) {
        let seen = s.load(Ordering::Relaxed);
        check!(seen == i, "thread presa a cpu{i} executou em {seen}");
    }
    check!(
        !sched::set_affinity(usize::MAX, 1),
        "afinidade de id inexistente"
    );
    sched::reap();
    check!(crate::x86::percpu::current().index == 0, "principal migrou");
    Ok(())
}

fn run_utest(mode: u64) -> Result<i64, String> {
    let p = crate::process::spawn_named("utest", mode, Vec::new()).map_err(String::from)?;
    Ok(crate::process::wait_and_reap(&p))
}

fn test_user_process() -> TestResult {
    let logs0 = crate::process::user_log_count();
    let frames0 = phys::stats().free;
    let code = run_utest(0)?;
    check!(code == 0, "init saiu com {code}");
    check!(
        crate::process::user_log_count() >= logs0 + 2,
        "logs do usuario nao chegaram"
    );
    let last = crate::x86::syscall::last_user_log();
    check!(
        last.starts_with("utest: ok pid="),
        "ultima mensagem: {last:?}"
    );
    check!(
        crate::process::count() == 0,
        "processo nao foi removido da tabela"
    );
    let frames1 = phys::stats().free;
    check!(
        frames1 + 8 >= frames0,
        "quadros do processo vazaram: {frames0} -> {frames1}"
    );
    check!(
        cpu::read_cr3() & !0xfff == sched::kernel_pml4().as_u64(),
        "CR3 nao voltou ao kernel"
    );
    kprint!("({last}) ");
    Ok(())
}

fn test_user_isolation() -> TestResult {
    let frames0 = phys::stats().free;
    let exc0 = crate::x86::traps::exception_count();
    for (mode, what) in [
        (1u64, "leitura do kernel"),
        (3, "instrucao privilegiada"),
        (4, "escrita em .rodata"),
    ] {
        let code = run_utest(mode)?;
        check!(
            code == nexo_syscall_abi::EXIT_KILLED,
            "{what}: processo saiu com {code} em vez de ser morto"
        );
    }
    check!(
        crate::x86::traps::exception_count() >= exc0 + 3,
        "excecoes de usuario nao contadas"
    );
    check!(crate::process::count() == 0, "processos sobrando");
    let frames1 = phys::stats().free;
    check!(
        frames1 + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames1}"
    );
    // O kernel continua integro: uma sonda de kernel ainda funciona.
    let r = probe(ProbeKind::Read, KERNEL_STACK_BASE + 8);
    check!(!r.faulted, "kernel instavel apos matar processos");
    Ok(())
}

fn test_user_syscall_error() -> TestResult {
    let code = run_utest(2)?;
    check!(
        code == nexo_syscall_abi::Status::NotSupported as u64 as i64,
        "status recebido pelo usuario: {code}"
    );
    Ok(())
}

fn test_user_ipc() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let sent0 = crate::ipc::messages_sent();
    let (a, b) = ChannelEnd::create_pair();
    let rights = Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT);
    let ha = alloc::vec![Handle {
        object: Object::Channel(a),
        rights
    }];
    let hb = alloc::vec![Handle {
        object: Object::Channel(b),
        rights
    }];
    let server = crate::process::spawn_named("utest", 5, ha).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 6, hb).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let sc = crate::process::wait_and_reap(&server);
    drop((server, client));
    sched::reap();
    check!(cc == 0, "cliente saiu com {cc}");
    check!(sc == 0, "servidor saiu com {sc}");
    let sent = crate::ipc::messages_sent() - sent0;
    check!(sent >= 4, "mensagens: {sent}");
    let ends = crate::ipc::live_channel_ends();
    check!(
        ends == ends0,
        "extremidades de canal vazaram: {ends0} -> {ends}"
    );
    check!(crate::process::count() == 0, "processos sobrando");
    kprint!("({sent} mensagens) ");
    Ok(())
}

fn test_user_services() -> TestResult {
    // init -> svcmgr -> echo (cai de proposito) + echo-client: o servico e reiniciado sem reiniciar o kernel.
    check!(
        crate::initrd::count() >= 5,
        "initrd com {} membros",
        crate::initrd::count()
    );
    let live0 = crate::process::live_max();
    let restarts0 = crate::x86::syscall::restart_log_count();
    let spawned0 = crate::process::spawned_total();
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let p = crate::process::spawn_named("init", 0, Vec::new()).map_err(String::from)?;
    let code = crate::process::wait_and_reap(&p);
    drop(p);
    sched::reap();
    check!(code == 0, "init saiu com {code}");
    let restarts = crate::x86::syscall::restart_log_count() - restarts0;
    check!(restarts >= 1, "nenhum reinicio de servico registrado");
    check!(
        crate::process::live_max() >= 4,
        "maximo de processos simultaneos: {}",
        crate::process::live_max()
    );
    let spawned = crate::process::spawned_total() - spawned0;
    check!(spawned >= 5, "processos criados: {spawned}");
    check!(
        crate::process::count() == 0,
        "processos sobrando: {}",
        crate::process::count()
    );
    sched::reap();
    check!(
        crate::ipc::live_channel_ends() == ends0,
        "canais vazaram: {} -> {}",
        ends0,
        crate::ipc::live_channel_ends()
    );
    let frames1 = phys::stats().free;
    check!(
        frames1 + 32 >= frames0,
        "quadros vazaram: {frames0} -> {frames1}"
    );
    let _ = live0;
    kprint!(
        "({spawned} processos, {restarts} reinicio(s), max {} simultaneos) ",
        crate::process::live_max()
    );
    Ok(())
}

fn test_user_syscall_fuzz() -> TestResult {
    let frames0 = phys::stats().free;
    let ends0 = crate::ipc::live_channel_ends();
    let exc0 = crate::x86::traps::exception_count();
    let code = run_utest(7)?;
    check!(code == 0, "fuzz de syscalls saiu com {code}");
    check!(crate::process::count() == 0, "processo do fuzz sobrando");
    sched::reap();
    check!(
        crate::ipc::live_channel_ends() == ends0,
        "canais vazaram no fuzz: {} -> {}",
        ends0,
        crate::ipc::live_channel_ends()
    );
    let frames1 = phys::stats().free;
    check!(
        frames1 + 32 >= frames0,
        "quadros vazaram no fuzz: {frames0} -> {frames1}"
    );
    let last = crate::x86::syscall::last_user_log();
    check!(
        last.starts_with("utest: fuzz terminou"),
        "ultima mensagem: {last:?}"
    );
    kprint!(
        "(coletor fechou {} ponta(s)) ",
        crate::ipc::collected_ends()
    );
    let r = probe(ProbeKind::Read, KERNEL_STACK_BASE + 8);
    check!(!r.faulted, "kernel instavel apos o fuzz");
    kprint!(
        "({}; {} excecoes) ",
        last,
        crate::x86::traps::exception_count() - exc0
    );
    Ok(())
}

static TIMER_LOG: IrqLock<Vec<(usize, u64)>> = IrqLock::new(Vec::new());
static PERIODIC_HITS: AtomicUsize = AtomicUsize::new(0);

fn timer_cb(arg: usize) {
    TIMER_LOG.lock().push((arg, crate::time::monotonic_ns()));
}

fn periodic_cb(_: usize) {
    PERIODIC_HITS.fetch_add(1, Ordering::Relaxed);
}

fn test_timers() -> TestResult {
    TIMER_LOG.lock().clear();
    PERIODIC_HITS.store(0, Ordering::Relaxed);
    let t0 = crate::time::monotonic_ns();
    let a = crate::timer::after_ns(30_000_000, timer_cb, 3);
    let b = crate::timer::after_ns(10_000_000, timer_cb, 1);
    let c = crate::timer::after_ns(20_000_000, timer_cb, 2);
    let cancelled = crate::timer::after_ns(15_000_000, timer_cb, 99);
    check!(crate::timer::cancel(cancelled), "cancelamento");
    check!(!crate::timer::cancel(cancelled), "cancelamento duplo");
    let p = crate::timer::periodic_ns(5_000_000, periodic_cb, 0);
    sched::sleep_ms(60);
    check!(crate::timer::cancel(p), "cancelar periodico");
    let hits = PERIODIC_HITS.load(Ordering::Relaxed);
    check!(
        (6..=14).contains(&hits),
        "periodico de 5 ms disparou {hits} vezes em 60 ms"
    );
    let log = TIMER_LOG.lock().clone();
    check!(
        log.len() == 3,
        "{} disparos (esperado 3): {:?}",
        log.len(),
        log.iter().map(|e| e.0).collect::<Vec<_>>()
    );
    check!(
        log.iter().map(|e| e.0).collect::<Vec<_>>() == alloc::vec![1, 2, 3],
        "ordem {:?}",
        log.iter().map(|e| e.0).collect::<Vec<_>>()
    );
    for (arg, at) in &log {
        let dt = (at - t0) / 1_000_000;
        let want = *arg as u64 * 10;
        check!(
            dt >= want && dt < want + 30,
            "timer {arg} disparou em {dt} ms (esperado >= {want})"
        );
    }
    check!(
        crate::timer::pending_count() == 0,
        "timers pendentes sobrando"
    );
    check!(
        crate::timer::fired_total() >= 3 + hits as u64,
        "total de disparos {}",
        crate::timer::fired_total()
    );
    let _ = (a, b, c);
    Ok(())
}

fn test_pci() -> TestResult {
    let devs = crate::pci::devices();
    check!(!devs.is_empty(), "nenhuma funcao PCI enumerada");
    check!(
        devs.iter().any(|d| d.class == 0x06 && d.subclass == 0x00),
        "host bridge ausente"
    );
    let virtio: alloc::vec::Vec<_> = devs.iter().filter(|d| d.is_virtio()).collect();
    check!(
        !virtio.is_empty(),
        "nenhum dispositivo virtio (rode com o disco de dados)"
    );
    for d in &virtio {
        let mmio = d.bars.iter().filter(|b| b.size != 0 && b.flags & 1 == 0);
        let mut n = 0;
        for b in mmio {
            check!(
                b.size.is_power_of_two(),
                "BAR de {:#06x} com tamanho {:#x}",
                d.bdf,
                b.size
            );
            check!(
                b.base.is_multiple_of(b.size),
                "BAR de {:#06x} desalinhado: {:#x}",
                d.bdf,
                b.base
            );
            check!(
                crate::pci::is_mmio_range(None, b.base, b.size),
                "BAR nao reconhecido como MMIO"
            );
            check!(
                crate::pci::is_mmio_range(Some(d.bdf), b.base, b.size),
                "BAR nao reconhecido para o proprio BDF"
            );
            check!(
                !crate::pci::is_mmio_range(Some(d.bdf ^ 0x1), b.base, b.size),
                "BAR aceito para outro BDF"
            );
            n += 1;
        }
        check!(
            n >= 1,
            "virtio {:#06x} sem BAR MMIO (interface moderna)",
            d.bdf
        );
    }
    // Estado documentado do DMA (ADR-0015): sem traducao IOMMU, o caminho e inseguro.
    check!(
        crate::iommu::dma_is_unrestricted(),
        "modo de IOMMU inesperado: {:?}",
        crate::iommu::mode()
    );
    check!(
        !crate::pci::is_mmio_range(None, 0, PAGE_SIZE),
        "pagina 0 aceita como MMIO"
    );
    check!(
        !crate::pci::is_mmio_range(None, u64::MAX - 4096, 8192),
        "overflow aceito como MMIO"
    );
    Ok(())
}

fn test_user_block() -> TestResult {
    use crate::ipc::{ChannelEnd, DeviceGrant, Handle, Object, Rights};
    if !crate::pci::devices()
        .iter()
        .any(|d| d.is_virtio() && (d.device == 0x1001 || d.device == 0x1042))
    {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let irqs0 = crate::irq::total();
    let (a, b) = ChannelEnd::create_pair();
    let hdrv = alloc::vec![
        Handle {
            object: Object::Device(Arc::new(DeviceGrant::all())),
            rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
        },
        Handle {
            object: Object::Channel(a),
            rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
        },
    ];
    let hcli = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let driver = crate::process::spawn_named("blockdev", 0, hdrv).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 8, hcli).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let dc = crate::process::wait_and_reap(&driver);
    drop((driver, client));
    sched::reap();
    check!(cc == 0, "cliente de bloco saiu com {cc}");
    check!(dc == 0, "driver saiu com {dc}");
    let irqs = crate::irq::total() - irqs0;
    kinfo!("irq: {irqs} interrupcao(oes) MSI-X entregue(s) ao driver de bloco");
    check!(
        irqs >= 1,
        "nenhuma interrupcao MSI-X chegou ao vetor de usuario"
    );
    check!(
        crate::irq::alloc() == Some(crate::irq::USER_VECTOR_BASE),
        "vetor nao devolvido no fim do driver"
    );
    crate::irq::free(crate::irq::USER_VECTOR_BASE);
    let ends = crate::ipc::live_channel_ends();
    check!(
        ends == ends0,
        "extremidades de canal vazaram: {ends0} -> {ends}"
    );
    let frames = settled_free_frames(frames0, 4);
    // As paginas de DMA e as tabelas de paginas do driver devem ter sido liberadas.
    check!(
        frames + 4 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Quadros livres apos dar tempo de as threads mortas serem colhidas (os quadros de um
/// processo sao liberados quando o ultimo `Arc` cai, um instante depois de ele sair da tabela).
fn settled_free_frames(frames0: u64, slack: u64) -> u64 {
    let mut frames = phys::stats().free;
    for _ in 0..200 {
        if frames + slack >= frames0 {
            break;
        }
        sched::sleep_ms(10);
        sched::reap();
        frames = phys::stats().free;
    }
    frames
}

fn has_virtio_blk() -> bool {
    crate::pci::devices()
        .iter()
        .any(|d| d.is_virtio() && (d.device == 0x1001 || d.device == 0x1042))
}

fn channel_handle(end: Arc<crate::ipc::ChannelEnd>) -> crate::ipc::Handle {
    crate::ipc::Handle {
        object: crate::ipc::Object::Channel(end),
        rights: crate::ipc::Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }
}

fn device_handle() -> crate::ipc::Handle {
    crate::ipc::Handle {
        object: crate::ipc::Object::Device(Arc::new(crate::ipc::DeviceGrant::all())),
        rights: crate::ipc::Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
    }
}

/// O driver de bloco cai (falta de pagina deliberada) no 2o pedido: o cliente ve o canal
/// fechado, o kernel continua e nada vaza.
fn test_user_block_crash() -> TestResult {
    use crate::ipc::ChannelEnd;
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let driver = crate::process::spawn_named(
        "blockdev",
        1,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 8, alloc::vec![channel_handle(b)])
        .map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let dc = crate::process::wait_and_reap(&driver);
    drop((driver, client));
    sched::reap();
    check!(
        dc == -1,
        "driver deveria ter sido morto pelo kernel; saiu com {dc}"
    );
    check!(cc != 0, "cliente deveria falhar apos a queda do driver");
    let ends = crate::ipc::live_channel_ends();
    check!(
        ends == ends0,
        "extremidades de canal vazaram: {ends0} -> {ends}"
    );
    let frames = settled_free_frames(frames0, 4);
    check!(
        frames + 4 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    check!(
        crate::irq::alloc() == Some(crate::irq::USER_VECTOR_BASE),
        "vetor nao devolvido apos a queda"
    );
    crate::irq::free(crate::irq::USER_VECTOR_BASE);
    Ok(())
}

/// blockdev <-> fs <-> utest(9): arquivos criados, lidos, alterados, listados e removidos;
/// contador de boots persistente.
fn test_user_fs() -> TestResult {
    use crate::ipc::ChannelEnd;
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let (c, d) = ChannelEnd::create_pair();
    let driver = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .map_err(String::from)?;
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b), channel_handle(c)])
            .map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 9, alloc::vec![channel_handle(d)])
        .map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let fc = crate::process::wait_and_reap(&fs);
    let dc = crate::process::wait_and_reap(&driver);
    drop((driver, fs, client));
    sched::reap();
    check!(cc == 0, "cliente do fs saiu com {cc}");
    check!(fc == 0, "servidor fs saiu com {fc}");
    check!(dc == 0, "driver saiu com {dc}");
    let ends = crate::ipc::live_channel_ends();
    check!(
        ends == ends0,
        "extremidades de canal vazaram: {ends0} -> {ends}"
    );
    let frames = settled_free_frames(frames0, 4);
    check!(
        frames + 4 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    crate::irq::free(crate::irq::USER_VECTOR_BASE);
    Ok(())
}

/// devmgr com concessao raiz: enumera, deriva concessoes por dispositivo, inicia blockdev e
/// rngdev por IDs, sobe o fs e entrega os canais ao utest(11), que usa fs e rng.
fn test_user_devmgr() -> TestResult {
    use crate::ipc::{ChannelEnd, DeviceGrant, Handle, Object, Rights};
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let mut live0 = 0;
    crate::process::for_each_live(|_| live0 += 1);
    let (x, y) = ChannelEnd::create_pair();
    let root = Handle {
        object: Object::Device(Arc::new(DeviceGrant::all())),
        rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_ALL),
    };
    let devmgr = crate::process::spawn_named("devmgr", 0, alloc::vec![root, channel_handle(x)])
        .map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 11, alloc::vec![channel_handle(y)])
        .map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let mc = crate::process::wait_and_reap(&devmgr);
    drop((devmgr, client));
    check!(cc == 0, "cliente saiu com {cc}");
    check!(mc == 0, "devmgr saiu com {mc}");
    // Os drivers e o fs terminam sozinhos quando seus canais fecham; espera-os.
    let mut live = usize::MAX;
    for _ in 0..200 {
        sched::reap();
        live = 0;
        crate::process::for_each_live(|_| live += 1);
        if live <= live0 {
            break;
        }
        sched::sleep_ms(10);
    }
    sched::reap();
    check!(
        live <= live0,
        "processos ainda vivos: {live} (antes {live0})"
    );
    let ends = crate::ipc::live_channel_ends();
    check!(
        ends == ends0,
        "extremidades de canal vazaram: {ends0} -> {ends}"
    );
    let frames = settled_free_frames(frames0, 8);
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    for v in 0..4u8 {
        crate::irq::free(crate::irq::USER_VECTOR_BASE + v);
    }
    Ok(())
}

/// vfs com dois namespaces (completo e so /tmp) sobre fs + espfs; utest(12) verifica
/// roteamento, ramfs por instancia e isolamento entre namespaces.
fn test_user_vfs() -> TestResult {
    use crate::ipc::{ChannelEnd, DeviceGrant, Handle, Object, Rights};
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let mut blks: Vec<u16> = crate::pci::devices()
        .iter()
        .filter(|d| d.is_virtio() && (d.device == 0x1001 || d.device == 0x1042))
        .map(|d| d.bdf)
        .collect();
    blks.sort_unstable();
    if blks.len() < 2 {
        return Err(String::from("disco de boot virtio ausente"));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    // cadeia de dados: blockdev (primeiro disco) + fs
    let (a1, b1) = ChannelEnd::create_pair();
    let (c1, d1) = ChannelEnd::create_pair();
    let blk_data = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a1)],
    )
    .map_err(String::from)?;
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b1), channel_handle(c1)])
            .map_err(String::from)?;
    // cadeia de boot: blockdev restrito ao segundo disco + espfs
    let (a2, b2) = ChannelEnd::create_pair();
    let (c2, d2) = ChannelEnd::create_pair();
    let boot_grant = Handle {
        object: Object::Device(Arc::new(DeviceGrant::for_device(blks[1]))),
        rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
    };
    let blk_boot =
        crate::process::spawn_named("blockdev", 0, alloc::vec![boot_grant, channel_handle(a2)])
            .map_err(String::from)?;
    let espfs = crate::process::spawn_named(
        "espfs",
        0,
        alloc::vec![channel_handle(b2), channel_handle(c2)],
    )
    .map_err(String::from)?;
    // vfs completo (arg 0 = tudo) e vfs so /tmp (arg 4, com canais dummy)
    let (x1, y1) = ChannelEnd::create_pair();
    let vfs_full = crate::process::spawn_named(
        "vfs",
        0,
        alloc::vec![channel_handle(d1), channel_handle(d2), channel_handle(x1)],
    )
    .map_err(String::from)?;
    let (dm1, dm2) = ChannelEnd::create_pair();
    let (x2, y2) = ChannelEnd::create_pair();
    let vfs_tmp = crate::process::spawn_named(
        "vfs",
        4,
        alloc::vec![channel_handle(dm1), channel_handle(dm2), channel_handle(x2)],
    )
    .map_err(String::from)?;
    let client = crate::process::spawn_named(
        "utest",
        12,
        alloc::vec![channel_handle(y1), channel_handle(y2)],
    )
    .map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let v1 = crate::process::wait_and_reap(&vfs_full);
    let v2 = crate::process::wait_and_reap(&vfs_tmp);
    let fc = crate::process::wait_and_reap(&fs);
    let ec = crate::process::wait_and_reap(&espfs);
    let d1c = crate::process::wait_and_reap(&blk_data);
    let d2c = crate::process::wait_and_reap(&blk_boot);
    drop((client, vfs_full, vfs_tmp, fs, espfs, blk_data, blk_boot));
    sched::reap();
    check!(cc == 0, "cliente do vfs saiu com {cc}");
    check!(v1 == 0 && v2 == 0, "vfs saiu com {v1}/{v2}");
    check!(fc == 0 && ec == 0, "fs/espfs sairam com {fc}/{ec}");
    check!(d1c == 0 && d2c == 0, "blockdevs sairam com {d1c}/{d2c}");
    let ends = crate::ipc::live_channel_ends();
    check!(
        ends == ends0,
        "extremidades de canal vazaram: {ends0} -> {ends}"
    );
    let frames = settled_free_frames(frames0, 8);
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    for v in 0..4u8 {
        crate::irq::free(crate::irq::USER_VECTOR_BASE + v);
    }
    Ok(())
}

/// `shell=1` na linha de comando: sobe blockdev(dados) + fs + blockdev(boot) + espfs + vfs +
/// consoledev + shell e espera o shell terminar (`sair`).
pub fn shell_mode() {
    use crate::ipc::{ChannelEnd, DeviceGrant, Handle, Object, Rights};
    if !has_virtio_blk() {
        panic!("shell=1 exige o disco de dados (virtio-blk)");
    }
    let console_bdf = crate::pci::devices()
        .iter()
        .find(|d| d.is_virtio() && (d.device == 0x1043 || d.device == 0x1003))
        .map(|d| d.bdf);
    let Some(console_bdf) = console_bdf else {
        panic!("shell=1 exige virtio-console (rode com --console-socket)");
    };
    let mut blks: Vec<u16> = crate::pci::devices()
        .iter()
        .filter(|d| d.is_virtio() && (d.device == 0x1001 || d.device == 0x1042))
        .map(|d| d.bdf)
        .collect();
    blks.sort_unstable();
    let (a1, b1) = ChannelEnd::create_pair();
    let (c1, d1) = ChannelEnd::create_pair();
    let _blk = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a1)],
    )
    .expect("blockdev");
    let _fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b1), channel_handle(c1)])
            .expect("fs");
    // /boot se houver segundo disco; senao canais soltos (montagem responde NotFound ao uso)
    let (c2, d2) = ChannelEnd::create_pair();
    if blks.len() >= 2 {
        let (a2, b2) = ChannelEnd::create_pair();
        let g = Handle {
            object: Object::Device(Arc::new(DeviceGrant::for_device(blks[1]))),
            rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
        };
        let _bb = crate::process::spawn_named("blockdev", 0, alloc::vec![g, channel_handle(a2)])
            .expect("blockdev boot");
        let _es = crate::process::spawn_named(
            "espfs",
            0,
            alloc::vec![channel_handle(b2), channel_handle(c2)],
        )
        .expect("espfs");
    }
    let (x, y) = ChannelEnd::create_pair();
    let _vfs = crate::process::spawn_named(
        "vfs",
        0,
        alloc::vec![channel_handle(d1), channel_handle(d2), channel_handle(x)],
    )
    .expect("vfs");
    let (ca, cb) = ChannelEnd::create_pair();
    let cg = Handle {
        object: Object::Device(Arc::new(DeviceGrant::for_device(console_bdf))),
        rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
    };
    let _con = crate::process::spawn_named("consoledev", 0, alloc::vec![cg, channel_handle(ca)])
        .expect("consoledev");
    let shell = crate::process::spawn_named(
        "shell",
        0,
        alloc::vec![channel_handle(cb), channel_handle(y)],
    )
    .expect("shell");
    kinfo!("[SHELL] shell de diagnostico ativo na console VirtIO");
    let code = crate::process::wait_and_reap(&shell);
    kinfo!("[SHELL] shell terminou com {code}");
}

/// `input-test=1` na linha de comando: inputdev + utest(13) esperando teclas do host (QMP).
pub fn input_test_mode(variant: u64) {
    use crate::ipc::{ChannelEnd, DeviceGrant, Handle, Object, Rights};
    let grant = |bdf: u16| Handle {
        object: Object::Device(Arc::new(DeviceGrant::for_device(bdf))),
        rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
    };
    let inputs: Vec<u16> = crate::pci::devices()
        .iter()
        .filter(|d| d.is_virtio() && d.device == 0x1052)
        .map(|d| d.bdf)
        .collect();
    let bdf = *inputs.first().expect("input-test exige um virtio-input");
    if variant == 4 {
        // Entrada MESCLADA: dois virtio-input (teclado + tablet), um inputdev para cada,
        // ambos empurrando no MESMO canal (duplicado pelo assinante) para o wm.
        let bdf2 = *inputs.get(1).expect("input-test=4 exige dois virtio-input");
        let (a1, b1) = ChannelEnd::create_pair();
        let (a2, b2) = ChannelEnd::create_pair();
        let _d1 =
            crate::process::spawn_named("inputdev", 0, alloc::vec![grant(bdf), channel_handle(a1)])
                .expect("inputdev 1");
        let _d2 = crate::process::spawn_named(
            "inputdev",
            0,
            alloc::vec![grant(bdf2), channel_handle(a2)],
        )
        .expect("inputdev 2");
        let (wa, wb) = ChannelEnd::create_pair();
        let _wm =
            crate::process::spawn_named("wm", 0, alloc::vec![channel_handle(wa)]).expect("wm");
        let client = crate::process::spawn_named(
            "utest",
            55,
            alloc::vec![channel_handle(wb), channel_handle(b1), channel_handle(b2)],
        )
        .expect("utest");
        kinfo!("[INPUT] aguardando teclas injetadas pelo host");
        let code = crate::process::wait_and_reap(&client);
        kinfo!("[INPUT] teste de entrada terminou com {code}");
        return;
    }
    let (a, b) = ChannelEnd::create_pair();
    let g = grant(bdf);
    let _drv = crate::process::spawn_named("inputdev", 0, alloc::vec![g, channel_handle(a)])
        .expect("inputdev");
    let client = if variant == 3 {
        // Cadeia do PONTEIRO: inputdev(tablet) --subscribe{64x48}--> wm --pointer--> janela.
        let (wa, wb) = ChannelEnd::create_pair();
        let _wm =
            crate::process::spawn_named("wm", 0, alloc::vec![channel_handle(wa)]).expect("wm");
        crate::process::spawn_named(
            "utest",
            54,
            alloc::vec![channel_handle(wb), channel_handle(b)],
        )
        .expect("utest")
    } else if variant == 2 {
        // Cadeia completa: inputdev --subscribe--> canal --set_input--> wm --evento key--> janela.
        let (wa, wb) = ChannelEnd::create_pair();
        let _wm =
            crate::process::spawn_named("wm", 0, alloc::vec![channel_handle(wa)]).expect("wm");
        crate::process::spawn_named(
            "utest",
            31,
            alloc::vec![channel_handle(wb), channel_handle(b)],
        )
        .expect("utest")
    } else {
        crate::process::spawn_named("utest", 13, alloc::vec![channel_handle(b)]).expect("utest")
    };
    kinfo!("[INPUT] aguardando teclas injetadas pelo host");
    let _ = variant;
    let code = crate::process::wait_and_reap(&client);
    kinfo!("[INPUT] teste de entrada terminou com {code}");
}

/// `fuzz=<segundos>` na linha de comando: rodadas de fuzz de syscalls (utest modo 7) com
/// sementes derivadas do TSC (registradas no log para reproduzir), checando vazamentos de
/// quadros/canais a cada rodada, ate esgotar o orcamento de tempo.
pub fn fuzz_mode(secs: u64) -> bool {
    let deadline = crate::time::monotonic_ns() + secs.saturating_mul(1_000_000_000);
    let mut rounds = 0u64;
    let mut ok = true;
    kinfo!("[FUZZ] iniciando: {secs} s de fuzz de syscalls (20000 por rodada)");
    while crate::time::monotonic_ns() < deadline {
        rounds += 1;
        let seed = crate::time::monotonic_ns() | 1;
        let frames0 = phys::stats().free;
        let ends0 = crate::ipc::live_channel_ends();
        let arg = 7 | (seed << 8);
        let code = match crate::process::spawn_named("utest", arg, Vec::new()) {
            Ok(p) => crate::process::wait_and_reap(&p),
            Err(e) => {
                kerror!("[FUZZ] rodada {rounds}: spawn falhou: {e}");
                ok = false;
                break;
            }
        };
        sched::reap();
        let frames = settled_free_frames(frames0, 8);
        let ends = crate::ipc::live_channel_ends();
        if code != 0 || ends != ends0 || frames + 8 < frames0 {
            kerror!(
                "[FUZZ] FAIL rodada {rounds} semente {seed:#x}: codigo {code}, canais {ends0}->{ends}, quadros {frames0}->{frames}"
            );
            ok = false;
            break;
        }
        if rounds.is_multiple_of(10) {
            kinfo!("[FUZZ] {rounds} rodadas ({} syscalls)", rounds * 20_000);
        }
    }
    if ok {
        kinfo!("[FUZZ] PASS rodadas={rounds} syscalls={}", rounds * 20_000);
    }
    ok
}

/// Espera múltipla de canais exercitada de um processo de usuário (utest modo 16).
fn test_user_wait_any() -> TestResult {
    let code = run_utest(16)?;
    check!(code == 0, "wait_any saiu com {code}");
    Ok(())
}

/// `net-test=1` na linha de comando: netdev + utest(14) trocando ARP com o slirp do QEMU.
pub fn net_test_mode() {
    use crate::ipc::{ChannelEnd, DeviceGrant, Handle, Object, Rights};
    let bdf = crate::pci::devices()
        .iter()
        .find(|d| d.is_virtio() && (d.device == 0x1041 || d.device == 0x1000))
        .map(|d| d.bdf)
        .expect("net-test=1 exige virtio-net-pci (rode com --net)");
    let (a, b) = ChannelEnd::create_pair();
    let g = Handle {
        object: Object::Device(Arc::new(DeviceGrant::for_device(bdf))),
        rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
    };
    let _drv = crate::process::spawn_named("netdev", 0, alloc::vec![g, channel_handle(a)])
        .expect("netdev");
    let tcp_port = nexo_boot_abi::cmdline_value(crate::boot::cmdline(), "tcp-port")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let client = crate::process::spawn_named(
        "utest",
        14 | (tcp_port << 8),
        alloc::vec![channel_handle(b)],
    )
    .expect("utest");
    kinfo!("[NET] aguardando troca ARP com o gateway do slirp");
    let code = crate::process::wait_and_reap(&client);
    kinfo!("[NET] teste de rede terminou com {code}");
    // Fase 2: netd (servico residente) com a API de sockets.
    let udp_port = nexo_boot_abi::cmdline_value(crate::boot::cmdline(), "udp-port")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    if tcp_port == 0 || udp_port == 0 {
        return;
    }
    let http_port = nexo_boot_abi::cmdline_value(crate::boot::cmdline(), "http-port")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let (na, nb) = ChannelEnd::create_pair();
    let (sa, sb) = ChannelEnd::create_pair();
    let g2 = Handle {
        object: Object::Device(Arc::new(DeviceGrant::for_device(bdf))),
        rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
    };
    let _drv2 = crate::process::spawn_named("netdev", 0, alloc::vec![g2, channel_handle(na)])
        .expect("netdev 2");
    let _netd = crate::process::spawn_named(
        "netd",
        0,
        alloc::vec![channel_handle(nb), channel_handle(sa)],
    )
    .expect("netd");
    let client2 = crate::process::spawn_named(
        "utest",
        15 | (tcp_port << 8) | (udp_port << 24) | (http_port << 40),
        alloc::vec![channel_handle(sb)],
    )
    .expect("utest 15");
    kinfo!("[NET] fase 2: netd + API de sockets");
    let code = crate::process::wait_and_reap(&client2);
    kinfo!("[NET] fase 2 (netd) terminou com {code}");
}

/// `fs-churn=1` na linha de comando: blockdev + fs + utest(10) escrevendo sem parar, para o
/// cenario `powercut` (o host mata o QEMU no meio das escritas e verifica o volume no boot seguinte).
pub fn fs_churn() -> ! {
    use crate::ipc::ChannelEnd;
    if !has_virtio_blk() {
        panic!("fs-churn exige o disco de dados (virtio-blk)");
    }
    let (a, b) = ChannelEnd::create_pair();
    let (c, d) = ChannelEnd::create_pair();
    let driver = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .expect("blockdev");
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b), channel_handle(c)])
            .expect("fs");
    let client =
        crate::process::spawn_named("utest", 10, alloc::vec![channel_handle(d)]).expect("utest");
    kinfo!("[CHURN] blockdev, fs e utest(10) ativos; aguardando o corte de energia");
    let code = crate::process::wait_and_reap(&client);
    let _ = (driver, fs);
    panic!("fs-churn: o cliente terminou com {code} (deveria escrever ate o corte)");
}

/// Renderizador 2D (nexo-gfx) sobre uma superficie de rascunho no heap: fill, composicao alfa
/// e clipping, com leitura de volta — prova a biblioteca no ambiente no_std/alloc do kernel.
/// Memoria compartilhada entre dois processos: o produtor cria um objeto, escreve um marcador e
/// transfere o handle ao consumidor por um canal; o consumidor le o marcador e responde na mesma
/// memoria. Verifica ausencia de vazamento de quadros ao final (o objeto libera os frames).
fn test_user_shmem() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let ha = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hb = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let producer = crate::process::spawn_named("utest", 17, ha).map_err(String::from)?;
    let consumer = crate::process::spawn_named("utest", 18, hb).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&consumer);
    let pc = crate::process::wait_and_reap(&producer);
    drop((producer, consumer));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "consumidor saiu com {cc}");
    check!(pc == 0, "produtor saiu com {pc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram (memoria compartilhada nao liberada): {frames0} -> {frames}"
    );
    Ok(())
}

/// Composição fim a fim entre processos: o serviço `wm` cria superfícies em memória
/// compartilhada (o cliente escreve os pixels), compõe a cena com ordem-Z numa saída
/// compartilhada, e o cliente confere os pixels compostos. Também verifica que nenhum
/// quadro vaza quando ambos encerram (os `MemoryObject` são liberados no Drop).
fn test_user_wm() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 19, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    // o cliente encerrou e fechou seu lado do canal; o wm sai com 0 ao ver PeerClosed
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram (superficies/saida nao liberadas): {frames0} -> {frames}"
    );
    Ok(())
}

/// Compositor **multi-cliente**: a mesma instância do `wm` atende duas sessões independentes
/// (a segunda aberta por `open`, transferindo a ponta de um canal novo). Cada sessão cria a sua
/// superfície; o wm compõe ambas por z-order e recusa que uma sessão mexa na superfície da outra.
fn test_user_wm_multi() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 20, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram (superficies/saida nao liberadas): {frames0} -> {frames}"
    );
    Ok(())
}

/// Restacking de janelas: com duas superfícies sobrepostas, `raise`/`lower` reordenam o z e a
/// saída composta acompanha (o pixel da sobreposição muda de cor conforme quem está na frente).
fn test_user_wm_restack() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 21, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Redimensionamento de superfície: o compositor realoca o `MemoryObject` (desmapeando o antigo
/// via `memory_unmap`) e a área nova da superfície passa a aparecer na saída composta; nenhum
/// quadro vaza (o buffer antigo é liberado quando as duas pontas o fecham).
fn test_user_wm_resize() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 22, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram (buffer antigo nao liberado): {frames0} -> {frames}"
    );
    Ok(())
}

/// Foco por clique: com uma fonte de entrada (canal de eventos evdev sintéticos), um clique traz a
/// superfície sob o ponteiro para a frente — observável na saída composta.
fn test_user_wm_input() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 23, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Entrega de teclado à janela em foco: a superfície focada por clique recebe as teclas (EV_KEY)
/// como eventos `key` na sua sessão.
fn test_user_wm_keyboard() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 24, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Opacidade por superfície: uma janela translúcida (`set_alpha`) mistura sua cor com o que está
/// abaixo na saída composta.
fn test_user_wm_alpha() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 25, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Toolkit de UI pela pilha completa: um cliente desenha um botão temático (`nexo-ui`) na sua
/// superfície e o `wm` o compõe — a saída composta mostra o botão nas cores do tema.
fn test_user_wm_ui() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 26, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Maximizar/restaurar: uma superfície pequena maximiza (preenche a saída) e restaura (volta ao
/// retângulo anterior); a saída composta reflete cada passo. Exercita a realocação (com `munmap`).
fn test_user_wm_maximize() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 27, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Atalho global Meta+Tab cicla o foco: com duas janelas sobrepostas, o atalho traz a janela de
/// trás para a frente (observável na saída composta).
fn test_user_wm_shortcut() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 28, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Apresentação no framebuffer **real**: o `wm`, de posse da concessão do dispositivo de vídeo
/// (o framebuffer é um BAR), consulta o layout (`fb_info`), mapeia a tela (`mmio_map`) e copia a
/// saída composta para ela; o kernel lê os pixels do framebuffer físico e confere. O console
/// gráfico fica suspenso durante o teste (a serial segue). Pulado se não há framebuffer ou se ele
/// não está num BAR de um dispositivo de vídeo.
fn test_user_wm_present() -> TestResult {
    use crate::ipc::{ChannelEnd, DeviceGrant, Handle, Object, Rights};
    let fb = crate::boot::info().framebuffer;
    if !fb.is_present() || fb.bytes_per_pixel != 4 {
        kinfo!("selftest: sem framebuffer utilizavel; apresentacao pulada");
        return Ok(());
    }
    let fb_end = fb.base + (fb.stride as u64) * (fb.height as u64) * 4;
    let vga = crate::pci::devices()
        .iter()
        .find(|d| {
            d.class == 0x03
                && d.bars.iter().any(|b| {
                    b.size != 0
                        && b.flags & 1 == 0
                        && fb.base >= b.base
                        && fb_end <= b.base + b.size
                })
        })
        .map(|d| d.bdf);
    let Some(bdf) = vga else {
        kinfo!("selftest: framebuffer fora de BAR de video; apresentacao pulada");
        return Ok(());
    };
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![
        channel_handle(a),
        Handle {
            object: Object::Device(Arc::new(DeviceGrant::for_device(bdf))),
            rights: Rights(nexo_syscall_abi::RIGHTS_DEVICE_DEFAULT),
        },
    ];
    let hclient = alloc::vec![channel_handle(b)];
    // O compositor passa a ser o dono do framebuffer durante o teste (log só na serial).
    crate::klog::disable_console();
    let run = || -> Result<(i64, i64, bool), String> {
        let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
        let client = crate::process::spawn_named("utest", 29, hclient).map_err(String::from)?;
        // Espera o magenta (255,0,255 — mesmos bytes em RGBX e BGRX) no pixel (8,8) da tela.
        let px_addr = fb.base + (8 * fb.stride as u64 + 8) * 4;
        let p = virt::phys_to_virt(nexo_mm::PhysAddr::new(px_addr)).as_ptr::<u8>();
        let mut seen = false;
        for _ in 0..400 {
            // SAFETY: px_addr está dentro do framebuffer (validado contra o BAR); physmap o cobre.
            let (b0, b1, b2) = unsafe {
                (
                    p.read_volatile(),
                    p.add(1).read_volatile(),
                    p.add(2).read_volatile(),
                )
            };
            if (b0, b1, b2) == (255, 0, 255) {
                seen = true;
                break;
            }
            sched::sleep_ms(10);
        }
        let cc = crate::process::wait_and_reap(&client);
        let wc = crate::process::wait_and_reap(&wm);
        drop((wm, client));
        Ok((cc, wc, seen))
    };
    let result = run();
    crate::klog::enable_console();
    let (cc, wc, seen) = result?;
    let frames = settled_free_frames(frames0, 8);
    check!(seen, "o magenta composto nao apareceu no framebuffer");
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Mosaico: `tile` organiza as janelas numa grade que cobre a saída, sem realocar buffers (o
/// conteúdo é escalado na composição); a saída composta mostra as janelas lado a lado.
fn test_user_wm_tile() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 30, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Captura segura de entrada: com a captura em vigor, as teclas vão para a superfície capturada
/// (ignorando o foco) e os cliques são engolidos; `ungrab` restaura o comportamento normal.
fn test_user_wm_grab() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 32, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Múltiplos displays emulados: cada display compõe só as suas janelas; `move_to_display` troca a
/// janela de tela (as saídas são MemoryObjects independentes).
fn test_user_wm_displays() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 33, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Login/bloqueio: o `greeter` cria a tela de login, captura a entrada (a senha não pode ser
/// roubada), rejeita a senha errada mantendo o bloqueio e, na certa, devolve a entrada à sessão.
fn test_user_greeter() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let greeter = crate::process::spawn_named("greeter", 0, alloc::vec![channel_handle(pa)])
        .map_err(String::from)?;
    let driver = crate::process::spawn_named(
        "utest",
        34,
        alloc::vec![channel_handle(wb), channel_handle(pb)],
    )
    .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let gc = crate::process::wait_and_reap(&greeter);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, greeter, driver));
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(gc == 0, "greeter saiu com {gc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Contextos (protótipo): só as janelas do contexto ativo são compostas e recebem entrada; a
/// troca preserva o estado das ocultas e move o foco para a janela de maior z do novo contexto.
fn test_user_wm_context() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 35, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Clipboard mediado: só a sessão dona da entrada (janela focada/capturada) lê/escreve; o
/// conteúdo atravessa sessões pela mediação e o histórico é opt-in.
fn test_user_wm_clipboard() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 36, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Notificações + não-perturbe: um aviso desenha o banner de sobreposição (inclusive vindo de
/// sessão em segundo plano); DND descarta avisos e só o dono da entrada o controla.
fn test_user_wm_notify() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 37, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Drag-and-drop por grant: só a janela onde o usuário solta recebe os dados; soltar no vazio
/// descarta; quem não detém a entrada não inicia arrasto.
fn test_user_wm_dnd() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 38, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Arquitetura de leitor de tela: o compositor emite eventos semânticos (foco+título, aviso,
/// troca de contexto) num canal assinado por `a11y_subscribe`.
fn test_user_wm_a11y() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 39, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Mecanismo da Faixa de Atividades: a sessão shell (bootstrap) enumera janelas e ativa qualquer
/// uma (troca de contexto + frente + foco); sessões comuns são negadas.
fn test_user_wm_shell() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 40, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Escala fracionária por janela (200%%/150%% via composição, sem realocar buffer) e a
/// preferência de redução de movimento (escrita mediada pela entrada; leitura livre).
fn test_user_wm_scale() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 41, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Mecanismo da Central de Ações: o registro guarda as notificações recentes (inclusive sob DND);
/// o shell lista e limpa; sessões comuns são negadas.
fn test_user_wm_center() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(a),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let hclient = alloc::vec![Handle {
        object: Object::Channel(b),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 42, hclient).map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, client));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Faixa de Atividades de verdade: o `shellui` (sessão privilegiada) desenha a barra, faz broker
/// de sessões e, ao receber o clique na célula (evento `pointer`), **ativa** a janela.
fn test_user_shellui() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let shell = crate::process::spawn_named(
        "shellui",
        0,
        alloc::vec![channel_handle(wb), channel_handle(pa)],
    )
    .map_err(String::from)?;
    let driver = crate::process::spawn_named("utest", 43, alloc::vec![channel_handle(pb)])
        .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let sc = crate::process::wait_and_reap(&shell);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, shell, driver));
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(sc == 0, "shellui saiu com {sc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Central de Ações visual: o clique na zona direita da barra abre o painel do `shellui` com um
/// marcador por notificação registrada; o segundo clique o fecha.
fn test_user_shellcenter() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let shell = crate::process::spawn_named(
        "shellui",
        0,
        alloc::vec![channel_handle(wb), channel_handle(pa)],
    )
    .map_err(String::from)?;
    let driver = crate::process::spawn_named("utest", 44, alloc::vec![channel_handle(pb)])
        .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let sc = crate::process::wait_and_reap(&shell);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, shell, driver));
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(sc == 0, "shellui saiu com {sc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// O primeiro aplicativo real: a calculadora — botões `nexo-ui` acionados por eventos `pointer`,
/// resultado lido pelo clipboard mediado (1 + 2 = 3, tudo por cliques).
fn test_user_calc() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let calc = crate::process::spawn_named("calc", 0, alloc::vec![channel_handle(pa)])
        .map_err(String::from)?;
    let driver = crate::process::spawn_named(
        "utest",
        45,
        alloc::vec![channel_handle(wb), channel_handle(pb)],
    )
    .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let cc = crate::process::wait_and_reap(&calc);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, calc, driver));
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(cc == 0, "calc saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Instalação transacional de pacotes no NexoFS real: v1 → v2 com o ponteiro `.cur` gravado por
/// último; a v1 fica intacta; pacote corrompido não muda nada.
fn test_user_install() -> TestResult {
    use crate::ipc::ChannelEnd;
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (a, b) = ChannelEnd::create_pair();
    let (c, d) = ChannelEnd::create_pair();
    let driver = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .map_err(String::from)?;
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b), channel_handle(c)])
            .map_err(String::from)?;
    let client = crate::process::spawn_named("utest", 46, alloc::vec![channel_handle(d)])
        .map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let fc = crate::process::wait_and_reap(&fs);
    let dc = crate::process::wait_and_reap(&driver);
    drop((driver, fs, client));
    sched::reap();
    check!(cc == 0, "instalador saiu com {cc}");
    check!(fc == 0, "servidor fs saiu com {fc}");
    check!(dc == 0, "driver saiu com {dc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    let frames = settled_free_frames(frames0, 4);
    check!(
        frames + 4 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Execução a partir da memória: o kernel entrega o ELF do `echo` num `MemoryObject`; o cliente
/// o spawna com `process_spawn_mem`, conversa com o filho e o encerra limpo — o elo
/// "instalar → executar" da plataforma de aplicativos.
fn test_user_spawn_mem() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, MemoryObject, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let elf = crate::initrd::find("echo").ok_or("echo ausente do initrd")?;
    // copia o ELF para um MemoryObject (o transporte de bytes grandes para o usuário)
    let pages = (elf.len() as u64).div_ceil(nexo_mm::PAGE_SIZE);
    let mut frames = Vec::new();
    for i in 0..pages as usize {
        let fr = phys::allocate_zeroed_frame().ok_or("sem quadros para o ELF")?;
        let src = &elf[i * 4096..elf.len().min((i + 1) * 4096)];
        let dst = virt::phys_to_virt(fr).as_mut_ptr::<u8>();
        // SAFETY: quadro recém-alocado, exclusivo, mapeado no physmap.
        unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) };
        frames.push(fr);
    }
    let mem = Handle {
        object: Object::Memory(Arc::new(MemoryObject {
            frames,
            len: pages * nexo_mm::PAGE_SIZE,
        })),
        rights: Rights(nexo_syscall_abi::RIGHTS_MEMORY_DEFAULT),
    };
    let (a, b) = ChannelEnd::create_pair();
    let arg = 47u64 | ((elf.len() as u64) << 8);
    let client = crate::process::spawn_named("utest", arg, alloc::vec![channel_handle(b), mem])
        .map_err(String::from)?;
    let _keep = a; // a ponta do kernel só mantém o canal vivo durante o teste
    let cc = crate::process::wait_and_reap(&client);
    drop((client, _keep));
    let frames = settled_free_frames(frames0, 8);
    check!(cc == 0, "cliente saiu com {cc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// O laço completo da plataforma: empacotar → instalar (NexoFS) → ler o manifesto → conceder
/// capacidades **só pelas permissões declaradas** → executar da instalação (`spawn_mem`).
fn test_user_launcher() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, MemoryObject, Object, Rights};
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let elf = crate::initrd::find("echo").ok_or("echo ausente do initrd")?;
    let pages = (elf.len() as u64).div_ceil(nexo_mm::PAGE_SIZE);
    let mut frames = Vec::new();
    for i in 0..pages as usize {
        let fr = phys::allocate_zeroed_frame().ok_or("sem quadros para o ELF")?;
        let src = &elf[i * 4096..elf.len().min((i + 1) * 4096)];
        let dst = virt::phys_to_virt(fr).as_mut_ptr::<u8>();
        // SAFETY: quadro recém-alocado, exclusivo, mapeado no physmap.
        unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) };
        frames.push(fr);
    }
    let mem = Handle {
        object: Object::Memory(Arc::new(MemoryObject {
            frames,
            len: pages * nexo_mm::PAGE_SIZE,
        })),
        rights: Rights(nexo_syscall_abi::RIGHTS_MEMORY_DEFAULT),
    };
    let (a, b) = ChannelEnd::create_pair();
    let (c, d) = ChannelEnd::create_pair();
    let driver = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .map_err(String::from)?;
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b), channel_handle(c)])
            .map_err(String::from)?;
    let arg = 48u64 | ((elf.len() as u64) << 8);
    let client = crate::process::spawn_named("utest", arg, alloc::vec![channel_handle(d), mem])
        .map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let fc = crate::process::wait_and_reap(&fs);
    let dc = crate::process::wait_and_reap(&driver);
    drop((driver, fs, client));
    sched::reap();
    check!(cc == 0, "lancador saiu com {cc}");
    check!(fc == 0, "servidor fs saiu com {fc}");
    check!(dc == 0, "driver saiu com {dc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    let frames = settled_free_frames(frames0, 8);
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// App **gráfico** instalado: a calculadora real, empacotada e instalada no NexoFS, é lançada e
/// ganha uma sessão do compositor **só porque o manifesto declara `janelas`** (a janela "calc"
/// aparece); o mesmo binário sem a permissão nasce sem sessão.
fn test_user_launch_gui() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, MemoryObject, Object, Rights};
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let elf = crate::initrd::find("calc").ok_or("calc ausente do initrd")?;
    let pages = (elf.len() as u64).div_ceil(nexo_mm::PAGE_SIZE);
    let mut frames = Vec::new();
    for i in 0..pages as usize {
        let fr = phys::allocate_zeroed_frame().ok_or("sem quadros para o ELF")?;
        let src = &elf[i * 4096..elf.len().min((i + 1) * 4096)];
        let dst = virt::phys_to_virt(fr).as_mut_ptr::<u8>();
        // SAFETY: quadro recém-alocado, exclusivo, mapeado no physmap.
        unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) };
        frames.push(fr);
    }
    let mem = Handle {
        object: Object::Memory(Arc::new(MemoryObject {
            frames,
            len: pages * nexo_mm::PAGE_SIZE,
        })),
        rights: Rights(nexo_syscall_abi::RIGHTS_MEMORY_DEFAULT),
    };
    let (a, b) = ChannelEnd::create_pair();
    let (c, d) = ChannelEnd::create_pair();
    let (wa, wb) = ChannelEnd::create_pair();
    let driver = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .map_err(String::from)?;
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b), channel_handle(c)])
            .map_err(String::from)?;
    let wm = crate::process::spawn_named("wm", 0, alloc::vec![channel_handle(wa)])
        .map_err(String::from)?;
    let arg = 49u64 | ((elf.len() as u64) << 8);
    let client = crate::process::spawn_named(
        "utest",
        arg,
        alloc::vec![channel_handle(d), mem, channel_handle(wb)],
    )
    .map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let wc = crate::process::wait_and_reap(&wm);
    let fc = crate::process::wait_and_reap(&fs);
    let dc = crate::process::wait_and_reap(&driver);
    drop((driver, fs, wm, client));
    sched::reap();
    check!(cc == 0, "lancador saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    check!(fc == 0, "servidor fs saiu com {fc}");
    check!(dc == 0, "driver saiu com {dc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    let frames = settled_free_frames(frames0, 8);
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

fn test_user_consent() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, MemoryObject, Object, Rights};
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let elf = crate::initrd::find("calc").ok_or("calc ausente do initrd")?;
    let pages = (elf.len() as u64).div_ceil(nexo_mm::PAGE_SIZE);
    let mut frames = Vec::new();
    for i in 0..pages as usize {
        let fr = phys::allocate_zeroed_frame().ok_or("sem quadros para o ELF")?;
        let src = &elf[i * 4096..elf.len().min((i + 1) * 4096)];
        let dst = virt::phys_to_virt(fr).as_mut_ptr::<u8>();
        // SAFETY: quadro recém-alocado, exclusivo, mapeado no physmap.
        unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) };
        frames.push(fr);
    }
    let mem = Handle {
        object: Object::Memory(Arc::new(MemoryObject {
            frames,
            len: pages * nexo_mm::PAGE_SIZE,
        })),
        rights: Rights(nexo_syscall_abi::RIGHTS_MEMORY_DEFAULT),
    };
    let (a, b) = ChannelEnd::create_pair();
    let (c, d) = ChannelEnd::create_pair();
    let (wa, wb) = ChannelEnd::create_pair();
    let driver = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .map_err(String::from)?;
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b), channel_handle(c)])
            .map_err(String::from)?;
    let wm = crate::process::spawn_named("wm", 0, alloc::vec![channel_handle(wa)])
        .map_err(String::from)?;
    let (pa, pb) = ChannelEnd::create_pair();
    let lanc = crate::process::spawn_named("lanc", 0, alloc::vec![channel_handle(pa)])
        .map_err(String::from)?;
    let arg = 57u64 | ((elf.len() as u64) << 8);
    let client = crate::process::spawn_named(
        "utest",
        arg,
        alloc::vec![
            channel_handle(d),
            mem,
            channel_handle(wb),
            channel_handle(pb)
        ],
    )
    .map_err(String::from)?;
    let cc = crate::process::wait_and_reap(&client);
    let lc = crate::process::wait_and_reap(&lanc);
    let wc = crate::process::wait_and_reap(&wm);
    let fc = crate::process::wait_and_reap(&fs);
    let dc = crate::process::wait_and_reap(&driver);
    drop((driver, fs, wm, lanc, client));
    sched::reap();
    check!(cc == 0, "driver saiu com {cc}");
    check!(lc == 0, "lanc saiu com {lc}");
    check!(wc == 0, "wm saiu com {wc}");
    check!(fc == 0, "servidor fs saiu com {fc}");
    check!(dc == 0, "driver saiu com {dc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    let frames = settled_free_frames(frames0, 8);
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

/// Configurações: toggles clicáveis com efeito real — `prefs` reflete o movimento reduzido e o
/// não-perturbe suprime o banner de avisos.
fn test_user_config() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let config = crate::process::spawn_named("config", 0, alloc::vec![channel_handle(pa)])
        .map_err(String::from)?;
    let driver = crate::process::spawn_named(
        "utest",
        50,
        alloc::vec![channel_handle(wb), channel_handle(pb)],
    )
    .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let cc = crate::process::wait_and_reap(&config);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, config, driver));
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(cc == 0, "config saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

fn test_user_monitor() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let mon = crate::process::spawn_named("monitor", 0, alloc::vec![channel_handle(pa)])
        .map_err(String::from)?;
    let driver = crate::process::spawn_named(
        "utest",
        51,
        alloc::vec![channel_handle(wb), channel_handle(pb)],
    )
    .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let cc = crate::process::wait_and_reap(&mon);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, mon, driver));
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(cc == 0, "monitor saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

fn test_user_agenda() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let mon = crate::process::spawn_named("agenda", 0, alloc::vec![channel_handle(pa)])
        .map_err(String::from)?;
    let driver = crate::process::spawn_named(
        "utest",
        56,
        alloc::vec![channel_handle(wb), channel_handle(pb)],
    )
    .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let cc = crate::process::wait_and_reap(&mon);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, mon, driver));
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(cc == 0, "agenda saiu com {cc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

fn test_user_term() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let (ca, cb) = ChannelEnd::create_pair();
    let (va, vb) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let term = crate::process::spawn_named(
        "term",
        0,
        alloc::vec![channel_handle(pa), channel_handle(cb), channel_handle(vb)],
    )
    .map_err(String::from)?;
    let shell = crate::process::spawn_named(
        "shell",
        0,
        alloc::vec![channel_handle(ca), channel_handle(va)],
    )
    .map_err(String::from)?;
    let driver = crate::process::spawn_named(
        "utest",
        52,
        alloc::vec![channel_handle(wb), channel_handle(pb)],
    )
    .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let sc = crate::process::wait_and_reap(&shell);
    let tc = crate::process::wait_and_reap(&term);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, term, shell, driver));
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(sc == 0, "shell saiu com {sc}");
    check!(tc == 0, "term saiu com {tc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

fn test_user_visor() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let (a, b) = ChannelEnd::create_pair();
    let (c, d) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let blk = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .map_err(String::from)?;
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b), channel_handle(c)])
            .map_err(String::from)?;
    let visor = crate::process::spawn_named("visor", 0, alloc::vec![channel_handle(pa)])
        .map_err(String::from)?;
    let driver = crate::process::spawn_named(
        "utest",
        53,
        alloc::vec![channel_handle(wb), channel_handle(pb), channel_handle(d)],
    )
    .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let vc = crate::process::wait_and_reap(&visor);
    let fc = crate::process::wait_and_reap(&fs);
    let bc = crate::process::wait_and_reap(&blk);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, blk, fs, visor, driver));
    sched::reap();
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(vc == 0, "visor saiu com {vc}");
    check!(fc == 0, "servidor fs saiu com {fc}");
    check!(bc == 0, "blockdev saiu com {bc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

fn test_user_editor() -> TestResult {
    use crate::ipc::{ChannelEnd, Handle, Object, Rights};
    if !has_virtio_blk() {
        return Err(String::from(
            "virtio-blk ausente (rode com o disco de dados)",
        ));
    }
    let ends0 = crate::ipc::live_channel_ends();
    let frames0 = phys::stats().free;
    let (wa, wb) = ChannelEnd::create_pair();
    let (pa, pb) = ChannelEnd::create_pair();
    let (a, b) = ChannelEnd::create_pair();
    let (c, d) = ChannelEnd::create_pair();
    let hserver = alloc::vec![Handle {
        object: Object::Channel(wa),
        rights: Rights(nexo_syscall_abi::RIGHTS_CHANNEL_DEFAULT),
    }];
    let wm = crate::process::spawn_named("wm", 0, hserver).map_err(String::from)?;
    let blk = crate::process::spawn_named(
        "blockdev",
        0,
        alloc::vec![device_handle(), channel_handle(a)],
    )
    .map_err(String::from)?;
    let fs =
        crate::process::spawn_named("fs", 0, alloc::vec![channel_handle(b), channel_handle(c)])
            .map_err(String::from)?;
    let editor = crate::process::spawn_named("editor", 0, alloc::vec![channel_handle(pa)])
        .map_err(String::from)?;
    let driver = crate::process::spawn_named(
        "utest",
        58,
        alloc::vec![channel_handle(wb), channel_handle(pb), channel_handle(d)],
    )
    .map_err(String::from)?;
    let dc = crate::process::wait_and_reap(&driver);
    let vc = crate::process::wait_and_reap(&editor);
    let fc = crate::process::wait_and_reap(&fs);
    let bc = crate::process::wait_and_reap(&blk);
    let wc = crate::process::wait_and_reap(&wm);
    drop((wm, blk, fs, editor, driver));
    sched::reap();
    let frames = settled_free_frames(frames0, 8);
    check!(dc == 0, "driver saiu com {dc}");
    check!(vc == 0, "editor saiu com {vc}");
    check!(fc == 0, "servidor fs saiu com {fc}");
    check!(bc == 0, "blockdev saiu com {bc}");
    check!(wc == 0, "wm saiu com {wc}");
    let ends = crate::ipc::live_channel_ends();
    check!(ends == ends0, "canais vazaram: {ends0} -> {ends}");
    check!(
        frames + 8 >= frames0,
        "quadros vazaram: {frames0} -> {frames}"
    );
    Ok(())
}

fn test_gfx() -> TestResult {
    use nexo_boot_abi::PixelFormat;
    use nexo_gfx::{Color, Rect, Surface};
    let mut buf = alloc::vec![0u8; 16 * 16 * 4];
    let mut s = Surface::new(&mut buf, 16, 16, 16, PixelFormat::Bgrx8888)
        .ok_or_else(|| String::from("superficie invalida"))?;
    s.clear(Color::rgb(0, 0, 0));
    s.fill_rect(Rect::new(2, 2, 8, 8), Color::rgb(200, 100, 50));
    check!(
        s.get(1, 1) == Color::rgb(0, 0, 0),
        "fundo alterado fora do rect"
    );
    check!(
        s.get(3, 3) == Color::rgb(200, 100, 50),
        "rect nao preenchido"
    );
    // composicao alfa: 50% branco sobre a cor solida
    s.blend(3, 3, Color::rgba(255, 255, 255, 128));
    let c = s.get(3, 3);
    check!((c.r as i32 - 227).abs() <= 2, "alfa incorreto: r={}", c.r);
    // clipping constringe o desenho
    s.set_clip(Rect::new(5, 5, 3, 3));
    s.fill_rect(Rect::new(0, 0, 16, 16), Color::WHITE);
    check!(
        s.get(0, 0) == Color::rgb(0, 0, 0),
        "clip nao respeitado (fora)"
    );
    check!(s.get(6, 6) == Color::WHITE, "clip nao respeitado (dentro)");
    // rasterizacao de texto: um glifo acende pixels
    s.reset_clip();
    s.clear(Color::BLACK);
    nexo_gfx::text::draw_glyph(&mut s, 'A', 0, 0, 1, Color::WHITE, None);
    let lit = (0..8)
        .flat_map(|y| (0..8i32).map(move |x| (x, y)))
        .filter(|&(x, y)| s.get(x, y) == Color::WHITE)
        .count();
    check!(lit > 0, "texto nao desenhou");
    check!(
        nexo_gfx::text::text_width("ok", 2) == 32,
        "largura de texto incorreta"
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
