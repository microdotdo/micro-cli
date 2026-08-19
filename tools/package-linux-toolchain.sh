#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 5 ]]; then
    printf 'usage: %s <micro> <micro-runner> <ablac> <abla-root> <output.tar.gz>\n' "$0" >&2
    exit 2
fi

micro_binary=$(realpath "$1")
runner_binary=$(realpath "$2")
compiler_binary=$(realpath "$3")
abla_root=$(realpath "$4")
output=$(realpath -m "$5")

for executable in "$micro_binary" "$runner_binary" "$compiler_binary"; do
    [[ -x $executable ]] || {
        printf 'missing executable: %s\n' "$executable" >&2
        exit 1
    }
done
for directory in "$abla_root/runtime" "$abla_root/stdlib"; do
    [[ -d $directory ]] || {
        printf 'missing Abla sysroot directory: %s\n' "$directory" >&2
        exit 1
    }
done
for tool in clang llc opt patchelf wasm-ld wasm-opt; do
    command -v "$tool" >/dev/null 2>&1 || {
        printf 'missing packaging tool: %s\n' "$tool" >&2
        exit 1
    }
done

working=$(mktemp -d)
trap 'rm -rf -- "$working"' EXIT
bundle="$working/micro-toolchain-linux-x86_64"
mkdir -p "$bundle/bin" "$bundle/lib" "$bundle/libexec" \
    "$bundle/share/abla" "$bundle/share/licenses"

cp "$micro_binary" "$bundle/bin/micro"
cp "$runner_binary" "$bundle/bin/micro-runner"
cp "$compiler_binary" "$bundle/libexec/ablac.bin"
cp "$(realpath "$(command -v wasm-ld)")" "$bundle/libexec/lld"
cp "$(realpath "$(command -v wasm-opt)")" "$bundle/libexec/wasm-opt"
cp "$(realpath "$(command -v opt)")" "$bundle/libexec/opt"
cp "$(realpath "$(command -v llc)")" "$bundle/libexec/llc"
cp "$(realpath "$(command -v clang)")" "$bundle/libexec/clang"
ln -s ../libexec/lld "$bundle/bin/wasm-ld"
ln -s ../libexec/wasm-opt "$bundle/bin/wasm-opt"
ln -s ../libexec/opt "$bundle/bin/opt"
ln -s ../libexec/llc "$bundle/bin/llc"
ln -s ../libexec/clang "$bundle/bin/clang"
cp -R "$abla_root/runtime" "$abla_root/stdlib" "$bundle/share/abla/"
cp "$(dirname "$0")/../LICENSE" "$bundle/share/licenses/micro-cli.txt"
if [[ -r $abla_root/LICENSE ]]; then
    abla_license="$abla_root/LICENSE"
elif [[ -r $abla_root/../licenses/abla.txt ]]; then
    abla_license="$abla_root/../licenses/abla.txt"
else
    printf 'missing Abla license beside sysroot: %s\n' "$abla_root" >&2
    exit 1
fi
cp "$abla_license" "$bundle/share/licenses/abla.txt"
chmod 0755 "$bundle/bin/micro" "$bundle/bin/micro-runner" \
    "$bundle/libexec/ablac.bin" "$bundle/libexec/lld"
chmod 0755 "$bundle/libexec/wasm-opt" "$bundle/libexec/opt" \
    "$bundle/libexec/llc" "$bundle/libexec/clang"

cat > "$bundle/bin/ablac" <<'SH'
#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export ABLA_SYSROOT="$root/share/abla"
export PATH="$root/bin:$PATH"
exec "$root/libexec/ablac.bin" "$@"
SH
chmod 0755 "$bundle/bin/ablac"

copy_dependencies() {
    local executable=$1
    local dependency
    local destination
    while IFS= read -r dependency; do
        case $(basename "$dependency") in
            ld-linux-*.so.*|libc.so.*|libm.so.*|libpthread.so.*|libdl.so.*|librt.so.*)
                continue
                ;;
        esac
        destination="$bundle/lib/$(basename "$dependency")"
        if [[ ! -e $destination ]]; then
            cp -L "$dependency" "$destination"
            chmod u+w "$destination"
        fi
    done < <(ldd "$executable" | awk '/=> \/[^ ]+/ { print $3 }')
}

copy_dependencies "$bundle/bin/micro"
copy_dependencies "$bundle/bin/micro-runner"
copy_dependencies "$bundle/libexec/ablac.bin"
copy_dependencies "$bundle/libexec/lld"
copy_dependencies "$bundle/libexec/wasm-opt"
copy_dependencies "$bundle/libexec/opt"
copy_dependencies "$bundle/libexec/llc"
copy_dependencies "$bundle/libexec/clang"

for executable in "$bundle/bin/micro" "$bundle/bin/micro-runner" \
    "$bundle/libexec/ablac.bin" "$bundle/libexec/lld" \
    "$bundle/libexec/wasm-opt" "$bundle/libexec/opt" \
    "$bundle/libexec/llc" "$bundle/libexec/clang"; do
    if patchelf --print-interpreter "$executable" >/dev/null 2>&1; then
        patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 "$executable"
        patchelf --set-rpath '$ORIGIN/../lib' "$executable"
    fi
done
for library in "$bundle"/lib/*.so*; do
    patchelf --set-rpath '$ORIGIN' "$library"
done

# Preserve the distributor copyright files for every system component carried
# in the archive. Missing entries are ignored outside Debian-based packaging
# environments; the release job asserts these files by building on Jammy.
for package in binaryen clang-21 libbsd0 libedit2 libffi8 libgcc-s1 \
    libicu70 libllvm21 liblzma5 libmd0 libssl3 libstdc++6 libtinfo6 \
    libxml2 libzstd1 zlib1g; do
    copyright="/usr/share/doc/$package/copyright"
    if [[ -r $copyright ]]; then
        cp "$copyright" "$bundle/share/licenses/$package.txt"
    fi
done

mkdir -p "$(dirname "$output")"
tar -C "$working" -czf "$output" micro-toolchain-linux-x86_64
(
    cd "$(dirname "$output")"
    sha256sum "$(basename "$output")" > "$(basename "$output").sha256"
)
