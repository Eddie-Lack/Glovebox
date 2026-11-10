/*
 * Glover game mod: models and textures from other worlds, loaded from the player's own ROM.
 *
 * The game has 5 object bank slots and 5 texture bank slots, and a level fills about 3 of each (LoadLevel).
 * A level built with the editor can use models from other banks. Instead of copying those models into the
 * patched ROM, the patch lists the extra banks each level needs (modExtraBanks, filled in by
 * tools/mod_patch.py). Before the level is parsed, Mod_LoadExtraBanks loads them with the game's own
 * LoadTextureBank and LoadObjectBank: it empties the slots for the call, then moves the new bank into the
 * mod's own lists and puts the level's banks back. FindTexture and FindObject are redirected to versions
 * that search the game's slots first, then the mod's lists.
 *
 * modExtraBanks holds one record per level: level number, number of texture banks, number of object banks,
 * then the texture bank numbers and the object bank numbers. 0xFF ends the list. Bit 7 of the level number
 * (0x80) makes the extra banks win when a texture id is in both the level's own banks and an extra bank. Two
 * worlds can hold different pictures under the same id (the Atlantis whale's mouth and a Space texture).
 *
 * Extra banks stay loaded until the next level is parsed. If the game has not started the heap again since
 * then (heap.c counts this in modHeapGen), the mod frees them first. Animated textures in extra banks do not
 * animate: the mod gives LoadTextureBank an empty cyclingTextures list and puts the level's list back after.
 * CycleTextures never holds a pointer into a bank the mod frees, and the list's 20 entries never overflow.
 */
#include "common.h"
#include "texture.h"
#include "banks.h"
#include "game.h"

void LoadTextureBank(u32 num);
void LoadObjectBank(u32 num);
void JallocFree(void *ptrRef);
void FreeOFTC(Object *obj);
void StopDrawing(char *name);
void osWritebackDCache(void *addr, s32 size);
void osInvalICache(void *addr, s32 size);
void FindTexture(Texture **texPtr, u32 texID, s32 verbose);
void FindObject(ObjectInstance **objCPtr, u32 objID, char *name, s32 vital);
void *JallocAlloc(s32 size, s32 clear, char *name);
s32 DMAGetSize(u32 start, u32 end);
void DMAMemory(void *dest, u32 start, u32 end);
s32 Rom2Ram(s32 addr, s32 base);
void RestoreObjectPointers(Object *obj, s32 base);

extern s32 numCyclingTextures;
extern Texture *cyclingTextures[20];
extern s32 modHeapGen;

#define MAX_EXTRA_TEX 8
#define MAX_EXTRA_OBJ 16
#define END_OF_LIST 0xFF

u8 modExtraBanks[256] = { END_OF_LIST };

/* Custom models (generated glTF, packed by tools/mod_patch.py): per level key one texture bank and one object bank in the
   mod's ROM store, in the game's own bank format and uncompressed. The key is the one of modExtraBanks. */
typedef struct CustomEntry {
    u8 key;
    u8 pad[3];
    u32 texStart;
    u32 texEnd;
    u32 objStart;
    u32 objEnd;
} CustomEntry;

#define MAX_CUSTOM 8

typedef struct CustomTable {
    u32 magic; /* 'CUS1' */
    u32 count;
    CustomEntry entry[MAX_CUSTOM];
} CustomTable;

CustomTable modCustom = { 0x43555331 };

static TextureBank extraTex[MAX_EXTRA_TEX];
static ObjectBank extraObj[MAX_EXTRA_OBJ];
static s32 numExtraTex;
static s32 numExtraObj;
static s32 extraHeapGen;
static s32 extraFirst;

static Texture *SearchTextureBank(TextureBank *bank, u32 texID) {
    s32 j;
    u8 *p;
    Texture *tex;

    p = (u8 *)bank->textures;
    for (j = 0; j < bank->count; j++) {
        tex = (Texture *)p;
        if (tex->crc == texID) {
            return tex;
        }
        p += tex->size;
    }
    return NULL;
}

static ObjectInstance *SearchObjectBank(ObjectBank *bank, u32 objID) {
    s32 j;

    for (j = 0; j < bank->count; j++) {
        if (bank->entries[j].crc == objID) {
            return bank->entries[j].inst;
        }
    }
    return NULL;
}

void Mod_FindTexture(Texture **texPtr, u32 texID, s32 verbose) {
    s32 i;
    Texture *tex;

    if (extraFirst) {
        for (i = 0; i < numExtraTex; i++) {
            if ((tex = SearchTextureBank(&extraTex[i], texID)) != NULL) {
                *texPtr = tex;
                return;
            }
        }
    }
    for (i = 4; i >= 0; i--) {
        if (textureBanks[i].data && (tex = SearchTextureBank(&textureBanks[i], texID)) != NULL) {
            *texPtr = tex;
            return;
        }
    }
    for (i = 0; i < numExtraTex; i++) {
        if ((tex = SearchTextureBank(&extraTex[i], texID)) != NULL) {
            *texPtr = tex;
            return;
        }
    }
    *texPtr = NULL;
}

void Mod_FindObject(ObjectInstance **objCPtr, u32 objID, char *name, s32 vital) {
    s32 i;
    ObjectInstance *inst;

    for (i = 0; i < 5; i++) {
        if (objectBanks[i].data && (inst = SearchObjectBank(&objectBanks[i], objID)) != NULL) {
            *objCPtr = inst;
            return;
        }
    }
    for (i = 0; i < numExtraObj; i++) {
        if ((inst = SearchObjectBank(&extraObj[i], objID)) != NULL) {
            *objCPtr = inst;
            return;
        }
    }
    *objCPtr = NULL;
    if (vital) {
        if (name) {
            sprintf(gameInfo.message, "LEVEL UNPLAYABLE - %s MISSING", name);
        } else {
            sprintf(gameInfo.message, "LEVEL UNPLAYABLE - OBJECT MISSING");
        }
        StopDrawing("fondobj");
        gameInfo.mode = 13;
    }
}

static void Redirect(u32 *fn, void *to) {
    fn[0] = 0x08000000 | (((u32)to & 0x0FFFFFFF) >> 2); /* j to */
    fn[1] = 0;                                           /* nop */
    osWritebackDCache(fn, 8);
    osInvalICache(fn, 8);
}

void Mod_InitBanks(void) {
    Redirect((u32 *)FindTexture, Mod_FindTexture);
    Redirect((u32 *)FindObject, Mod_FindObject);
}

/* The game has started the heap again (heap.c) and the extra banks' memory is gone. FindTexture and FindObject must not look in it. */
void Mod_ForgetExtraBanks(void) {
    numExtraObj = 0;
    numExtraTex = 0;
    extraFirst = 0;
}

static void FreeExtraBanks(void) {
    s32 i;
    s32 j;

    /* After the game starts the heap again, the banks' memory is gone already */
    if (extraHeapGen == modHeapGen) {
        for (i = 0; i < numExtraObj; i++) {
            for (j = 0; j < extraObj[i].count; j++) {
                FreeOFTC(extraObj[i].entries[j].inst->obj);
            }
            JallocFree(&extraObj[i].data);
        }
        for (i = 0; i < numExtraTex; i++) {
            JallocFree(&extraTex[i].data);
        }
    }
    numExtraObj = 0;
    numExtraTex = 0;
    extraFirst = 0;
}

/* Loads a texture bank and an object bank from the ROM as the game's LoadTextureBank and LoadObjectBank do (banks.c), into the
   mod's lists. Animated textures are not supported. */
static void LoadCustomBanks(CustomEntry *e) {
    s32 *tptr;
    u8 *optr;
    Texture *cur;
    Texture *tex;
    TextureBank *tb;
    ObjectBank *ob;
    ObjectBankEntry *ent;
    ObjectInstance *inst;
    s32 i;

    if (e->texEnd > e->texStart && numExtraTex < MAX_EXTRA_TEX) {
        tptr = JallocAlloc(DMAGetSize(e->texStart, e->texEnd), 0, "CusTex");
        DMAMemory(tptr, e->texStart, e->texEnd);
        tb = &extraTex[numExtraTex++];
        tb->data = (s32)tptr;
        tb->count = *tptr;
        tb->textures = (s32)(tptr + 1);
        cur = (Texture *)tb->textures;
        for (i = 0; i < tb->count; i++) {
            tex = cur;
            tex->data = (u8 *)tex + sizeof(Texture);
            if (tex->palette) {
                tex->palette = (s32 *)((u32)tex->palette + (u32)tex);
            }
            cur = (Texture *)((u8 *)cur + tex->size);
        }
    }
    if (e->objEnd > e->objStart && numExtraObj < MAX_EXTRA_OBJ) {
        optr = JallocAlloc(DMAGetSize(e->objStart, e->objEnd), 0, "CusObj");
        DMAMemory(optr, e->objStart, e->objEnd);
        ob = &extraObj[numExtraObj++];
        ob->data = optr;
        ob->entries = (ObjectBankEntry *)optr;
        for (i = 0; ob->entries[i].inst; i++) {
            ent = &ob->entries[i];
            ent->inst = (ObjectInstance *)Rom2Ram((s32)ent->inst, (s32)optr);
            inst = ent->inst;
            inst->obj = (Object *)Rom2Ram((s32)inst->obj, (s32)optr);
            if (inst->anim) {
                inst->anim = (AnimState *)Rom2Ram((s32)inst->anim, (s32)optr);
                if (inst->anim->anims) {
                    inst->anim->anims = (struct AnimInfo *)Rom2Ram((s32)inst->anim->anims, (s32)optr);
                }
            }
            RestoreObjectPointers(inst->obj, (s32)optr);
        }
        ob->count = i;
    }
}

void Mod_LoadExtraBanks(s32 level) {
    u8 *p;
    s32 i;
    s32 numTex;
    s32 numObj;
    s32 cycling;
    Texture *cyclingSave[20];
    TextureBank texSlots[5];
    ObjectBank objSlots[5];

    FreeExtraBanks();
    extraHeapGen = modHeapGen;
    for (p = modExtraBanks; p[0] != END_OF_LIST && (p[0] & 0x7F) != level; p += 3 + p[1] + p[2]) {
    }
    for (i = 0; i < modCustom.count && i < MAX_CUSTOM; i++) {
        if (modCustom.entry[i].key == level) {
            LoadCustomBanks(&modCustom.entry[i]);
        }
    }
    if (p[0] == END_OF_LIST) {
        return;
    }
    extraFirst = p[0] >> 7;
    numTex = p[1];
    numObj = p[2];
    p += 3;

    cycling = numCyclingTextures;
    memcpy(cyclingSave, cyclingTextures, sizeof(cyclingSave));
    for (i = 0; i < numTex; i++, p++) {
        if (numExtraTex >= MAX_EXTRA_TEX) {
            continue;
        }
        memcpy(texSlots, textureBanks, sizeof(texSlots));
        bzero(textureBanks, sizeof(texSlots));
        numCyclingTextures = 0;
        LoadTextureBank(*p);
        if (textureBanks[0].data) {
            extraTex[numExtraTex++] = textureBanks[0];
        }
        memcpy(textureBanks, texSlots, sizeof(texSlots));
    }
    memcpy(cyclingTextures, cyclingSave, sizeof(cyclingSave));
    numCyclingTextures = cycling;

    /* Their models find their textures through Mod_FindTexture, which searches the banks loaded above */
    for (i = 0; i < numObj; i++, p++) {
        if (numExtraObj >= MAX_EXTRA_OBJ) {
            continue;
        }
        memcpy(objSlots, objectBanks, sizeof(objSlots));
        bzero(objectBanks, sizeof(objSlots));
        LoadObjectBank(*p);
        if (objectBanks[0].data) {
            extraObj[numExtraObj++] = objectBanks[0];
        }
        memcpy(objectBanks, objSlots, sizeof(objSlots));
    }
}
