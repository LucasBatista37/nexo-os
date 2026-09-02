# Nexo OS — comandos únicos (Plano Mestre §8). Tudo delega para tools/ (Python 3) e cargo.
#   make image      -> build/nexo.img (loader + kernel + ESP FAT32 + GPT)
#   make run        -> inicia a imagem no QEMU com display e serial no terminal
#   make test       -> testes de host + cenários em QEMU headless (o que o CI executa)
#   make ci         -> lint + test + verificação de reprodutibilidade

.PHONY: all image run run-debug test test-host test-qemu lint fmt ci check-toolchain reproducible clean stress fuzz netcap roadmap idl idl-check

# Stress prolongado (gate F1: 24 h = DURATION=86400). Log em build/logs/stress.log.
# Margem +900s +1% da duracao: o relogio do guest (TCG) atrasa em relacao a parede sob
# carga do host (~1-2 min/24h ocioso; mais com QEMUs concorrentes — 300s fixos quase
# mataram um gate de 24h; para 7 dias a folga precisa escalar com a duracao).
DURATION ?= 600
SMP ?= 4

all: image

check-toolchain:
	tools/check-toolchain

image:
	tools/build-image --cmdline ""

run: image
	tools/run-qemu

run-debug: image
	tools/run-qemu --gdb

test: test-host test-qemu

test-host:
	cargo test --workspace

test-qemu:
	tools/test-qemu

lint:
	cargo fmt --all --check
	cd kernel && cargo fmt --check
	cd boot/loader && cargo fmt --check
	cd services && cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy --workspace --lib --target x86_64-unknown-none -- -D warnings
	cd kernel && cargo clippy --release -- -D warnings
	cd boot/loader && cargo clippy --release -- -D warnings
	cd services && cargo clippy --workspace --release -- -D warnings
	tools/nexo-unsafe-audit

fmt:
	cargo fmt --all
	cd kernel && cargo fmt
	cd boot/loader && cargo fmt
	cd services && cargo fmt --all

reproducible:
	tools/build-image --cmdline "" --out build/repro-a.img | tee build/repro-a.txt
	tools/build-image --cmdline "" --out build/repro-b.img | tee build/repro-b.txt
	@cmp build/repro-a.img build/repro-b.img && echo "[nexo] imagem reproduzivel: OK" || (echo "[nexo] imagem NAO reproduzivel; primeiras diferencas (offset, a, b):"; cmp -l build/repro-a.img build/repro-b.img | head -20; exit 1)

ci: lint idl-check test-host image test-qemu reproducible

stress: image
	tools/build-image --no-build --cmdline "selftest=0 stress=$(DURATION) exit" --out build/nexo-stress-long.img
	mkdir -p build/logs
	NEXO_SMP=$(SMP) tools/run-qemu --test --image build/nexo-stress-long.img --disk build/nexo-stresslong-data.img --timeout $$(( $(DURATION) + 900 + $(DURATION) / 100 )) --log build/logs/stress-long.log; \
	rc=$$?; if [ $$rc -eq 33 ]; then echo "[nexo] stress de $(DURATION)s: PASS"; else echo "[nexo] stress: FALHA (codigo $$rc)"; exit 1; fi

# Fuzz de syscalls com sementes aleatorias (derivadas do TSC, registradas no log) por
# DURATION segundos; usado pelo workflow semanal .github/workflows/fuzz.yml.
fuzz: image
	tools/build-image --no-build --cmdline "selftest=0 fuzz=$(DURATION) exit" --out build/nexo-fuzz.img
	mkdir -p build/logs
	NEXO_SMP=$(SMP) tools/run-qemu --test --image build/nexo-fuzz.img --disk build/nexo-fuzz-data.img --timeout $$(( $(DURATION) + 600 )) --log build/logs/fuzz.log; \
	rc=$$?; if [ $$rc -eq 33 ]; then echo "[nexo] fuzz de $(DURATION)s: PASS"; else echo "[nexo] fuzz: FALHA (codigo $$rc)"; exit 1; fi

# Captura de rede autorizada para diagnostico: sobe o Nexo com slirp, grava um pcap de TODOS
# os pacotes da interface e imprime um resumo por protocolo/fluxo (tools/netcap).
netcap:
	tools/netcap

# Regenera os protocolos tipados a partir de idl/*.idl (abi/proto/src/generated) e formata
# (a saida do gerador so e estavel depois do rustfmt).
idl:
	tools/idlgen
	cargo fmt -p nexo-proto

# Falha se os modulos gerados estiverem defasados em relacao a IDL (usado no CI).
idl-check: idl
	git diff --exit-code -- abi/proto/src/generated || (echo "erro: rode 'make idl' e commite abi/proto/src/generated"; exit 1)

roadmap:
	tools/roadmap-status

clean:
	rm -rf build target kernel/target boot/loader/target services/target
