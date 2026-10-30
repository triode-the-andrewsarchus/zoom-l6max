ENTRY(probe)
PROVIDE(PROBE_BASE = 0x80108000);
SECTIONS {
  . = PROBE_BASE;
  .text : { KEEP(*(.text.probe)) KEEP(*(.text.ainr_capture)) KEEP(*(.text.inspector_*)) *(.text*) *(.rodata*) }
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment*) }
}
