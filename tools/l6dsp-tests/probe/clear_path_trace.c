#include <inttypes.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <qemu-plugin.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static bool selector;
static void observe(unsigned int cpu, void *data) {
    fprintf(stderr, "CLEAR_PATH cpu=%u pc=%08" PRIxPTR "\n", cpu, (uintptr_t)data);
}
static void translate(struct qemu_plugin_tb *tb, void *data) {
    (void)data;
    static const uint64_t pcs[] = {
        0x80085176, 0x8001001e, 0x80003e88, 0x80091fa8, 0x20280718,
        0x20280750, 0x20280672, 0x202806c0, 0x80024678,
        // Remaining entries are enabled only by selector=on.
        0x80051278, 0x8005292c, 0x8005294e, 0x80088dd8, 0x80088de6,
        0x800542a8, 0x800542ee, 0x80054340, 0x800543a2, 0x20280580,
    };
    const size_t count = selector ? sizeof(pcs) / sizeof(pcs[0]) : 9;
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); ++i) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        for (size_t j = 0; j < count; ++j)
            if (pc == pcs[j]) {
                qemu_plugin_register_vcpu_insn_exec_cb(insn, observe, QEMU_PLUGIN_CB_NO_REGS, (void *)(uintptr_t)pc);
                break;
            }
    }
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info, int argc, char **argv) {
    if (!info->system_emulation || argc > 1 || (argc == 1 && strcmp(argv[0], "selector=on") != 0)) return -1;
    selector = argc == 1;
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    return 0;
}
