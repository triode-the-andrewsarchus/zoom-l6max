// Stock L6max 1.10: observe CPU accesses overlapping the word-copy slots.
// Instrumentation perturbs timing and does not observe DMA.
#include <qemu-plugin.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdint.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
struct hit { uintptr_t pc; uint64_t address, count, words, nonzero; };
static struct hit hits[256];
static unsigned used;
static uint64_t overflow, watched_reads;
static void memory(unsigned cpu, qemu_plugin_meminfo_t info, uint64_t addr, void *opaque) {
    (void)cpu;
    uint64_t size = UINT64_C(1) << qemu_plugin_mem_size_shift(info);
    if (addr >= 0x80464fd0 || addr + size <= 0x80464fc8) return;
    if (!qemu_plugin_mem_is_store(info)) { watched_reads++; return; }
    uintptr_t pc = (uintptr_t)opaque;
    unsigned i;
    for (i=0; i<used && (hits[i].pc!=pc || hits[i].address!=addr); i++);
    if (i==used) { if (used==256) { overflow++; return; } hits[used++] = (struct hit){.pc=pc,.address=addr}; }
    hits[i].count++;
    qemu_plugin_mem_value value = qemu_plugin_mem_get_value(info);
    if (value.type == QEMU_PLUGIN_MEM_VALUE_U32) {
        hits[i].words++;
        if (value.data.u32 != 0) hits[i].nonzero++;
    }
}
static void translate(struct qemu_plugin_tb *tb, void *opaque) {
    (void)opaque;
    for (size_t i=0; i<qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn=qemu_plugin_tb_get_insn(tb,i);
        qemu_plugin_register_vcpu_mem_cb(insn,memory,QEMU_PLUGIN_CB_NO_REGS,QEMU_PLUGIN_MEM_RW,(void *)(uintptr_t)qemu_plugin_insn_vaddr(insn));
    }
}
static void finish(void *opaque) {
    (void)opaque;
    for (unsigned i=0; i<used; i++) fprintf(stderr,"DEFERRED_WRITE pc=%08" PRIxPTR " address=%08" PRIx64 " count=%" PRIu64 " words=%" PRIu64 " nonzero32=%" PRIu64 "\n",hits[i].pc,hits[i].address,hits[i].count,hits[i].words,hits[i].nonzero);
    fprintf(stderr,"DEFERRED_DONE reads=%" PRIu64 " overflow=%" PRIu64 "\n",watched_reads,overflow);
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,const qemu_info_t *info,int argc,char **argv) {
    (void)argc; (void)argv; if (!info->system_emulation) return -1;
    qemu_plugin_register_vcpu_tb_trans_cb(id,translate,NULL);
    qemu_plugin_register_atexit_cb(id,finish,NULL);
    return 0;
}
