//! `init` — primeiro processo. Inicia o `svcmgr`, espera-o terminar e propaga o resultado.
#![no_std]
#![no_main]

use nexo_rt::log;

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    log!("init: pid {} iniciando svcmgr", nexo_sys::get_pid());
    let svc = match nexo_sys::process_spawn("svcmgr", 0, &[]) {
        Ok(h) => h,
        Err(e) => {
            log!("init: falha ao iniciar svcmgr: {:?}", e);
            nexo_sys::exit(30)
        }
    };
    match nexo_sys::process_wait(svc) {
        Ok(code) => {
            log!("init: svcmgr terminou com {} (reinicios feitos)", code);
            // svcmgr devolve o número de reinícios; o cenário exige pelo menos um.
            nexo_sys::exit(if code >= 1 { 0 } else { 31 })
        }
        Err(e) => {
            log!("init: wait falhou: {:?}", e);
            nexo_sys::exit(32)
        }
    }
}
