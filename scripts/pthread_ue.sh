#!/usr/bin/env bash
#
# Install or restore the userspace-OSAL libpthread.a replacement.
#
# The stock vitasdk libpthread.a (pthread-embedded) backs every pthread
# mutex/condvar/semaphore with a kernel object (sceKernelCreateMutex /
# sceKernelCreateSema). The kernel keeps a finite pool of these and ruffle
# creates hundreds during game preload, exhausting the pool so a later
# pthread_cond_init fails with ENOSPC (errno 28).
#
# This repo vendors the pthread-embedded source with a userspace OSAL
# (platform/vita/vita_osal.c) that implements semaphores and mutexes on top
# of LwMutex/LwCond work areas allocated in process memory, so each pthread
# sync object consumes no per-object kernel allocation.
#
# Usage:
#   scripts/pthread_ue.sh install   # back up SDK lib and install replacement
#   scripts/pthread_ue.sh restore   # restore the original SDK lib

set -euo pipefail

SDK_LIB="/usr/local/vitasdk/arm-vita-eabi/lib/libpthread.a"
BAK_LIB="$SDK_LIB.sdk.bak"
REPL_LIB="$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib/pthread-embedded/platform/vita" && pwd)/libpthread.a"

if [[ ! -d /usr/local/vitasdk/arm-vita-eabi/lib ]]; then
  echo "error: vitasdk not found at /usr/local/vitasdk" >&2
  exit 1
fi

case "${1:-}" in
  install)
    if [[ ! -f "$REPL_LIB" ]]; then
      echo "error: replacement lib not built at $REPL_LIB" >&2
      echo "build it with: make -C lib/pthread-embedded/platform/vita" >&2
      exit 1
    fi
    if [[ ! -f "$BAK_LIB" ]]; then
      echo "backing up SDK lib to $BAK_LIB"
      cp -a "$SDK_LIB" "$BAK_LIB"
    else
      echo "backup already exists at $BAK_LIB (not overwriting)"
    fi
    echo "installing userspace-OSAL libpthread.a"
    cp -a "$REPL_LIB" "$SDK_LIB"
    echo "done. Rebuild the vpk; the game log should show:"
    echo "  [PTE] userspace pthread OSAL active (LwMutex+LwCond)"
    ;;
  restore)
    if [[ ! -f "$BAK_LIB" ]]; then
      echo "error: no backup at $BAK_LIB; nothing to restore" >&2
      exit 1
    fi
    echo "restoring original SDK libpthread.a"
    cp -a "$BAK_LIB" "$SDK_LIB"
    rm -f "$BAK_LIB"
    echo "done."
    ;;
  *)
    echo "usage: $0 {install|restore}" >&2
    exit 1
    ;;
esac
