TARGET=release
LIBSODIUM_RELEASE=1.0.11
BASEDIR:=$(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))

export SODIUM_LIB_DIR := $(BASEDIR)/local/lib/
export SODIUM_STATIC := yes

release:
	rm -rf libsodium/ local/ target/ # clean out who-knows-what
	$(MAKE) release*

libsodium:
	[ -d libsodium ] || git clone https://github.com/jedisct1/libsodium libsodium
	set -ex && cd libsodium && \
		git fetch && \
		git checkout $(LIBSODIUM_RELEASE) && \
		rm -rf lib && \
		./autogen.sh && \
		CC=musl-gcc ./configure --prefix=$$PWD/../local/ --disable-shared && \
		$(MAKE) && \
		$(MAKE) install

release*: libsodium
	cargo build --target=x86_64-unknown-linux-musl --release
	strip target/x86_64-unknown-linux-musl/release/turboshell
	mv target/x86_64-unknown-linux-musl/release/turboshell target/tsh
	@echo -----------------------------------------------------------------
	@echo && echo
	@target/tsh --version

clean:
	rm -rf target/

fullclean: clean
	rm -rf libsodium/ local/
