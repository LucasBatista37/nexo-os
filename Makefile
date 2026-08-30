# Nexo OS — comandos únicos (Plano Mestre §8). Tudo delega para tools/ (Python 3) e cargo.
#   make image      -> build/nexo.img (loader + kernel + ESP FAT32 + GPT)
#   make run        -> inicia a imagem no QEMU com display e serial no terminal
#   make test       -> testes de host + cenários em QEMU headless (o que o CI executa)
#   make ci         -> lint + test + verificação de reprodutibilidade

.PHONY: all image run run-debug test test-host test-qemu lint fmt ci check-toolchain reproducible clean stress

# Stress prolongado (gate F1: 24 h = DURATION=86400). Log em build/logs/stress.log.
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
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy --workspace --lib --target x86_64-unknown-none -- -D warnings
	cd kernel && cargo clippy --release -- -D warnings
	cd boot/loader && cargo clippy --release -- -D warnings

fmt:
	cargo fmt --all
	cd kernel && cargo fmt
	cd boot/loader && cargo fmt

reproducible:
	tools/build-image --cmdline "" --out build/repro-a.img | tee build/repro-a.txt
	tools/build-image --cmdline "" --out build/repro-b.img | tee build/repro-b.txt
	@cmp build/repro-a.img build/repro-b.img && echo "[nexo] imagem reproduzivel: OK" || (echo "[nexo] imagem NAO reproduzivel; primeiras diferencas (offset, a, b):"; cmp -l build/repro-a.img build/repro-b.img | head -20; exit 1)

ci: lint test-host image test-qemu reproducible

stress: image
	tools/build-image --no-build --cmdline "selftest=0 stress=$(DURATION) exit" --out build/nexo-stress-long.img
	mkdir -p build/logs
	NEXO_SMP=$(SMP) tools/run-qemu --test --image build/nexo-stress-long.img --timeout $$(( $(DURATION) + 300 )) --log build/logs/stress.log

clean:
	rm -rf build target kernel/target boot/loader/target
