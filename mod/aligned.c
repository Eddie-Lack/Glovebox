/*
 * Glover game mod: data that must sit on a 16 byte boundary (docs/technical.md, "Real hardware").
 *
 * GCC 2.7.2 keeps at most 8 bytes of an `aligned` attribute, so a variable's place depends on what the linker puts before it. This
 * object file is the one the linker script (tools/build_mod.py) puts first in .bss, on a 16 byte boundary. It holds the buffer of a
 * level file's 8 byte header (slots.c): the PI writes whole 8 byte words and the data cache works in 16 byte lines, so the buffer is
 * alone in its line. tools/build_mod.py checks the linked address.
 */
#include "common.h"

u32 levelHeader[4];
