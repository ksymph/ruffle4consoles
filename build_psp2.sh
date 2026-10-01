# Wrapped Allocators seem to cause issues with touch and don't seem to help with the OOMs 
#export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,--wrap=malloc"
#export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,--wrap=free"
#export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,--wrap=calloc"
#export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,--wrap=realloc"
#export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,--wrap=memalign"
#export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,--wrap=memcpy"
#export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,--wrap=memset"
#export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,-q"
export RUSTFLAGS="$RUSTFLAGS -C target-cpu=cortex-a9"
cargo +nightly vita build vpk --profile=vita

# We keep the debug .elf (with line tables) for crash symbolization, but
# vita-elf-create embeds those debug sections into the packaged eboot, which
# bloats it (and the vpk) by several times. Rebuild a stripped velf/self from
# the debug .elf and repack the vpk so the installed eboot stays small.
VITA_DIR="target/armv7-sony-vita-newlibeabihf/vita"
VSDK="${VITASDK:-/usr/local/vitasdk}/bin"
"$VSDK/vita-elf-create" -s "$VITA_DIR/ruffle4consoles.elf" "$VITA_DIR/ruffle4consoles.velf"
"$VSDK/vita-make-fself" -s "$VITA_DIR/ruffle4consoles.velf" "$VITA_DIR/ruffle4consoles.self"
"$VSDK/vita-pack-vpk" -s "$VITA_DIR/ruffle4consoles.sfo" -b "$VITA_DIR/ruffle4consoles.self" \
    --add "static/vita/sce_sys/icon0.png=sce_sys/icon0.png" \
    --add "static/vita/sce_sys/livearea/contents/bg0.png=sce_sys/livearea/contents/bg0.png" \
    --add "static/vita/sce_sys/livearea/contents/startup.png=sce_sys/livearea/contents/startup.png" \
    --add "static/vita/sce_sys/livearea/contents/template.xml=sce_sys/livearea/contents/template.xml" \
    --add "static/vita/sce_sys/pic0.png=sce_sys/pic0.png" \
    "$VITA_DIR/ruffle4consoles.vpk"
