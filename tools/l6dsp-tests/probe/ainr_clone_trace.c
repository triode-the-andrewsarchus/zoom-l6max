// QEMU-only CPU memory trace of the two copied AINR code ranges used by
// ainr_isolation.py. Regions 0, 1, 2 and 3 are respectively the clone prefix,
// candidate tail, clone suffix, and original stock-state range. It does not
// instrument helper instructions outside those copied code ranges or DMA.
// Optional probe instructions at 0x80f1ff00..0x80f1ff0c are positive controls.
#include <stdint.h>
#include <stdio.h>
#include <inttypes.h>
#include <qemu-plugin.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static uint64_t counts[2][4][2], positive[4];
static uint64_t min_addr[2][4], max_addr[2][4];
static void memory(unsigned cpu, qemu_plugin_meminfo_t info, uint64_t address, void *opaque) {
 (void)cpu; uintptr_t pc=(uintptr_t)opaque; unsigned store=qemu_plugin_mem_is_store(info)?1:0;
 if(pc>=0x80f1ff00 && pc<0x80f1ff10) {
  const uint64_t test[]={0x80e00000,0x80e20000,0x80e30000,0x80e50000};
  unsigned index=(pc-0x80f1ff00)/4;
  if(!store && address==test[index]) positive[index]++;
  return;
 }
 unsigned clone=(pc>=0x80f10000)?1:0; uint64_t base=clone?0x80e30000:0x80e00000;
 unsigned shift=qemu_plugin_mem_size_shift(info); if(shift>=20)return; uint64_t end=address+(UINT64_C(1)<<shift);
 const uint64_t starts[]={base,base+0xb94c,base+0x19514,0x20280000};
 const uint64_t ends[]={base+0xb94c,base+0x19514,base+0x27000,0x202a7000};
 for(unsigned i=0;i<4;i++) if(address<ends[i] && end>starts[i]) {
  counts[clone][i][store]++;if(!min_addr[clone][i]||address<min_addr[clone][i]) min_addr[clone][i]=address;
  if(end>max_addr[clone][i])max_addr[clone][i]=end;
 }
}
static void translate(struct qemu_plugin_tb *tb,void *unused) {
 (void)unused; for(size_t i=0;i<qemu_plugin_tb_n_insns(tb);i++) {
  struct qemu_plugin_insn *insn=qemu_plugin_tb_get_insn(tb,i);uintptr_t pc=qemu_plugin_insn_vaddr(insn);
  if((pc>=0x80f00000 && pc<0x80f05b4c)||(pc>=0x80f10000 && pc<0x80f15b4c)||(pc>=0x80f1ff00 && pc<0x80f1ff10))
   qemu_plugin_register_vcpu_mem_cb(insn,memory,QEMU_PLUGIN_CB_NO_REGS,QEMU_PLUGIN_MEM_RW,(void*)pc);
 }
}
static void finish(void *unused) {
 (void)unused; fprintf(stderr,"CLONE_TRACE positive=%" PRIu64 ",%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",positive[0],positive[1],positive[2],positive[3]);
 for(unsigned c=0;c<2;c++)for(unsigned i=0;i<4;i++)fprintf(stderr,"CLONE_TRACE clone=%u region=%u reads=%" PRIu64 " writes=%" PRIu64 " min=%" PRIx64 " end=%" PRIx64 "\n",c,i,counts[c][i][0],counts[c][i][1],min_addr[c][i],max_addr[c][i]);
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,const qemu_info_t *info,int argc,char **argv) {
 (void)argc;(void)argv;if(!info->system_emulation)return -1;qemu_plugin_register_vcpu_tb_trans_cb(id,translate,NULL);qemu_plugin_register_atexit_cb(id,finish,NULL);return 0;
}
