// QEMU-only fault injector for one selector admission test. Never use on hardware.
#include <errno.h>
#include <inttypes.h>
#include <qemu-plugin.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static uint64_t begin_pc, return_pc;
static uint32_t saved_ready;
static unsigned injected, restored, active, errors;

static bool read_word(uint64_t address, uint32_t *value) {
    GByteArray *bytes = g_byte_array_new();
    bool ok = qemu_plugin_read_memory_vaddr(address, bytes, 4) && bytes->len == 4;
    if (ok) *value = (uint32_t)bytes->data[0] | (uint32_t)bytes->data[1] << 8 |
                     (uint32_t)bytes->data[2] << 16 | (uint32_t)bytes->data[3] << 24;
    g_byte_array_unref(bytes);
    return ok;
}

static bool write_word(uint64_t address, uint32_t value) {
    uint8_t data[] = {value, value >> 8, value >> 16, value >> 24};
    GByteArray *bytes = g_byte_array_sized_new(sizeof(data));
    g_byte_array_append(bytes, data, sizeof(data));
    bool ok = qemu_plugin_write_memory_vaddr(address, bytes);
    g_byte_array_unref(bytes);
    return ok;
}

static void observe(unsigned cpu, void *opaque) {
    (void)cpu;
    uint64_t pc = (uintptr_t)opaque;
    uint32_t value;
    if (pc == begin_pc && !injected) {
        if (!read_word(0x81fff004, &value)) { errors++; return; }
        if (value != 0x4f564c50) return;
        if (!read_word(0x83e0b008, &saved_ready) || saved_ready != 0x47415445 ||
            !write_word(0x81fff004, 0) || !write_word(0x83e0b008, 0xdeadbeef)) {
            errors++;
            return;
        }
        active = 1;
        injected++;
        fprintf(stderr, "SELECTOR_REJECT injected ready=%08" PRIx32 "\n", saved_ready);
    } else if (pc == return_pc && active) {
        if (!read_word(0x83e0b028, &value) || !write_word(0x83e0b008, saved_ready)) {
            errors++;
            return;
        }
        active = 0;
        restored++;
        fprintf(stderr, "SELECTOR_REJECT restored status=%" PRIu32 "\n", value);
    }
}

static void translate(struct qemu_plugin_tb *tb, void *unused) {
    (void)unused;
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        if (pc == begin_pc || pc == return_pc)
            qemu_plugin_register_vcpu_insn_exec_cb(insn, observe, QEMU_PLUGIN_CB_R_REGS, (void *)(uintptr_t)pc);
    }
}

static void finish(void *unused) {
    (void)unused;
    fprintf(stderr, "SELECTOR_REJECT injected=%u restored=%u active=%u errors=%u\n",
            injected, restored, active, errors);
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info, int argc, char **argv) {
    if (!info->system_emulation || argc != 2 || strncmp(argv[0], "begin=", 6) || strncmp(argv[1], "return=", 7)) return -1;
    char *end;
    errno = 0;
    begin_pc = strtoull(argv[0] + 6, &end, 0);
    if (errno || *end || !begin_pc) return -1;
    errno = 0;
    return_pc = strtoull(argv[1] + 7, &end, 0);
    if (errno || *end || !return_pc || return_pc == begin_pc) return -1;
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    qemu_plugin_register_atexit_cb(id, finish, NULL);
    return 0;
}
