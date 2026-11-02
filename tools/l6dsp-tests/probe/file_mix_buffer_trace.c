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
static struct hit hits[128];
static unsigned used;
static uint64_t overflow, workspace_read;
static void memory(unsigned cpu, qemu_plugin_meminfo_t info, uint64_t addr, void *opaque) {
    (void)cpu;
    bool store = qemu_plugin_mem_is_store(info);
    if (store && (addr == 0x83e0a000 || addr == 0x83e0a020)) {
        qemu_plugin_mem_value v = qemu_plugin_mem_get_value(info);
        if (v.type == QEMU_PLUGIN_MEM_VALUE_U32 && addr == 0x83e0a000 && v.data.u32 == 0x4d495831 && !done) {
            active = true; fprintf(stderr, "MIX_TRACE_BEGIN before_read=%" PRIu64 " before_write=%" PRIu64 "\n", before_read, before_write);
        } else if (v.type == QEMU_PLUGIN_MEM_VALUE_U32 && addr == 0x83e0a020 && v.data.u32 == 0x444f4e45 && active) {
            active = false; done = true;
            for (unsigned i = 0; i < used; i++) fprintf(stderr, "MIX_TRACE_HIT pc=%08" PRIxPTR " read=%" PRIu64 " write=%" PRIu64 "\n", hits[i].pc, hits[i].read, hits[i].write);
            fprintf(stderr, "MIX_TRACE_END workspace_read=%" PRIu64 " overflow=%" PRIu64 "\n", workspace_read, overflow);
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
    if (i == used) { if (used == 128) { overflow++; return; } hits[used++].pc = pc; }
    if (store) hits[i].write++; else hits[i].read++;
}
static void translate(struct qemu_plugin_tb *tb, void *opaque) {
    (void)opaque;
    for (size_t i=0; i<qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn=qemu_plugin_tb_get_insn(tb,i);
        uintptr_t pc=qemu_plugin_insn_vaddr(insn);
        qemu_plugin_register_vcpu_mem_cb(insn,memory,QEMU_PLUGIN_CB_NO_REGS,QEMU_PLUGIN_MEM_RW,(void *)pc);
    }
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,const qemu_info_t *info,int argc,char **argv) {
    (void)argc; (void)argv; if (!info->system_emulation) return -1;
    qemu_plugin_register_vcpu_tb_trans_cb(id,translate,NULL); return 0;
}
