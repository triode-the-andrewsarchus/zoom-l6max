#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <qemu-plugin.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static void observe(unsigned int cpu, void *data) {
    fprintf(stderr, "CLEAR_PATH cpu=%u pc=%08" PRIxPTR "\n", cpu, (uintptr_t)data);
}
static void translate(struct qemu_plugin_tb *tb, void *data) {
    (void)data;
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); ++i) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        if (pc == 0x80085176 || pc == 0x8001001e || pc == 0x80003e88 ||
            pc == 0x80091fa8 || pc == 0x20280718 || pc == 0x20280750 ||
            pc == 0x20280672 || pc == 0x202806c0 || pc == 0x80024678)
            qemu_plugin_register_vcpu_insn_exec_cb(insn, observe, QEMU_PLUGIN_CB_NO_REGS, (void *)(uintptr_t)pc);
    }
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info, int argc, char **argv) {
    (void)argc; (void)argv;
    if (!info->system_emulation) return -1;
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    return 0;
}
