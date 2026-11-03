// QEMU-only single-vCPU trace. Default mode watches copied AINR instructions.
// allcpu=on watches every CPU instruction during marker-bracketed solo calls.
// Neither mode observes DMA, debugger writes, or unmonitored execution paths.
#include <inttypes.h>
#include <qemu-plugin.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static bool allcpu;
static unsigned active;
static uint64_t starts, ends, invalid, ignored, controls[4];
static uint64_t counts[2][4][2], first_pc[2][4][2], min_addr[2][4], max_addr[2][4];
static uint64_t pcs[128], pc_counts[128][2], pc_overflow;

static void marker(unsigned cpu, void *opaque) {
    (void)cpu;
    uintptr_t pc = (uintptr_t)opaque;
    if (pc == 0x80f1ff10) {
        if (active) invalid++;
        active = 1;
        starts++;
    } else {
        if (!active) invalid++;
        active = 0;
        ends++;
    }
}

static void record_access(unsigned actor, unsigned region, bool store,
                          uint64_t address, uint64_t end, uintptr_t pc) {
    counts[actor][region][store]++;
    if (!first_pc[actor][region][store]) first_pc[actor][region][store] = pc;
    if (!min_addr[actor][region] || address < min_addr[actor][region]) min_addr[actor][region] = address;
    if (end > max_addr[actor][region]) max_addr[actor][region] = end;
    if (allcpu && actor == 1 && region == 1) {
        unsigned i = 0;
        while (i < 128 && pcs[i] && pcs[i] != pc) i++;
        if (i == 128) pc_overflow++;
        else { pcs[i] = pc; pc_counts[i][store]++; }
    }
}

static void capture_memory(unsigned actor, uint64_t base, qemu_plugin_meminfo_t info,
                           uint64_t address, uintptr_t pc) {
    unsigned shift = qemu_plugin_mem_size_shift(info);
    if (shift >= 20) { ignored++; return; }
    uint64_t end = address + (UINT64_C(1) << shift);
    const uint64_t lo[] = {base, base + 0xb94c, base + 0x19514, 0x20280000};
    const uint64_t hi[] = {base + 0xb94c, base + 0x19514, base + 0x27000, 0x202a7000};
    for (unsigned i = 0; i < 4; i++)
        if (address < hi[i] && end > lo[i])
            record_access(actor, i, qemu_plugin_mem_is_store(info), address, end, pc);
}

static void clone_memory(unsigned cpu, qemu_plugin_meminfo_t info, uint64_t address, void *opaque) {
    (void)cpu;
    uintptr_t pc = (uintptr_t)opaque;
    if (pc >= 0x80f1ff00 && pc < 0x80f1ff10) {
        const uint64_t test[] = {0x80e00000, 0x80e20000, 0x80e30000, 0x80e50000};
        unsigned i = (pc - 0x80f1ff00) / 4;
        if (!qemu_plugin_mem_is_store(info) && address == test[i]) controls[i]++;
        return;
    }
    unsigned clone = pc >= 0x80f10000;
    capture_memory(clone, clone ? 0x80e30000 : 0x80e00000, info, address, pc);
}

static void allcpu_memory(unsigned cpu, qemu_plugin_meminfo_t info, uint64_t address, void *opaque) {
    (void)cpu;
    if (!active) return;
    uintptr_t pc = (uintptr_t)opaque;
    if (pc == 0x80f1ff18 || pc == 0x80f1ff1c) {
        const uint64_t test[] = {0x80e30000, 0x80e50000};
        unsigned i = (pc - 0x80f1ff18) / 4;
        if (!qemu_plugin_mem_is_store(info) && address == test[i]) controls[i]++;
        return;
    }
    unsigned actor = !(pc >= 0x80f10000 && pc < 0x80f15b4c);
    capture_memory(actor, 0x80e30000, info, address, pc);
}

static void translate(struct qemu_plugin_tb *tb, void *unused) {
    (void)unused;
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uintptr_t pc = qemu_plugin_insn_vaddr(insn);
        if (allcpu && (pc == 0x80f1ff10 || pc == 0x80f1ff14))
            qemu_plugin_register_vcpu_insn_exec_cb(insn, marker, QEMU_PLUGIN_CB_NO_REGS, (void *)pc);
        if (allcpu || (pc >= 0x80f00000 && pc < 0x80f05b4c) ||
            (pc >= 0x80f10000 && pc < 0x80f15b4c) || (pc >= 0x80f1ff00 && pc < 0x80f1ff10))
            qemu_plugin_register_vcpu_mem_cb(insn, allcpu ? allcpu_memory : clone_memory, QEMU_PLUGIN_CB_NO_REGS, QEMU_PLUGIN_MEM_RW, (void *)pc);
    }
}

static void finish(void *unused) {
    (void)unused;
    if (allcpu) {
        fprintf(stderr, "ALLCPU starts=%" PRIu64 " ends=%" PRIu64 " active=%u invalid=%" PRIu64
                        " positive=%" PRIu64 ",%" PRIu64 "\n", starts, ends, active, invalid, controls[0], controls[1]);
        for (unsigned a = 0; a < 2; a++) for (unsigned i = 0; i < 4; i++)
            fprintf(stderr, "ALLCPU actor=%u region=%u reads=%" PRIu64 " writes=%" PRIu64
                            " first_read=%" PRIx64 " first_write=%" PRIx64 " min=%" PRIx64 " end=%" PRIx64 "\n",
                    a, i, counts[a][i][0], counts[a][i][1], first_pc[a][i][0], first_pc[a][i][1], min_addr[a][i], max_addr[a][i]);
        fprintf(stderr, "ALLCPU pc_overflow=%" PRIu64 " ignored=%" PRIu64 "\n", pc_overflow, ignored);
        for (unsigned i = 0; i < 128 && pcs[i]; i++)
            fprintf(stderr, "ALLCPU external_pc=%" PRIx64 " reads=%" PRIu64 " writes=%" PRIu64 "\n", pcs[i], pc_counts[i][0], pc_counts[i][1]);
    } else {
        fprintf(stderr, "CLONE_TRACE positive=%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n", controls[0], controls[1], controls[2], controls[3]);
        for (unsigned a = 0; a < 2; a++) for (unsigned i = 0; i < 4; i++)
            fprintf(stderr, "CLONE_TRACE clone=%u region=%u reads=%" PRIu64 " writes=%" PRIu64 " min=%" PRIx64 " end=%" PRIx64 "\n",
                    a, i, counts[a][i][0], counts[a][i][1], min_addr[a][i], max_addr[a][i]);
    }
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info, int argc, char **argv) {
    if (!info->system_emulation || argc > 1 || (argc == 1 && strcmp(argv[0], "allcpu=on") != 0)) return -1;
    allcpu = argc == 1;
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    qemu_plugin_register_atexit_cb(id, finish, NULL);
    return 0;
}
