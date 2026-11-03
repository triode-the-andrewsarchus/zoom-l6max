#include <inttypes.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <qemu-plugin.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static bool selector, task;
struct site { uint64_t pc; bool selector; };

static uint32_t word(const uint8_t *p) {
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}

static void observe_task(unsigned int cpu, uint64_t pc) {
    GByteArray *memory = g_byte_array_new();
    uint32_t tcb = 0;
    if (!qemu_plugin_read_memory_vaddr(0x808a3af0, memory, 4) || memory->len < 4) goto error;
    tcb = word(memory->data);
    if (!qemu_plugin_read_memory_vaddr(tcb, memory, 100) || memory->len < 100) goto error;
    char name[49] = {0};
    memcpy(name, memory->data + 52, 48);
    for (size_t i = 0; i < 48 && name[i]; i++)
        if (name[i] < 32 || name[i] > 126) name[i] = '?';
    fprintf(stderr, "CLEAR_TASK cpu=%u pc=%08" PRIx64 " tcb=%08" PRIx32 " priority=%" PRIu32 " task=%s\n",
            cpu, pc, tcb, word(memory->data + 44), name);
    g_byte_array_unref(memory);
    return;
error:
    fprintf(stderr, "CLEAR_TASK cpu=%u pc=%08" PRIx64 " read_error tcb=%08" PRIx32 "\n", cpu, pc, tcb);
    g_byte_array_unref(memory);
}

static void observe(unsigned int cpu, void *data) {
    const struct site *site = data;
    fprintf(stderr, "CLEAR_PATH cpu=%u pc=%08" PRIx64 "\n", cpu, site->pc);
    if (task && site->selector) observe_task(cpu, site->pc);
}
static void translate(struct qemu_plugin_tb *tb, void *data) {
    (void)data;
    static const struct site sites[] = {
        {0x80085176, false}, {0x8001001e, false}, {0x80003e88, false},
        {0x80091fa8, false}, {0x20280718, false}, {0x20280750, false},
        {0x20280672, false}, {0x202806c0, false}, {0x80024678, false},
        {0x80051278, true}, {0x8005292c, true}, {0x8005294e, true},
        {0x80088dd8, true}, {0x80088de6, true}, {0x800542a8, true},
        {0x800542ee, true}, {0x80054340, true}, {0x800543a2, true},
        {0x20280580, true},
    };
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); ++i) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        for (size_t j = 0; j < sizeof(sites) / sizeof(sites[0]); ++j)
            if (pc == sites[j].pc && (!sites[j].selector || selector)) {
                qemu_plugin_register_vcpu_insn_exec_cb(insn, observe,
                    task ? QEMU_PLUGIN_CB_R_REGS : QEMU_PLUGIN_CB_NO_REGS, (void *)&sites[j]);
                break;
            }
    }
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info, int argc, char **argv) {
    if (!info->system_emulation || argc > 2) return -1;
    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "selector=on") == 0 && !selector) selector = true;
        else if (strcmp(argv[i], "task=on") == 0 && !task) task = true;
        else return -1;
    }
    if (task && !selector) return -1;
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    return 0;
}
