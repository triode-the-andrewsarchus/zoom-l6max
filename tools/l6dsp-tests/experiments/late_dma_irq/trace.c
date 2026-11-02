#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <qemu-plugin.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static bool active, done;
static uint64_t counts[5];
static const uint64_t addresses[] = {0x80024b70,0x80026658,0x80043af8,0x8001f360,0x800149b0};
static void instruction(unsigned int cpu, void *opaque) {
    (void)cpu; if (active) counts[(uintptr_t)opaque]++;
}
static void memory(unsigned int cpu, qemu_plugin_meminfo_t info, uint64_t addr, void *opaque) {
    (void)cpu; (void)opaque;
    if (addr != 0x83e0e000 || !qemu_plugin_mem_is_store(info)) return;
    qemu_plugin_mem_value v = qemu_plugin_mem_get_value(info);
    if (v.type != QEMU_PLUGIN_MEM_VALUE_U32) return;
    if (v.data.u32 == 0x4254474e && !done) { active = true; fprintf(stderr,"IRQ_PROFILE_BEGIN\n"); }
    if (v.data.u32 == 0x454e4421 && active) {
        active=false; done=true;
        for (unsigned i=0; i<5; i++) fprintf(stderr,"IRQ_PC %08lx %lu\n",(unsigned long)addresses[i],(unsigned long)counts[i]);
        fprintf(stderr,"IRQ_PROFILE_END\n");
    }
}
static void translate(struct qemu_plugin_tb *tb, void *opaque) {
    (void)opaque;
    for (size_t i=0; i<qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn=qemu_plugin_tb_get_insn(tb,i);
        uint64_t pc=qemu_plugin_insn_vaddr(insn);
        for (unsigned j=0;j<5;j++) if (pc==addresses[j])
            qemu_plugin_register_vcpu_insn_exec_cb(insn,instruction,QEMU_PLUGIN_CB_NO_REGS,(void *)(uintptr_t)j);
        if (pc>=0x80108000 && pc<0x80118000)
            qemu_plugin_register_vcpu_mem_cb(insn,memory,QEMU_PLUGIN_CB_NO_REGS,QEMU_PLUGIN_MEM_W,NULL);
    }
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,const qemu_info_t *info,int argc,char **argv) {
    (void)argc;(void)argv;if (!info->system_emulation) return -1;
    qemu_plugin_register_vcpu_tb_trans_cb(id,translate,NULL);return 0;
}
