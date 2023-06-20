build:
	cargo build --release

install:
	cp -i target/release/dash /usr/bin/dash
