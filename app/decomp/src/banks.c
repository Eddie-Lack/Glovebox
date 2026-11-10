/* LoadObjectBank and LoadTextureBank from gloveworld's src/banks.c (2de3e72), for tools/game_banks.py. */

void LoadObjectBank(u32 num) {
    s32 ptr;
    u32 start;
    u32 end;
    s32 size;
    s16 slot = -1;
    s16 i = 0;
    s32 rom;
    ObjectInstance *inst;

    switch (num) {
        case 71:
            start = (u32)tweenObjBankRomStart;
            end = (u32)ootwBonusObjBankRomStart;
            sprintf(message, "TWEEN");
            break;
        case 0:
            start = (u32)systemObjBankRomStart;
            end = (u32)presentationObjBankRomStart;
            sprintf(message, "SYSTEM");
            break;
        case 67:
            start = (u32)wayroomObjBankRomStart;
            end = (u32)hubPart2ObjBankRomStart;
            sprintf(message, "WAYROOM");
            break;
        case 66:
            start = (u32)outroObjBankRomStart;
            end = (u32)systemObjBankRomStart;
            sprintf(message, "OUTRO");
            break;
        case 68:
            start = (u32)presentationObjBankRomStart;
            end = (u32)flythruObjBankRomStart;
            sprintf(message, "PRESENTATION");
            break;
        case 69:
            start = (u32)flythruObjBankRomStart;
            end = (u32)landscapeFilesRomStart;
            sprintf(message, "FLYTHRU");
            break;
        case 65:
            start = (u32)introObjBankRomStart;
            end = (u32)wayroomObjBankRomStart;
            sprintf(message, "INTRO");
            break;
        case 64:
            start = (u32)hubSharedObjBankRomStart;
            end = (u32)hubPart1ObjBankRomStart;
            sprintf(message, "HUB_SHARED");
            break;
        case 2:
            start = (u32)hubPart1ObjBankRomStart;
            end = (u32)piratesL3bObjBankRomStart;
            sprintf(message, "HUB_PART1");
            break;
        case 3:
            start = (u32)hubPart2ObjBankRomStart;
            end = (u32)hubPart3ObjBankRomStart;
            sprintf(message, "HUB_PART2");
            break;
        case 4:
            start = (u32)hubPart3ObjBankRomStart;
            end = (u32)hubPart4ObjBankRomStart;
            sprintf(message, "HUB_PART3");
            break;
        case 5:
            start = (u32)hubPart4ObjBankRomStart;
            end = (u32)hubPart5ObjBankRomStart;
            sprintf(message, "HUB_PART4");
            break;
        case 6:
            start = (u32)hubPart5ObjBankRomStart;
            end = (u32)hubPart6ObjBankRomStart;
            sprintf(message, "HUB_PART5");
            break;
        case 7:
            start = (u32)hubPart6ObjBankRomStart;
            end = (u32)hubPart7ObjBankRomStart;
            sprintf(message, "HUB_PART6");
            break;
        case 8:
            start = (u32)hubPart7ObjBankRomStart;
            end = (u32)hubPart8ObjBankRomStart;
            sprintf(message, "HUB_PART7");
            break;
        case 9:
            start = (u32)hubPart8ObjBankRomStart;
            end = (u32)assaultCourseObjBankRomStart;
            sprintf(message, "HUB_PART8");
            break;
        case 10:
            start = (u32)caveObjBankRomStart;
            end = (u32)introObjBankRomStart;
            sprintf(message, "CAVE");
            break;
        case 11:
            start = (u32)assaultCourseObjBankRomStart;
            end = (u32)titleObjBankRomStart;
            sprintf(message, "ASSAULT COURSE");
            break;
        case 1:
            start = (u32)genericObjBankRomStart;
            end = (u32)atlantisSharedObjBankRomStart;
            sprintf(message, "GENERIC");
            break;
        case 12:
            start = (u32)atlantisSharedObjBankRomStart;
            end = (u32)atlantisL1ObjBankRomStart;
            sprintf(message, "ATLANTIS_SHARED");
            break;
        case 13:
            start = (u32)atlantisL1ObjBankRomStart;
            end = (u32)atlantisL2ObjBankRomStart;
            sprintf(message, "ATLANTIS_L1");
            break;
        case 14:
            start = (u32)atlantisL2ObjBankRomStart;
            end = (u32)atlantisL3aObjBankRomStart;
            sprintf(message, "ATLANTIS_L2");
            break;
        case 15:
            start = (u32)atlantisL3aObjBankRomStart;
            end = (u32)atlantisBossObjBankRomStart;
            sprintf(message, "ATLANTIS_L3A");
            break;
        case 17:
            start = (u32)atlantisBossObjBankRomStart;
            end = (u32)atlantisBonusObjBankRomStart;
            sprintf(message, "ATLANTIS_BOSS");
            break;
        case 18:
            start = (u32)atlantisBonusObjBankRomStart;
            end = (u32)carnivalSharedObjBankRomStart;
            sprintf(message, "ATLANTIS_BONUS");
            break;
        case 19:
            start = (u32)carnivalSharedObjBankRomStart;
            end = (u32)carnivalL1ObjBankRomStart;
            sprintf(message, "CARNIVAL_SHARED");
            break;
        case 20:
            start = (u32)carnivalL1ObjBankRomStart;
            end = (u32)carnivalL2aObjBankRomStart;
            sprintf(message, "CARNIVAL_L1");
            break;
        case 21:
            start = (u32)carnivalL2aObjBankRomStart;
            end = (u32)carnivalL3aObjBankRomStart;
            sprintf(message, "CARNIVAL_L2A");
            break;
        case 23:
            start = (u32)carnivalL3aObjBankRomStart;
            end = (u32)carnivalBossObjBankRomStart;
            sprintf(message, "CARNIVAL_L3A");
            break;
        case 25:
            start = (u32)carnivalBossObjBankRomStart;
            end = (u32)carnivalBonusObjBankRomStart;
            sprintf(message, "CARNIVAL_BOSS");
            break;
        case 26:
            start = (u32)carnivalBonusObjBankRomStart;
            end = (u32)piratesSharedObjBankRomStart;
            sprintf(message, "CARNIVAL_BONUS");
            break;
        case 38:
            start = (u32)piratesSharedObjBankRomStart;
            end = (u32)piratesL1ObjBankRomStart;
            sprintf(message, "PIRATES_SHARED");
            break;
        case 39:
            start = (u32)piratesL1ObjBankRomStart;
            end = (u32)piratesL2aObjBankRomStart;
            sprintf(message, "PIRATES_L1");
            break;
        case 40:
            start = (u32)piratesL2aObjBankRomStart;
            end = (u32)piratesBossObjBankRomStart;
            sprintf(message, "PIRATES_L2A");
            break;
        case 43:
            start = (u32)piratesL3bObjBankRomStart;
            end = (u32)prehistoricL3bObjBankRomStart;
            sprintf(message, "PIRATES_L3B");
            break;
        case 44:
            start = (u32)piratesBossObjBankRomStart;
            end = (u32)piratesBonusObjBankRomStart;
            sprintf(message, "PIRATES_BOSS");
            break;
        case 45:
            start = (u32)piratesBonusObjBankRomStart;
            end = (u32)prehistoricSharedObjBankRomStart;
            sprintf(message, "PIRATES_BONUS");
            break;
        case 46:
            start = (u32)prehistoricSharedObjBankRomStart;
            end = (u32)prehistoricL1aObjBankRomStart;
            sprintf(message, "PREHISTORIC_SHARED");
            break;
        case 47:
            start = (u32)prehistoricL1aObjBankRomStart;
            end = (u32)prehistoricL2aObjBankRomStart;
            sprintf(message, "PREHISTORIC_L1A");
            break;
        case 49:
            start = (u32)prehistoricL2aObjBankRomStart;
            end = (u32)prehistoricBossObjBankRomStart;
            sprintf(message, "PREHISTORIC_L2A");
            break;
        case 52:
            start = (u32)prehistoricL3bObjBankRomStart;
            end = (u32)ootwL2aObjBankRomStart;
            sprintf(message, "PREHISTORIC_L3B");
            break;
        case 53:
            start = (u32)prehistoricBossObjBankRomStart;
            end = (u32)prehistoricBonusObjBankRomStart;
            sprintf(message, "PREHISTORIC_BOSS");
            break;
        case 54:
            start = (u32)prehistoricBonusObjBankRomStart;
            end = (u32)fortressSharedObjBankRomStart;
            sprintf(message, "PREHISTORIC_BONUS");
            break;
        case 55:
            start = (u32)fortressSharedObjBankRomStart;
            end = (u32)fortressL1aObjBankRomStart;
            sprintf(message, "FORTRESS_SHARED");
            break;
        case 56:
            start = (u32)fortressL1aObjBankRomStart;
            end = (u32)fortressL2aObjBankRomStart;
            sprintf(message, "FORTRESS_L1A");
            break;
        case 58:
            start = (u32)fortressL2aObjBankRomStart;
            end = (u32)fortressL3bObjBankRomStart;
            sprintf(message, "FORTRESS_L2A");
            break;
        case 61:
            start = (u32)fortressL3bObjBankRomStart;
            end = (u32)fortressBonusObjBankRomStart;
            sprintf(message, "FORTRESS_L3B");
            break;
        case 63:
            start = (u32)fortressBonusObjBankRomStart;
            end = (u32)fortressBossObjBankRomStart;
            sprintf(message, "FORTRESS_BONUS");
            break;
        case 62:
            start = (u32)fortressBossObjBankRomStart;
            end = (u32)hubSharedObjBankRomStart;
            sprintf(message, "FORTRESS_BOSS");
            break;
        case 27:
            start = (u32)ootwSharedObjBankRomStart;
            end = (u32)caveObjBankRomStart;
            sprintf(message, "OOTW_SHARED");
            break;
        case 29:
            start = (u32)ootwL2aObjBankRomStart;
            end = (u32)ootwL2bObjBankRomStart;
            sprintf(message, "OOTW_L2A");
            break;
        case 30:
            start = (u32)ootwL2bObjBankRomStart;
            end = (u32)ootwL3ObjBankRomStart;
            sprintf(message, "OOTW_L2B");
            break;
        case 31:
            start = (u32)ootwL3ObjBankRomStart;
            end = (u32)ootwBoss1ObjBankRomStart;
            sprintf(message, "OOTW_L3");
            break;
        case 33:
            start = (u32)ootwBoss1ObjBankRomStart;
            end = (u32)tweenObjBankRomStart;
            sprintf(message, "OOTW_BOSS1");
            break;
        case 35:
            start = (u32)ootwBonusObjBankRomStart;
            end = (u32)ootwSharedObjBankRomStart;
            sprintf(message, "OOTW_BONUS");
            break;
        case 70:
            start = (u32)titleObjBankRomStart;
            end = (u32)outroObjBankRomStart;
            sprintf(message, "TITLE_BANK");
            break;
        default:
            PRINTF("ERROR: no object bank specified....\n");
            for (;;) {
            }
            break;
    }
    size = end - start;
    ptr = (s32)JallocAlloc(DMAGetSize(start, end), 0, "ObjBank");
    DMAMemory((void *)ptr, start, end);
    if (ptr != 0) {
        PRINTF("Loaded object bank %s (size %d)\n", message, size);
    } else {
        PRINTF("Unable to load object bank %s\n", message);
        return;
    }
    while (objectBanks[++slot].data) {
    }
    if (slot >= 5) {
        PRINTF("ERROR:too many object banks\n");
        return;
    }
    objectBanks[slot].data = (void *)ptr;
    objectBanks[slot].entries = (ObjectBankEntry *)ptr;
    for (i = 0; objectBanks[slot].entries[i].inst; i++) {
        rom = (s32)objectBanks[slot].entries[i].inst;
        objectBanks[slot].entries[i].inst = (ObjectInstance *)Rom2Ram(rom, ptr);
        inst = objectBanks[slot].entries[i].inst;
        inst->obj = (Object *)Rom2Ram((s32)inst->obj, ptr);
        if (inst->anim) {
            inst->anim = (AnimState *)Rom2Ram((s32)inst->anim, ptr);
            if (inst->anim->anims) {
                inst->anim->anims = (struct AnimInfo *)Rom2Ram((s32)inst->anim->anims, ptr);
            }
        }
        RestoreObjectPointers(inst->obj, ptr);
    }
    objectBanks[slot].count = i;
    if (num == 0) {
        InitFadeOverlay();
    }
}

void LoadTextureBank(u32 num) {
    u32 *ptr;
    u32 start;
    u32 end;
    s32 size;
    s16 slot = -1;
    s16 i = 0;
    Texture *tex;
    Texture *cur;

    switch (num) {
        case 18:
            start = (u32)rcDemoTexBankRomStart;
            end = (u32)genericObjBankRomStart;
            sprintf(message, "RCDEMO_TEX_BANK");
            break;
        case CKBONUS_TEX_BANK:
            start = (u32)ckBonusTexBankRomStart;
            end = (u32)ckCheat1TexBankRomStart;
            sprintf(message, "CKBONUS_TEX_BANK");
            break;
        case 16:
            start = (u32)ckCheat1TexBankRomStart;
            end = (u32)ckCheat2TexBankRomStart;
            sprintf(message, "CKCHEAT1_TEX_BANK");
            break;
        case 17:
            start = (u32)ckCheat2TexBankRomStart;
            end = (u32)ckCheat2TexBankRomEnd;
            sprintf(message, "CKCHEAT2_TEX_BANK");
            break;
        case FONT_TEX_BANK:
            start = (u32)fontTexBankRomStart;
            end = (u32)cameoTexBankRomStart;
            sprintf(message, "FONT_TEX_BANK");
            slot = 0;
            break;
        case HUB_TEX_BANK:
            start = (u32)hubTexBankRomStart;
            end = (u32)levelEndTexBankRomStart;
            sprintf(message, "HUB_TEX_BANK");
            break;
        case CAMEO_TEX_BANK:
            start = (u32)cameoTexBankRomStart;
            end = (u32)levelEnd2TexBankRomStart;
            sprintf(message, "CAMEO_TEX_BANK");
            break;
        case TITLE_TEX_BANK:
            start = (u32)titleTexBankRomStart;
            end = (u32)genericTexBankRomStart;
            sprintf(message, "TITLE_TEX_BANK");
            break;
        case LEVEND1_TEX_BANK:
            start = (u32)levelEndTexBankRomStart;
            end = (u32)fontTexBankRomStart;
            sprintf(message, "LEVELEND_TEX_BANK");
            break;
        case LEVEND2_TEX_BANK:
            start = (u32)levelEnd2TexBankRomStart;
            end = (u32)presentTexBankRomStart;
            sprintf(message, "LEVELEND_TEX_BANK");
            break;
        case GENERIC_TEX_BANK:
            start = (u32)genericTexBankRomStart;
            end = (u32)atlantisTexBankRomStart;
            sprintf(message, "GENERIC_TEX_BANK");
            break;
        case ATLANTIS_TEX_BANK:
            start = (u32)atlantisTexBankRomStart;
            end = (u32)carnivalTexBankRomStart;
            sprintf(message, "ATLANTIS_TEX_BANK");
            break;
        case CARNIVAL_TEX_BANK:
            start = (u32)carnivalTexBankRomStart;
            end = (u32)piratesTexBankRomStart;
            sprintf(message, "CARNIVAL_TEX_BANK");
            break;
        case PIRATES_TEX_BANK:
            start = (u32)piratesTexBankRomStart;
            end = (u32)prehistoricTexBankRomStart;
            sprintf(message, "PIRATES_TEX_BANK");
            break;
        case PREHISTORIC_TEX_BANK:
            start = (u32)prehistoricTexBankRomStart;
            end = (u32)fortressTexBankRomStart;
            sprintf(message, "PREHISTORIC_TEX_BANK");
            break;
        case FORTRESS_TEX_BANK:
            start = (u32)fortressTexBankRomStart;
            end = (u32)ootwTexBankRomStart;
            sprintf(message, "FORTRESS_TEX_BANK");
            break;
        case OOTW_TEX_BANK:
            start = (u32)ootwTexBankRomStart;
            end = (u32)hubTexBankRomStart;
            sprintf(message, "OOTW_TEX_BANK");
            break;
        case PRESENT_TEX_BANK:
            start = (u32)presentTexBankRomStart;
            end = (u32)flythruTexBankRomStart;
            sprintf(message, "PRESENT_TEX_BANK");
            break;
        case FLYTHRU_TEX_BANK:
            start = (u32)flythruTexBankRomStart;
            end = (u32)ckBonusTexBankRomStart;
            sprintf(message, "FLYTHRU_TEX_BANK");
            break;
        default:
            PRINTF("Invalid texture bank!\n");
            Crash("LOAD TEXTURE BANK");
            break;
    }
    size = end - start;
    if (num != FONT_TEX_BANK) {
        ptr = JallocAlloc(DMAGetSize(start, end), 0, "TexBank");
    } else {
        ptr = fontTexBankBuffer;
    }
    DMAMemory(ptr, start, end);
    if (ptr != NULL) {
        PRINTF("Loaded texture bank %s [%d]\n", message, size);
    } else {
        PRINTF("Unable to load texture bank %s\n", message);
        return;
    }
    if (num != FONT_TEX_BANK) {
        while (textureBanks[++slot].data) {
        }
        if (slot >= 5) {
            PRINTF("ERROR:too many texture banks downloaded\n");
            return;
        }
    }
    textureBanks[slot].data = (s32)ptr;
    textureBanks[slot].count = *ptr;
    ptr += 1;
    textureBanks[slot].textures = (s32)ptr;
    cur = (Texture *)textureBanks[slot].textures;
    for (i = 0; i < textureBanks[slot].count; i++) {
        tex = cur;
        tex->data = (u8 *)tex + sizeof(Texture);
        if (tex->palette) {
            tex->palette = (s32 *)((u32)tex->palette + (u32)tex);
        }
        cur = (Texture *)((u8 *)cur + tex->size);
        if (tex->flags & 4) {
            cyclingTextures[numCyclingTextures++] = tex;
        }
    }
}
