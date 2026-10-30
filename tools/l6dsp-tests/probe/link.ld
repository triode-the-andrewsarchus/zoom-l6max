ENTRY(probe)
SECTIONS {
  . = 0x80108000;
  .text : { KEEP(*(.text.probe)) KEEP(*(.text.inspector_*)) *(.text*) *(.rodata*) }
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment*) }
}
