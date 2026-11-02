    .syntax unified
    .thumb
    .section .text.probe,"ax",%progbits
    .global probe
    .type probe,%function
    .thumb_func
probe:
    // These are the return addresses of the two observed selector calls.
    // Other users of the shared veneer must execute the original clear.
    ldr.w r12, .Lselector_return_1
    cmp lr, r12
    beq .Lselection
    ldr.w r12, .Lselector_return_2
    cmp lr, r12
    beq .Lselection
    ldr.w r12, .Lstock_clear
    bx r12
.Lselection:
    b.w probe_selection
    .align 2
.Lselector_return_1: .word 0x20280677
.Lselector_return_2: .word 0x202806c5
.Lstock_clear: .word 0x80024679
    .size probe, .-probe
