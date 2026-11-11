/*
 * Glover game mod: extra level slots.
 *
 * The editor can store new levels behind the mod instead of in the ROM's level files. crates/core/src/modpatch.rs lists them in
 * modSlots: per Space slot 35 to 41, the cart address of its file, its name, song and bonus time. The ROM keeps the
 * retail Space levels in those slots. A flag, modSlotsActive, decides which ones load:
 *   - A loading zone in a hub (a platform with exit type 11, SPACE_EXIT, and the tag modSlots.entryTag) sets the flag.
 *     Every other hub portal clears it, and so does the parse of any level that is not an extra one or the way room (42).
 *   - With the flag set, LoadLandscapeFile reads the level from modSlots, levelInfo shows the extra level's name, song and
 *     bonus time, the extra banks are those of the extra level (banks.c, key level + 0x40), and the Space world's bits in
 *     the save are swapped for the extra levels'.
 *   - Reaching the end of an extra boss (exit type 3) leaves the hub's crystal count alone and returns to the hub.
 *
 * Save data. The Space world's level-done bits (`levels` bits 25 to 29), all-garibs bits (`allGaribs`, the same bits) and
 * best times (bestTimes row 5) are swapped with modSlotsStash* when the flag changes. The game's own code (way room doors,
 * BonusOpen, CheckForLevelCompleted, the end-of-level screens) then works on the extra levels' values. The EEPROM writes
 * always get the retail values (Mod_osEepromLongWrite builds the images and their CRCs). The extra levels' values are
 * stored in EEPROM blocks SLOTS_BLOCK to SLOTS_BLOCK + 5, which the game does not use, in one record with its own CRC.
 *
 * Its hooks are installed at run time by Mod_InitSlots, only with an Expansion Pak. The call sites come from
 * tools/build_mod.py (sites.h).
 */
#include "common.h"
#include "game.h"
#include "save.h"
#include "platform.h"
#include "crc.h"
#include "enums.h"
#include "hand.h"
#include "ball.h"
#include "actor.h"

#define SLOTS_FIRST 35 /* the first Space level */
#define SLOTS_COUNT 7  /* 35 to 41 */
#define SLOTS_BLOCK 56
#define RECORD_SIZE 48
#define NUM_SLOTS 6     /* save slots in the EEPROM */
#define NUM_RAM_SLOTS 7 /* the level select uses a seventh slot after them, in RAM only */
#define TIMES_SIZE 30
#define TIMES_OFFSET (5 * TIMES_SIZE) /* Space is row 5 of bestTimes */
#define TIMES_BLOCK 0xC0 /* the game writes and checks this much from bestTimes (eeprom.c), 12 bytes past its 6 rows */
#define WAYROOM_LEVEL 42
#define ANY_HUB 0xFF
#define RING_TAG 77      /* a checkpoint ring a rolling ball touches (the editor's checkpoint ring preset) */
#define RING_BELOW 40.0f /* a ball this far under a ring's height, or less, still counts as coming down through it */
#define MAX_LEVEL_FILE 0x40000 /* the largest level file the mod reads: 256 KiB (the retail levels are under 64 KiB) */
#define JAL(fn) (0x0C000000 | (((u32)(fn) & 0x0FFFFFFF) >> 2))

typedef struct SlotLevel {
    u32 rom;       /* cart address of the level file; 0 if this slot has no extra level */
    s16 bonusTime; /* 0x7FFF keeps the retail value */
    u8 song;       /* 0xFF keeps the retail value */
    u8 pad;
    char name[0x28];
} SlotLevel;

typedef struct SlotTable {
    u32 magic;
    u8 boot;     /* test ROMs: start with the flag set, when the first level loaded is an extra one */
    u8 entryHub; /* the hub level (0 to 7) whose zone leads to the extra levels, or ANY_HUB */
    u16 entryTag;
    SlotLevel level[SLOTS_COUNT];
} SlotTable;

/* Filled in by crates/core/src/modpatch.rs */
SlotTable modSlots = { 0x534C5431 };

extern u8 bestTimes[6][TIMES_SIZE];
extern u8 bestTimesCopy[6][TIMES_SIZE];
extern u32 eepromCRCS[8];
extern s16 eepromPresent;
extern s16 currentSaveSlot;
extern OSIoMesg dmaIOMessageBuf;
extern OSMesgQueue dmaMessageQ;

void *LoadLandscapeFile(s32 num);
void CheckForExit(Platform *platform);
void StartNewGame(void);
s32 BonusOpen(void);
void CheckRestartGate(Platform *platform);
void *JallocAlloc(s32 size, s32 clear, char *name);
s32 DMAGetSize(u32 start, u32 end);
void DMAMemory(void *dest, u32 start, u32 end);
void Wait(s16 time);
void StopDrawing(char *name);
void osInvalICache(void *addr, s32 size);
void osWritebackDCache(void *addr, s32 size);
void osInvalDCache(void *addr, s32 size);

typedef struct RetailInfo {
    char name[0x28];
    u8 song;
    s16 bonusTime;
} RetailInfo;

s32 modSlotsActive; /* the flag; tests read it (mod.json lists its address). Declared before the arrays: GCC 2.7.2 does not align .bss */
static s32 bootPending;
static s32 recordOnCartValid;
static s32 recordIoFailed; /* a read or write of the record returned an error: the record is left alone for the rest of the session */
u16 modSlotsStash[NUM_RAM_SLOTS]; /* levels bits 0-4, allGaribs bits 5-9: the other set's, whichever is not in saveSlot */
static u8 hubTargetAtStart;
u8 modSlotsStashTimes[TIMES_SIZE];
static s8 worldCompleteAtStart[7];
static RetailInfo retailInfo[SLOTS_COUNT];
static u8 retailSlots[NUM_SLOTS * sizeof(SaveSlot)];
static u8 retailTimes[TIMES_BLOCK];
static u8 recordOnCart[RECORD_SIZE];

#include "sites.h"

static u16 SlotBits(SaveSlot *s) {
    return ((s->levels >> 25) & 31) | (((s->allGaribs >> 25) & 31) << 5);
}

static void PutSlotBits(SaveSlot *s, u16 bits) {
    s->levels = (s->levels & ~(31 << 25)) | ((bits & 31) << 25);
    s->allGaribs = (s->allGaribs & ~(31 << 25)) | (((bits >> 5) & 31) << 25);
}

static void SwapSaveBits(void) {
    s32 i;
    u16 old;
    u8 t;

    for (i = 0; i < NUM_RAM_SLOTS; i++) {
        old = SlotBits(&saveSlot[i]);
        PutSlotBits(&saveSlot[i], modSlotsStash[i]);
        modSlotsStash[i] = old;
    }
    for (i = 0; i < TIMES_SIZE; i++) {
        t = bestTimes[5][i];
        bestTimes[5][i] = modSlotsStashTimes[i];
        modSlotsStashTimes[i] = t;
    }
}

static void SwapLevelInfo(s32 toExtra) {
    s32 i;
    LevelInfo *info;
    SlotLevel *sl;

    for (i = 0; i < SLOTS_COUNT; i++) {
        sl = &modSlots.level[i];
        info = &levelInfo[SLOTS_FIRST + i];
        if (sl->rom == 0) {
            continue;
        }
        if (toExtra) {
            memcpy(retailInfo[i].name, info->name, sizeof(info->name));
            retailInfo[i].song = info->song;
            retailInfo[i].bonusTime = info->bonusTime;
            memcpy(info->name, sl->name, sizeof(info->name));
            if (sl->song != 0xFF) {
                info->song = sl->song;
            }
            if (sl->bonusTime != 0x7FFF) {
                info->bonusTime = sl->bonusTime;
            }
        } else {
            memcpy(info->name, retailInfo[i].name, sizeof(info->name));
            info->song = retailInfo[i].song;
            info->bonusTime = retailInfo[i].bonusTime;
        }
    }
}

/* Turns the extra levels on or off: levelInfo rows and the save bits change together */
static void SlotsSet(s32 on) {
    if (on == modSlotsActive) {
        return;
    }
    if (on) {
        hubTargetAtStart = saveSlot[currentSaveSlot].hubTarget;
        memcpy(worldCompleteAtStart, gameInfo.worldComplete, sizeof(worldCompleteAtStart));
    }
    SwapSaveBits();
    SwapLevelInfo(on);
    modSlotsActive = on;
}

static s32 IsExtraLevel(s32 num) {
    return num >= SLOTS_FIRST && num < SLOTS_FIRST + SLOTS_COUNT && modSlots.level[num - SLOTS_FIRST].rom != 0;
}

/* Called before a level is parsed. Returns the level number to look up in modExtraBanks. */
s32 Mod_SlotLevel(s32 num) {
    if (bootPending) {
        bootPending = FALSE;
        if (IsExtraLevel(num)) {
            SlotsSet(TRUE);
        }
    }
    if (modSlotsActive && num != WAYROOM_LEVEL && !IsExtraLevel(num)) {
        SlotsSet(FALSE);
    }
    return (modSlotsActive && IsExtraLevel(num)) ? num + 0x40 : num;
}

/* The first 8 bytes of a level file, read by DMA. The PI writes whole 8 byte words and the data cache works in 16 byte lines, so the
   buffer is alone in its line and 16 byte aligned. It is in aligned.c, which the linker script places on a 16 byte boundary. */
extern u32 levelHeader[4];

void *Mod_LoadLandscapeFile(s32 num) {
    u32 rom;
    void *buf;
    s32 size;

    if (!modSlotsActive || !IsExtraLevel(num)) {
        return LoadLandscapeFile(num);
    }
    rom = modSlots.level[num - SLOTS_FIRST].rom;
    osInvalDCache(levelHeader, sizeof(levelHeader));
    osPiStartDma(&dmaIOMessageBuf, 0, 0, rom, levelHeader, 8, &dmaMessageQ);
    osRecvMesg(&dmaMessageQ, NULL, OS_MESG_BLOCK);
    size = levelHeader[0];
    if (size < 16 || size > MAX_LEVEL_FILE) {
        /* as LoadLandscapeFile does for a file it cannot find: a message instead of a hang */
        StopDrawing("loadlandscape");
        gameInfo.mode = 13;
        sprintf(gameInfo.message, "LEVEL UNPLAYABLE - NO LEV FILE");
        return NULL;
    }
    /* modpatch.rs stores each file padded with zeros to a multiple of 16, so the DMA reads a whole number of 16 byte blocks */
    size = (size + 15) & ~15;
    buf = JallocAlloc(DMAGetSize(rom, rom + size), 1, "LandData");
    DMAMemory(buf, rom, rom + size);
    return buf;
}

/* The entry zone sets the flag; the other hub portals clear it. Only the call that starts the exit counts. */
void Mod_CheckForExit(Platform *platform) {
    u8 before = gameInfo.exitFlags & 1;
    s32 hub;

    CheckForExit(platform);
    if (!before && (gameInfo.exitFlags & 1) && platform->exitType >= 6 && platform->exitType <= 11) {
        hub = modSlots.entryHub == ANY_HUB ? gameInfo.level <= HUB8 : gameInfo.level == modSlots.entryHub;
        SlotsSet(platform->exitType == 11 && platform->tag == modSlots.entryTag && hub);
    }
}

/* CheckForExits (game.c, case WORLD_EXIT) has just counted a crystal and marked the world done; BonusOpen decides between
   the way room and the hub. An extra boss gives the crystal back and goes to the hub. */
s32 Mod_WorldExitBonusOpen(void) {
    if (modSlotsActive) {
        saveSlot[currentSaveSlot].hubTarget = hubTargetAtStart;
        memcpy(gameInfo.worldComplete, worldCompleteAtStart, sizeof(worldCompleteAtStart));
        return FALSE;
    }
    return BonusOpen();
}

/* A new game in a slot starts the extra levels over too */
void Mod_StartNewGame(void) {
    StartNewGame();
    if (modSlotsActive) {
        PutSlotBits(&saveSlot[currentSaveSlot], 0);
    } else {
        modSlotsStash[currentSaveSlot] = 0;
    }
}

/* A checkpoint ring is touched when the ball comes down through its height (CheckRestartGate). A ball that rolls on a flat floor
   never does, and a ring sits 26 over the floor, where the restart needs it. For a ring with the tag RING_TAG in an extra level the
   ball counts as coming from above when it is under the ring's height and not more than RING_BELOW under it: the game's own check
   then runs with the ball's previous y put above the ring for the call. */
void Mod_CheckRestartGate(Platform *platform) {
    f32 prevY = ball.prevPos.y;
    f32 ringY = platform->actor.pos.y;
    s32 lend = FALSE;

    if (modSlotsActive && platform->tag == RING_TAG && platform->restartGate != NULL && platform->restartGate->untouched
        && ball.pos.y <= ringY && ball.pos.y > ringY - RING_BELOW && ringY >= prevY) {
        ball.prevPos.y = ringY + 1.0f;
        lend = TRUE;
    }
    CheckRestartGate(platform);
    if (lend) {
        ball.prevPos.y = prevY;
    }
}

/* EEPROM. The game writes saveSlot, bestTimes and their CRCs from RAM; with the flag set RAM holds the extra levels' bits,
   the writes then get copies with the retail bits. */

static void BuildRetailImages(void) {
    s32 i;

    memcpy(retailSlots, saveSlot, sizeof(retailSlots));
    memcpy(retailTimes, bestTimes, sizeof(retailTimes)); /* bestTimes and the 12 bytes after it, as the game writes them */
    if (modSlotsActive) {
        for (i = 0; i < NUM_SLOTS; i++) {
            PutSlotBits((SaveSlot *)retailSlots + i, modSlotsStash[i]);
        }
        memcpy(retailTimes + TIMES_OFFSET, modSlotsStashTimes, TIMES_SIZE);
    }
}

static void BuildRecord(u8 *rec) {
    s32 i;
    u16 *bits = (u16 *)rec;

    for (i = 0; i < NUM_SLOTS; i++) {
        bits[i] = modSlotsActive ? SlotBits(&saveSlot[i]) : modSlotsStash[i];
    }
    memcpy(rec + 12, modSlotsActive ? bestTimes[5] : modSlotsStashTimes, TIMES_SIZE);
    rec[42] = rec[43] = 0;
    *(u32 *)(rec + 44) = UpdateCRCData(rec, 44);
}

static void WriteRecord(OSMesgQueue *mq) {
    u8 rec[RECORD_SIZE];
    s32 i;

    if (recordIoFailed || !eepromPresent) {
        return;
    }
    BuildRecord(rec);
    for (i = 0; recordOnCartValid && i < RECORD_SIZE && rec[i] == recordOnCart[i]; i++) {
    }
    if (recordOnCartValid && i == RECORD_SIZE) {
        return;
    }
    if (osEepromLongWrite(mq, SLOTS_BLOCK, rec, RECORD_SIZE) != 0) {
        recordIoFailed = TRUE;
        return;
    }
    Wait(10000);
    memcpy(recordOnCart, rec, RECORD_SIZE);
    recordOnCartValid = TRUE;
}

static void ClearStash(void) {
    s32 i;

    for (i = 0; i < NUM_SLOTS; i++) {
        modSlotsStash[i] = 0;
    }
    memcpy(modSlotsStashTimes, bestTimesCopy[5], TIMES_SIZE);
    recordOnCartValid = FALSE;
}

static void ReadRecord(OSMesgQueue *mq) {
    u8 rec[RECORD_SIZE];
    s32 i;
    u16 *bits = (u16 *)rec;

    if (!eepromPresent) {
        return;
    }
    if (osEepromLongRead(mq, SLOTS_BLOCK, rec, RECORD_SIZE) != 0) {
        /* no answer from the chip: nothing is read and nothing is written over the record */
        recordIoFailed = TRUE;
        ClearStash();
        return;
    }
    Wait(10000);
    if (UpdateCRCData(rec, 44) == *(u32 *)(rec + 44) && (bits[0] & 0xFC00) == 0) {
        for (i = 0; i < NUM_SLOTS; i++) {
            modSlotsStash[i] = bits[i];
        }
        memcpy(modSlotsStashTimes, rec + 12, TIMES_SIZE);
        memcpy(recordOnCart, rec, RECORD_SIZE);
        recordOnCartValid = TRUE;
    } else {
        ClearStash();
    }
}

s32 Mod_osEepromLongWrite(OSMesgQueue *mq, u8 address, u8 *buffer, s32 length) {
    s32 ret;
    u32 crcs[8];
    s32 i;

    if (modSlotsActive && length > 0) {
        if (address == 0x1F && buffer == (u8 *)saveSlot) {
            BuildRetailImages();
            ret = osEepromLongWrite(mq, address, retailSlots, length);
        } else if (address == 0x07 && buffer == (u8 *)bestTimes) {
            BuildRetailImages();
            ret = osEepromLongWrite(mq, address, retailTimes, length > TIMES_BLOCK ? TIMES_BLOCK : length);
        } else if (address == 0x34 && buffer == (u8 *)eepromCRCS) {
            BuildRetailImages();
            for (i = 0; i < NUM_SLOTS; i++) {
                crcs[i] = UpdateCRCData(retailSlots + i * sizeof(SaveSlot), sizeof(SaveSlot));
            }
            crcs[6] = eepromCRCS[6];
            crcs[7] = UpdateCRCData(retailTimes, sizeof(retailTimes));
            ret = osEepromLongWrite(mq, address, (u8 *)crcs, length);
        } else {
            ret = osEepromLongWrite(mq, address, buffer, length);
        }
    } else {
        ret = osEepromLongWrite(mq, address, buffer, length);
    }
    if (address == 0x1F && buffer == (u8 *)saveSlot) {
        Wait(10000);
        WriteRecord(mq);
    }
    return ret;
}

s32 Mod_osEepromLongRead(OSMesgQueue *mq, u8 address, u8 *buffer, s32 length) {
    s32 ret = osEepromLongRead(mq, address, buffer, length);

    if (address == 0x1F && buffer == (u8 *)saveSlot) {
        Wait(10000);
        ReadRecord(mq);
    }
    return ret;
}

static void Patch(u32 *const *sites, s32 count, void *original, void *replacement) {
    s32 i;

    for (i = 0; i < count; i++) {
        if (*sites[i] == JAL(original)) {
            *sites[i] = JAL(replacement);
            osWritebackDCache(sites[i], 4);
            osInvalICache(sites[i], 4);
        }
    }
}

#define PATCH(list, original, replacement) Patch(list, sizeof(list) / sizeof(list[0]), (void *)original, (void *)replacement)

/* The hooks go in only when a level is stored behind the mod; a ROM with none keeps the game's own calls */
void Mod_InitSlots(void) {
    s32 i;
    s32 any = FALSE;

    for (i = 0; i < SLOTS_COUNT; i++) {
        if (modSlots.level[i].rom != 0) {
            any = TRUE;
        }
    }
    if (!any) {
        return;
    }
    PATCH(sitesLoadLandscapeFile, LoadLandscapeFile, Mod_LoadLandscapeFile);
    PATCH(sitesCheckForExit, CheckForExit, Mod_CheckForExit);
    PATCH(sitesBonusOpen, BonusOpen, Mod_WorldExitBonusOpen);
    PATCH(sitesStartNewGame, StartNewGame, Mod_StartNewGame);
    PATCH(sitesEepromWrite, osEepromLongWrite, Mod_osEepromLongWrite);
    PATCH(sitesEepromRead, osEepromLongRead, Mod_osEepromLongRead);
    PATCH(sitesCheckRestartGate, CheckRestartGate, Mod_CheckRestartGate);
    memcpy(modSlotsStashTimes, bestTimesCopy[5], TIMES_SIZE);
    bootPending = modSlots.boot;
}
