ENTRY(probe)
PROVIDE(PROBE_BASE = 0x80108000);
SECTIONS {
  . = PROBE_BASE;
  .text : { KEEP(*(.text.probe)) KEEP(*(.text.ainr_capture)) KEEP(*(.text.inspector_*)) KEEP(*(.text.ainr_selector_veneer)) KEEP(*(.text.before_ainr_disable)) KEEP(*(.text.offline_mix_veneer)) *(.text*) *(.rodata*) }
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment*) }
}
