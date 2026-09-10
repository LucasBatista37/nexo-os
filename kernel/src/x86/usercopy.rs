//! Cópias entre kernel e usuário à prova de corrida (tabela de *fixup*).
//!
//! Validar o ponteiro e **depois** copiar não é atômico. Desde que um processo pode ter
//! várias threads (que compartilham o espaço de endereçamento), outra thread do mesmo
//! processo pode chamar `memory_unmap` na janela entre a validação e a cópia —
//! [`crate::process::Space::unmap_user_shared`] limpa as PTEs e ainda faz *shootdown* de TLB
//! nas outras CPUs, exatamente para que a mudança valha em todas elas. O kernel tocaria então
//! uma página ausente e morreria num `#PF` fatal: um pânico de kernel ao alcance de qualquer
//! processo sem privilégio.
//!
//! A resposta clássica é a *exception table*: a instrução que copia é conhecida por endereço;
//! quando a falta acontece nela, o handler de `#PF` desvia o RIP para um rótulo de retomada
//! que devolve "faltou" em vez de derrubar a máquina. A tabela aqui tem uma entrada só —
//! todas as cópias usuário↔kernel passam por `nexo_copy_user`.
//!
//! A validação prévia continua existindo e continua sendo o caminho normal: ela dá o erro
//! limpo (`BadAddress`) para o ponteiro errado de sempre. O fixup cobre só a corrida.

use core::arch::global_asm;
use core::sync::atomic::{AtomicU64, Ordering};

global_asm!(
    ".global nexo_copy_user",
    ".global nexo_copy_user_inicio",
    ".global nexo_copy_user_fim",
    ".global nexo_copy_user_retomada",
    // rdi = destino, rsi = origem, rdx = bytes. Devolve 0 se copiou tudo, 1 se faltou página.
    "nexo_copy_user:",
    "    mov rcx, rdx",
    "    xor eax, eax",
    "    cld",
    // Só esta instrução é protegida: uma falta aqui volta pela retomada, e não pelo caminho
    // fatal. `rep movsb` é reinicializável, mas não retomamos a cópia: o buffer do usuário
    // saiu de baixo dos nossos pés e a syscall inteira falha.
    "nexo_copy_user_inicio:",
    "    rep movsb",
    "nexo_copy_user_fim:",
    "    ret",
    "nexo_copy_user_retomada:",
    "    mov eax, 1",
    "    ret",
);

// SAFETY: símbolos definidos pelo `global_asm!` logo acima, nesta mesma unidade de compilação;
// as assinaturas correspondem ao que o código em assembly faz (System V: rdi, rsi, rdx → rax).
unsafe extern "C" {
    fn nexo_copy_user(destino: *mut u8, origem: *const u8, bytes: usize) -> u64;
    // Rótulos: declarados como funções só para obter seus endereços sem `static` externo.
    fn nexo_copy_user_inicio();
    fn nexo_copy_user_fim();
    fn nexo_copy_user_retomada();
}

/// Faltas recuperadas pelo fixup desde o boot (corridas de desmapeamento vencidas).
static FALTAS: AtomicU64 = AtomicU64::new(0);

/// Endereços dos rótulos: início e fim da instrução protegida, e a retomada.
fn faixa() -> (u64, u64, u64) {
    (
        nexo_copy_user_inicio as *const () as usize as u64,
        nexo_copy_user_fim as *const () as usize as u64,
        nexo_copy_user_retomada as *const () as usize as u64,
    )
}

/// Endereço de retomada, se `rip` está dentro da cópia protegida.
///
/// Consultado pelo handler de `#PF` **antes** do caminho fatal, e só para faltas ocorridas em
/// modo kernel: em modo usuário nenhum RIP pode cair nesta faixa.
pub fn retomada_para(rip: u64) -> Option<u64> {
    let (inicio, fim, retomada) = faixa();
    if rip >= inicio && rip < fim {
        FALTAS.fetch_add(1, Ordering::Relaxed);
        Some(retomada)
    } else {
        None
    }
}

/// Quantas faltas de página em cópias usuário↔kernel já foram recuperadas.
///
/// Zero é o esperado: um número diferente de zero significa que alguém desmapeou um buffer
/// enquanto o kernel o copiava. Exposto ao usuário por `debug_info 9` — é o que permite ao
/// auto-teste **provar** que a corrida aconteceu e foi vencida, em vez de só não quebrar.
pub fn faltas() -> u64 {
    FALTAS.load(Ordering::Relaxed)
}

/// Copia `bytes` de `origem` para `destino` tolerando falta de página.
///
/// Devolve `true` se copiou tudo e `false` se a faixa deixou de estar mapeada no meio do
/// caminho. Em caso de falta o destino pode ter sido parcialmente escrito — quem chama
/// descarta o resultado (a syscall falha com `BadAddress`).
///
/// Um dos lados é sempre memória do kernel; o outro é do usuário e já passou pela validação
/// de faixa e de bits da página.
pub fn copiar(destino: *mut u8, origem: *const u8, bytes: usize) -> bool {
    if bytes == 0 {
        return true;
    }
    // SAFETY: a rotina só executa `rep movsb` entre dois ponteiros fornecidos por quem chama;
    // uma falta de página dentro dela é desviada pelo handler de `#PF` para a retomada, que
    // devolve 1 sem tocar em mais nada. Não há outra memória envolvida.
    unsafe { nexo_copy_user(destino, origem, bytes) == 0 }
}
