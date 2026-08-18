PROJECT_DIR := $(abspath .)
ABLAC_DIR ?= $(abspath ../ablac)
COMPILER ?= $(ABLAC_DIR)/build/ablac
BUILD_DIR ?= $(PROJECT_DIR)/build
RUN_TEST := $(if $(ABLA_TEST_LD_LIBRARY_PATH),env LD_LIBRARY_PATH=$(ABLA_TEST_LD_LIBRARY_PATH),)

.PHONY: all build test install clean

all: build

build:
	mkdir -p $(BUILD_DIR)
	cd $(PROJECT_DIR) && $(COMPILER) build $(PROJECT_DIR)/src/main.ab -o $(BUILD_DIR)/micro --no-cache

test:
	mkdir -p $(BUILD_DIR)
	cd $(PROJECT_DIR) && $(COMPILER) build $(PROJECT_DIR)/tests/core_test.ab -o $(BUILD_DIR)/core-test --no-cache
	cd $(PROJECT_DIR) && $(RUN_TEST) $(BUILD_DIR)/core-test

install: build
	install -Dm755 $(BUILD_DIR)/micro $(HOME)/.local/bin/micro

clean:
	@if [ -d "$(BUILD_DIR)" ]; then gio trash "$(BUILD_DIR)"; fi
