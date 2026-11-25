/*
 * Glover game mod: rotation and scale for every 3D object, from the level file.
 *
 * Built against the gloveworld decomp (tools/build_mod.py) and loaded into Expansion Pak RAM (MOD_RAM) by
 * loader.c. The level format already has SetActorRotation (TOKEN_LANDROT) and SetActorScale (TOKEN_LANDSCALE).
 * The unmodded game applies them to the last landscape, background or animated background actor
 * (lastLandscape, parse.c). This mod applies them to the most recent object of any of these kinds instead:
 *
 *   landscape, background, animated background   unchanged: the game's own handler runs
 *   water                                        rotQuat and scale of the water actor (drawn every frame)
 *   platform, null platform                      rotQuat (or the spin angles, for a platform that spins)
 *                                                and scale, before the platform's collision is built
 *   enemy, powerup                               a base rotation and scale, multiplied in when drawn,
 *                                                because the game rewrites rotQuat and scale every frame
 *
 * In all 48 shipped levels every SetActorRotation and SetActorScale follows a landscape-type actor. The
 * shipped levels parse exactly as they do in the unmodded game.
 *
 * Hooks (call sites patched by tools/build_mod.py, each a same-size `jal` change):
 *   ParseLandscapeFile -> ParseTokenGeneric     Mod_ParseTokenGeneric
 *   LoadLevel -> ParseLandscapeFile             Mod_ParseLandscapeFile (which first loads the level's extra banks, banks.c)
 *   every call to DrawActor                     Mod_DrawActor
 *   UpdatePlatforms -> RestartEnemy (respawn)   Mod_RestartEnemy
 */
#include "common.h"
#include "actor.h"
#include "platform.h"
#include "collect.h"
#include "water.h"

#define MAX_BASES 256

enum {
    KIND_NONE,
    KIND_LAND,
    KIND_SKIP, /* an object the game didn't create (pickup types 7 and 9): its rotation and scale are skipped */
    KIND_WATER,
    KIND_PLATFORM,
    KIND_ENEMY,
    KIND_PICKUP,
};

typedef struct Base {
    Actor *actor;
    Platform *platform;
    s32 kind;
    s32 hasRot;
    Vec3 euler;
    Quat rot;
    Vec3 scale;
} Base;

void ParseLandscapeFile(s32 num);
void Mod_LoadExtraBanks(s32 level);
s32 Mod_SlotLevel(s32 num);
s32 ParseTokenGeneric(s16 token, u8 **data);
void DrawActor(Actor *ptr);
void RestartEnemy(Enemy *enemy, f32 xpos, f32 ypos, f32 zpos, s32 tag, f32 initYRot, s32 enemyType);
void GetQuaternionFromXYZRot(Quat *quat, Vec3 *rot);
void QuaternionMultiply(Quat *dest, Quat *src1, Quat *src2);

extern s32 numberOfEnemies;
extern Enemy enemies[];
extern CollectableList collectableList;
extern Quat zeroQuat;

static Base bases[MAX_BASES];
static s32 numBases;
static s32 pendingKind;
static s32 pickupCount;
static s32 curKind;
static Actor *curActor;
static Platform *curPlatform;
static Base *curBase;

static Base *FindBase(Actor *actor) {
    s32 i;

    for (i = 0; i < numBases; i++) {
        if (bases[i].actor == actor) {
            return &bases[i];
        }
    }
    return NULL;
}

static Base *NewBase(Actor *actor, s32 kind) {
    Base *b = FindBase(actor);

    if (b == NULL) {
        if (numBases == MAX_BASES) {
            return NULL;
        }
        b = &bases[numBases++];
        b->actor = actor;
        b->platform = NULL;
        b->hasRot = FALSE;
        b->rot = zeroQuat;
        b->scale.x = b->scale.y = b->scale.z = 1.0f;
    }
    b->kind = kind;
    return b;
}

/* The object the previous token declared exists now: find it. */
static void ResolvePending(void) {
    Platform *platform;

    if (pendingKind == KIND_NONE) {
        return;
    }
    curKind = pendingKind;
    curActor = NULL;
    curPlatform = NULL;
    curBase = NULL;
    switch (pendingKind) {
        case KIND_WATER:
            if (waterList.count) {
                curActor = waterList.head.next->actor;
            }
            break;
        case KIND_PLATFORM:
            if (platformList.count) {
                platform = platformList.head.prev;
                curPlatform = platform;
                curActor = &platform->actor;
                curBase = NewBase(curActor, KIND_PLATFORM);
                if (curBase != NULL) {
                    curBase->platform = platform;
                }
            }
            break;
        case KIND_ENEMY:
            if (numberOfEnemies > 0) {
                curActor = &enemies[numberOfEnemies - 1].actor;
            }
            break;
        case KIND_PICKUP:
            /* Pickup types 7 and 9 aren't created (parse.c) */
            if (collectableList.count > pickupCount) {
                curActor = &collectableList.head.next->actor;
            }
            break;
    }
    if (curActor == NULL && curKind != KIND_LAND) {
        curKind = KIND_SKIP;
    }
    pendingKind = KIND_NONE;
}

static void ApplyRotation(Vec3 *v) {
    Base *b;

    switch (curKind) {
        case KIND_WATER:
            GetQuaternionFromXYZRot(&curActor->rotQuat, v);
            break;
        case KIND_PLATFORM:
            /* Set now: the collision that SetPlatformPos builds uses it. A spinning platform gets it added to its
               spin angles after parsing (FinishLevel). */
            if (curBase != NULL) {
                curBase->hasRot = TRUE;
                curBase->euler = *v;
            }
            if (curPlatform->motion == NULL) {
                GetQuaternionFromXYZRot(&curActor->rotQuat, v);
            }
            break;
        case KIND_ENEMY:
        case KIND_PICKUP:
            b = NewBase(curActor, curKind);
            if (b != NULL) {
                b->hasRot = TRUE;
                GetQuaternionFromXYZRot(&b->rot, v);
            }
            break;
    }
}

static void ApplyScale(Vec3 *v) {
    Base *b;

    switch (curKind) {
        case KIND_WATER:
        case KIND_PLATFORM:
            curActor->scale = *v;
            break;
        case KIND_ENEMY:
        case KIND_PICKUP:
            b = NewBase(curActor, curKind);
            if (b != NULL) {
                b->scale = *v;
            }
            break;
    }
}

s32 Mod_ParseTokenGeneric(s16 token, u8 **data) {
    Vec3 v;

    ResolvePending();
    if ((token == TOKEN_LANDROT || token == TOKEN_LANDSCALE) && curKind > KIND_LAND) {
        memcpy(&v, *data, sizeof(Vec3));
        *data += sizeof(Vec3);
        if (token == TOKEN_LANDROT) {
            ApplyRotation(&v);
        } else {
            ApplyScale(&v);
        }
        return TRUE;
    }
    switch (token) {
        case TOKEN_LANDSCAPE:
        case TOKEN_BACKGROUND:
        case TOKEN_CAMEO_OBJ:
            pendingKind = KIND_LAND;
            break;
        case TOKEN_WATER:
            pendingKind = KIND_WATER;
            break;
        case TOKEN_PLATFORM:
        case TOKEN_DUMMY:
            pendingKind = KIND_PLATFORM;
            break;
        case TOKEN_ENEMY:
            pendingKind = KIND_ENEMY;
            break;
        case TOKEN_PICKUP:
            pendingKind = KIND_PICKUP;
            pickupCount = collectableList.count;
            break;
    }
    return ParseTokenGeneric(token, data);
}

/* The enemy's collision sphere and shadow follow its scale (the mean of the three axes). */
static void ScaleEnemyCollision(Enemy *enemy, Base *b) {
    f32 k = (ABS(b->scale.x) + ABS(b->scale.y) + ABS(b->scale.z)) / 3.0f;

    if (enemy->actor.obj == NULL) {
        return;
    }
    enemy->actor.obj->spheres[0].radius *= k;
    enemy->actor.obj->spheres[0].targetRadius *= k;
    if (enemy->actor.shadow != NULL) {
        enemy->actor.shadow->shadowSize *= k;
    }
}

static void FinishLevel(void) {
    s32 i;
    Base *b;
    PlatformMotion *motion;

    for (i = 0; i < numBases; i++) {
        b = &bases[i];
        if (b->kind == KIND_PLATFORM && b->hasRot && b->platform->motion != NULL) {
            motion = b->platform->motion;
            motion->rot.x += b->euler.x;
            motion->rot.y += b->euler.y;
            motion->rot.z += b->euler.z;
            GetQuaternionFromXYZRot(&b->actor->rotQuat, &motion->rot);
        } else if (b->kind == KIND_ENEMY) {
            ScaleEnemyCollision((Enemy *)b->actor, b);
        }
    }
}

void Mod_ParseLandscapeFile(s32 num) {
    Mod_LoadExtraBanks(Mod_SlotLevel(num)); /* an extra level (slots.c) has its own banks, keyed num + 0x40 */
    numBases = 0;
    pendingKind = KIND_NONE;
    curKind = KIND_NONE;
    curActor = NULL;
    curPlatform = NULL;
    curBase = NULL;
    ParseLandscapeFile(num);
    FinishLevel();
}

void Mod_RestartEnemy(Enemy *enemy, f32 xpos, f32 ypos, f32 zpos, s32 tag, f32 initYRot, s32 enemyType) {
    Base *b;

    RestartEnemy(enemy, xpos, ypos, zpos, tag, initYRot, enemyType);
    b = FindBase(&enemy->actor);
    if (b != NULL && b->kind == KIND_ENEMY) {
        ScaleEnemyCollision(enemy, b);
    }
}

void Mod_DrawActor(Actor *ptr) {
    Base *b;
    Quat q;
    Vec3 s;

    if (numBases == 0 || (ptr->type >= PLAT_ACTOR && ptr->type != COLLECTABLE_ACTOR)) {
        DrawActor(ptr);
        return;
    }
    b = FindBase(ptr);
    if (b == NULL || (b->kind != KIND_ENEMY && b->kind != KIND_PICKUP)) {
        DrawActor(ptr);
        return;
    }
    q = ptr->rotQuat;
    s = ptr->scale;
    /* rotQuat is GetQuaternionFromXYZRot's qY*qZ*qX; multiplying the base on the right applies it first, in the
       actor's own axes, as SetActorRotation does for landscape actors */
    if (b->hasRot) {
        QuaternionMultiply(&ptr->rotQuat, &q, &b->rot);
    }
    ptr->scale.x *= b->scale.x;
    ptr->scale.y *= b->scale.y;
    ptr->scale.z *= b->scale.z;
    DrawActor(ptr);
    ptr->rotQuat = q;
    ptr->scale = s;
}
