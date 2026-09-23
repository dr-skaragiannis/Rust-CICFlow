.PHONY: all build release test bench clean fmt check run-sample experiment

all: build test

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

bench:
	cargo bench

check:
	cargo check
	cargo test

fmt:
	cargo fmt --all

clean:
	cargo clean

run-sample: release
	./target/release/cicflowmeter -r tests/data/sample_traffic.pcap -o ./sample_output --format csv
	@echo "Output generated in ./sample_output/"

experiment: release
	python scripts/run_experiments.py $(EXPERIMENT_ARGS)
	@echo "Experiment results written under ./experiments/"
