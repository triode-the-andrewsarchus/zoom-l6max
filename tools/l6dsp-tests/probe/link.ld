ENTRY(probe)
SECTIONS {
  . = 0x80108000;
  .text : { KEEP(*(.text.probe)) *(.text*) *(.rodata*) }
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment*) }
}
