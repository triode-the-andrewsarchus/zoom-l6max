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
static uint32_t saved_word, target_address = 0x83e0b008;
static unsigned injected, restored, active, errors, callbacks, hold_callbacks;
static bool closed_gate;

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

static void inject_fault(void) {
    uint32_t value;
    if (!read_word(0x81fff004, &value)) { errors++; return; }
    if (value != 0x4f564c50) return;
    uint32_t expected = closed_gate ? 0 : 0x47415445;
    uint32_t injected_word = closed_gate ? 4 : 0xdeadbeef;
    if (!read_word(target_address, &saved_word) || saved_word != expected ||
        !write_word(0x81fff004, 0) || !write_word(target_address, injected_word)) {
        errors++;
        return;
    }
    active = 1;
    injected++;
    fprintf(stderr, "SELECTOR_REJECT injected %s=%08" PRIx32 "\n", closed_gate ? "gate" : "ready", saved_word);
}

static void selector_return(void) {
    uint32_t status;
    if (!read_word(0x83e0b028, &status)) { errors++; return; }
    if (hold_callbacks) {
        active = 2;
        fprintf(stderr, "SELECTOR_REJECT held status=%" PRIu32 "\n", status);
        return;
    }
    if (!write_word(target_address, saved_word)) { errors++; return; }
    active = 0;
    restored++;
    fprintf(stderr, "SELECTOR_REJECT restored status=%" PRIu32 "\n", status);
}

static void audio_entry(void) {
    if (++callbacks != hold_callbacks) return;
    if (!write_word(target_address, saved_word)) { errors++; return; }
    active = 0;
    restored++;
    fprintf(stderr, "SELECTOR_REJECT restored callbacks=%u\n", callbacks);
}

static void observe(unsigned cpu, void *opaque) {
    (void)cpu;
    uint64_t pc = (uintptr_t)opaque;
    if (pc == begin_pc && !injected) inject_fault();
    else if (pc == return_pc && active == 1) selector_return();
    else if (pc == 0x202800e8 && active == 2) audio_entry();
}

static void translate(struct qemu_plugin_tb *tb, void *unused) {
    (void)unused;
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint64_t pc = qemu_plugin_insn_vaddr(insn);
        if (pc == begin_pc || pc == return_pc || (hold_callbacks && pc == 0x202800e8))
            qemu_plugin_register_vcpu_insn_exec_cb(insn, observe, QEMU_PLUGIN_CB_R_REGS, (void *)(uintptr_t)pc);
    }
}

static void finish(void *unused) {
    (void)unused;
    fprintf(stderr, "SELECTOR_REJECT injected=%u restored=%u active=%u errors=%u\n",
            injected, restored, active, errors);
}

static bool parse_address(const char *text, uint64_t *value) {
    char *end;
    errno = 0;
    *value = strtoull(text, &end, 0);
    return !errno && !*end && *value;
}

static bool parse_optional(int argc, char **argv) {
    for (int i = 2; i < argc; i++) {
        if (!strcmp(argv[i], "closed=on") && !closed_gate) {
            closed_gate = true;
            target_address = 0x83e0b000;
        } else if (!strncmp(argv[i], "hold=", 5) && !hold_callbacks) {
            uint64_t count;
            if (!parse_address(argv[i] + 5, &count) || count > 1000000) return false;
            hold_callbacks = (unsigned)count;
        } else return false;
    }
    return true;
}

QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id, const qemu_info_t *info, int argc, char **argv) {
    if (!info->system_emulation || argc < 2 || argc > 4 || strncmp(argv[0], "begin=", 6) || strncmp(argv[1], "return=", 7)) return -1;
    if (!parse_address(argv[0] + 6, &begin_pc) || !parse_address(argv[1] + 7, &return_pc) ||
        return_pc == begin_pc || !parse_optional(argc, argv)) return -1;
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    qemu_plugin_register_atexit_cb(id, finish, NULL);
    return 0;
}
