/*
 * Glover game mod: the matrix guard.
 *
 * The game's per-frame matrices (Dynamic.objectMtx in display.h) hold 256. Every drawn object node, shadow and footprint takes one, and
 * the game does not check the count. The 257th and 258th overwrite the camera's matrices, and the next ones overwrite the start of the
 * display list. The RSP then runs matrix data as commands and the frame never finishes. The game stops (mupen64plus showed it with rice
 * and with angrylion in a level with 259 matrices in one view).
 * The guard skips what would pass the end. It skips an actor whose object nodes would leave fewer than MTX_RESERVE free. The opaque
 * pass draws the nearest first, so the far ones go. The glove and the ball may use the reserve. The guard skips a shadow or footprint
 * when no matrix is free.
 * modPeakMatrices is the most matrices a frame asked for, the skipped ones included. modSkippedDraws counts the skipped draws.
 * mod.json has the addresses of both, for tests in the emulator.
 *
 * Its hooks are installed at run time by Mod_InitGuard, only with an Expansion Pak. The ROM's code does not change for them. The call
 * sites come from tools/build_mod.py (sites.h).
 */
#include "common.h"
#include "hand.h"
#include "ball.h"
#include "actor.h"

#define JAL(fn) (0x0C000000 | (((u32)(fn) & 0x0FFFFFFF) >> 2))
#define MTX_MAX 256
#define MTX_RESERVE 24

#include "sites.h"

void TransformAndDrawObject(Object *obj, f32 time, s16 doKeys, s16 arg3);
void CastShadow(Vec3 *pos, Vec3 *plane, f32 size, f32 altitude, u16 alpha, void *vert, Vec3 *lightDir);
void DrawFootprint(void *footprint);
void osInvalICache(void *addr, s32 size);
void osWritebackDCache(void *addr, s32 size);
extern s32 objectMatrix;
extern Actor *currentDrawActor;

s32 modPeakMatrices;
s32 modSkippedDraws;

static s32 CountNodes(Object *obj) {
    s32 n = 0;

    for (; obj != NULL; obj = obj->sibling) {
        n += 1 + CountNodes(obj->child);
    }
    return n;
}

static s32 MatrixRoom(s32 need, s32 reserve) {
    static s32 lastCount;
    static s32 skippedNodes;

    /* objectMatrix starts at 0 each frame (DrawObjects): what a frame asks for is the count plus what was skipped in it */
    if (objectMatrix < lastCount) {
        skippedNodes = 0;
    }
    lastCount = objectMatrix;
    if (objectMatrix + skippedNodes + need > modPeakMatrices) {
        modPeakMatrices = objectMatrix + skippedNodes + need;
    }
    if (objectMatrix + need > MTX_MAX - reserve) {
        skippedNodes += need;
        modSkippedDraws++;
        return FALSE;
    }
    return TRUE;
}

void Mod_TransformAndDrawObject(Object *obj, f32 time, s16 doKeys, s16 arg3) {
    s32 own = currentDrawActor == &hand || currentDrawActor == &ball;

    if (MatrixRoom(CountNodes(obj), own ? 0 : MTX_RESERVE)) {
        TransformAndDrawObject(obj, time, doKeys, arg3);
    }
}

void Mod_CastShadow(Vec3 *pos, Vec3 *plane, f32 size, f32 altitude, u16 alpha, void *vert, Vec3 *lightDir) {
    if (MatrixRoom(1, 0)) {
        CastShadow(pos, plane, size, altitude, alpha, vert, lightDir);
    }
}

void Mod_DrawFootprint(void *footprint) {
    if (MatrixRoom(1, 0)) {
        DrawFootprint(footprint);
    }
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

void Mod_InitGuard(void) {
    PATCH(sitesTransformAndDrawObject, TransformAndDrawObject, Mod_TransformAndDrawObject);
    PATCH(sitesCastShadow, CastShadow, Mod_CastShadow);
    PATCH(sitesDrawFootprint, DrawFootprint, Mod_DrawFootprint);
}
