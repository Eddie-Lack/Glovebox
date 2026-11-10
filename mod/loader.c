/*
 * Loader for the transforms mod, written over ScreenShot (personal.c), a debug function the game never calls.
 * MainProc's call to InitCRCTable is redirected here. It runs once, right after CreateMessageQueues makes
 * dmaMessageQ (which DMAMemory waits on) and before anything is loaded or drawn.
 *
 * With an Expansion Pak it copies the mod from ROM into MOD_RAM, near the top of the Expansion Pak's memory, with the game's own
 * DMAMemory, then invalidates the instruction cache over it. Mod_Init gives the game's heap the memory below it (heap.c). Without one it puts the original
 * instructions back at every hook except the parser's, which it points at NoPakParseToken: that skips
 * SetActorRotation and SetActorScale after an enemy, platform, powerup or water. The game then ignores them;
 * it does not apply them to the previous landscape actor.
 * MOD_RAM, MOD_ROM, MOD_SIZE, PARSE_HOOK and the hook table come from tools/build_mod.py (hooks.h).
 */
#include "common.h"

void DMAMemory(void *dest, u32 start, u32 end);
void osInvalICache(void *addr, s32 size);
void osWritebackDCache(void *addr, s32 size);
void InitCRCTable(void);
s32 ParseTokenGeneric(s16 token, u8 **data);
void Mod_Init(void);

typedef struct Hook {
    u32 *addr;
    u32 original;
} Hook;

#include "hooks.h"

#define OS_MEM_SIZE (*(u32 *)0x80000318)
#define JAL(fn) (0x0C000000 | (((u32)(fn) & 0x0FFFFFFF) >> 2))

static s32 lastIsLand = TRUE;

s32 NoPakParseToken(s16 token, u8 **data) {
    if (token == TOKEN_LANDROT || token == TOKEN_LANDSCALE) {
        if (!lastIsLand) {
            *data += 12;
            return TRUE;
        }
    } else if (token == TOKEN_LANDSCAPE || token == TOKEN_BACKGROUND || token == TOKEN_CAMEO_OBJ) {
        lastIsLand = TRUE;
    } else if (token == TOKEN_WATER || token == TOKEN_PLATFORM || token == TOKEN_DUMMY || token == TOKEN_ENEMY
               || token == TOKEN_PICKUP) {
        lastIsLand = FALSE;
    }
    return ParseTokenGeneric(token, data);
}

void ModLoader(void) {
    s32 i;

    if (OS_MEM_SIZE >= 0x800000) {
        /* DMAMemory invalidates the data cache over the target before the DMA. The ROM address, the RAM address and the length are all
           multiples of 16 (tools/hw_check.py checks it). */
        DMAMemory((void *)MOD_RAM, MOD_ROM, MOD_ROM + MOD_SIZE);
        osInvalICache((void *)MOD_RAM, MOD_SIZE);
        Mod_Init();
    } else {
        for (i = 0; i < (s32)(sizeof(hooks) / sizeof(hooks[0])); i++) {
            if (hooks[i].addr == (u32 *)PARSE_HOOK) {
                *hooks[i].addr = JAL(NoPakParseToken);
            } else {
                *hooks[i].addr = hooks[i].original;
            }
            osWritebackDCache(hooks[i].addr, 4);
            osInvalICache(hooks[i].addr, 4);
        }
    }
    InitCRCTable();
}
