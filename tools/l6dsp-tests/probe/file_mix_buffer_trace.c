// QEMU-only CPU memory trace for the L6max 1.10 recorded-project probe.
// The marker and workspace addresses belong to that diagnostic, not stock ABI.
#include <inttypes.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <qemu-plugin.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static bool active, done;
static uint64_t before_read, before_write;
struct hit { uintptr_t pc; uint64_t read, write; };
static struct hit hits[512];
static unsigned used;
static uint64_t overflow, workspace_read;
static bool live_bypass;
static uint64_t bypass_windows, helpers[2][4];
static const uint64_t helper_pc[] = {0x8002bb18, 0x8002bba8, 0x80066180, 0x800664f0};
static void helper(unsigned cpu, void *opaque) {
    (void)cpu;
    if (active) helpers[live_bypass ? 1 : 0][(uintptr_t)opaque]++;
}
static void memory(unsigned cpu, qemu_plugin_meminfo_t info, uint64_t addr, void *opaque) {
    (void)cpu;
    bool store = qemu_plugin_mem_is_store(info);
    if (store && (addr == 0x83e0a000 || addr == 0x83e0a020 || addr == 0x83e0b820)) {
        qemu_plugin_mem_value v = qemu_plugin_mem_get_value(info);
        if (v.type == QEMU_PLUGIN_MEM_VALUE_U32 && addr == 0x83e0a000 && v.data.u32 == 0x4d495831 && !done) {
            active = true; fprintf(stderr, "MIX_TRACE_BEGIN before_read=%" PRIu64 " before_write=%" PRIu64 "\n", before_read, before_write);
        } else if (v.type == QEMU_PLUGIN_MEM_VALUE_U32 && addr == 0x83e0a020 && v.data.u32 == 0x444f4e45 && active) {
            active = false; done = true;
            for (unsigned i = 0; i < used; i++) fprintf(stderr, "MIX_TRACE_HIT pc=%08" PRIxPTR " read=%" PRIu64 " write=%" PRIu64 "\n", hits[i].pc, hits[i].read, hits[i].write);
            for (unsigned i = 0; i < 4; i++) fprintf(stderr, "MIX_TRACE_HELPER pc=%08" PRIx64 " mix=%" PRIu64 " bypass=%" PRIu64 "\n", helper_pc[i], helpers[0][i], helpers[1][i]);
            fprintf(stderr, "MIX_TRACE_BYPASS windows=%" PRIu64 " active=%u\n", bypass_windows, live_bypass);
            fprintf(stderr, "MIX_TRACE_END workspace_read=%" PRIu64 " overflow=%" PRIu64 "\n", workspace_read, overflow);
        } else if (v.type == QEMU_PLUGIN_MEM_VALUE_U32 && addr == 0x83e0b820 && active) {
            if (v.data.u32 == 2) { live_bypass = true; bypass_windows++; }
            else if (v.data.u32 == 0) live_bypass = false;
        }
    }
    unsigned shift = qemu_plugin_mem_size_shift(info);
    uint64_t size = UINT64_C(1) << shift;
    // Positive control: the diagnostic callback reads the twelve track arrays.
    if (active && !store && addr >= 0x81570400 && addr < 0x8206cc00)
        workspace_read++;
    if (addr >= 0x20010410 || addr + size <= 0x20010210) return;
    if (!active) { if (!done) { if (store) before_write++; else before_read++; } return; }
    uintptr_t pc = (uintptr_t)opaque;
    unsigned i;
    for (i=0; i<used && hits[i].pc != pc; i++);
    if (i == used) { if (used == 512) { overflow++; return; } hits[used++].pc = pc; }
    if (store) hits[i].write++; else hits[i].read++;
}
static void translate(struct qemu_plugin_tb *tb, void *opaque) {
    (void)opaque;
    for (size_t i=0; i<qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn=qemu_plugin_tb_get_insn(tb,i);
        uintptr_t pc=qemu_plugin_insn_vaddr(insn);
        qemu_plugin_register_vcpu_mem_cb(insn,memory,QEMU_PLUGIN_CB_NO_REGS,QEMU_PLUGIN_MEM_RW,(void *)pc);
        for (unsigned j=0; j<4; j++) if (pc == helper_pc[j])
            qemu_plugin_register_vcpu_insn_exec_cb(insn,helper,QEMU_PLUGIN_CB_NO_REGS,(void *)(uintptr_t)j);
    }
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,const qemu_info_t *info,int argc,char **argv) {
    (void)argc; (void)argv; if (!info->system_emulation) return -1;
    qemu_plugin_register_vcpu_tb_trans_cb(id,translate,NULL); return 0;
}
