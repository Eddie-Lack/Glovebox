/*
 * Glover game mod: the Expansion Pak's memory for the game's heap.
 *
 * The game gives its Jalloc heap everything from jallocHeapStart (0x802B0D10) to 0x80400000, about 1.3 MB.
 * With the Expansion Pak the mod moves the end up to MOD_RAM, just below the mod itself, which adds about
 * 3.9 MB. Mod_Init runs from the loader right after MainProc's JallocInit, before anything is allocated. It
 * starts the heap again with the larger size. The game starts the heap again when it changes levels
 * (game.c, frontend.c); those calls go through Mod_JallocInit, which keeps the larger size.
 * Jalloc (jalloc.c) has no 4 MB limits of its own: static blocks grow down from the end, dynamic ones up from the
 * start, and sizes are 32-bit.
 */
#include "common.h"

void JallocInit(u32 start, u32 size);

extern u8 jallocHeapStart[];
extern u8 modBssStart[];
extern u8 modBssEnd[];

void Mod_InitBanks(void);
void Mod_InitGuard(void);
void Mod_InitSlots(void);
void Mod_ForgetExtraBanks(void);

/* How many times the heap has been started; banks.c frees its extra banks only within the same heap */
s32 modHeapGen;

void Mod_Init(void) {
    u8 *p;

    for (p = modBssStart; p < modBssEnd; p++) {
        *p = 0;
    }
    JallocInit((u32)jallocHeapStart, MOD_RAM - (u32)jallocHeapStart);
    modHeapGen++;
    Mod_InitBanks();
    Mod_InitGuard();
    Mod_InitSlots();
}

void Mod_JallocInit(u32 start, u32 size) {
    JallocInit(start, MOD_RAM - start);
    modHeapGen++;
    Mod_ForgetExtraBanks();
}
