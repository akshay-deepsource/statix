build:
	apt-get update
	apt-get install -y build-essential curl
	curl https://sh.rustup.rs -sSf | bash -s -- -y
	PATH="/root/.cargo/bin:${PATH}"
	cargo build --release
	cp ./target/release/statix /app/statix
