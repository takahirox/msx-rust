/* Original MIT test driver. Compile against the pinned upstream emu2413.c;
 * see README.md in this directory. The upstream C file is not vendored here. */
#include "emu2413.c"
static OPLL *chip;
static void clocks(int samples) { while (samples--) update_output(chip); }
static void write_reg(int reg, int data) {
  clocks(1); /* address-write delay: 72 clocks >= 12 */
  OPLL_writeReg(chip, reg, data);
  clocks(2); /* data-write delay: 144 clocks >= 84 */
}
static void trace(const char *label) {
  printf("%s", label);
  for (int n=0; n<32; n++) {
    clocks(1);
    int sum=0;
    for (int c=0; c<14; c++)
      if (chip->rhythm_mode ? (c<6 || c>=9) : c<9) sum+=chip->ch_out[c];
    printf(" %d",sum);
  }
  puts("");
}
int main(void) {
  for (int preset=0; preset<16; preset++) {
    chip=OPLL_new(3579545,49716);
    int patch[]={0x21,0x21,0x3f,0,0,0xf0,0,0x0f};
    for(int i=0;i<8;i++) write_reg(i,patch[i]);
    write_reg(0x30,preset<<4); write_reg(0x10,0x22); write_reg(0x20,0x19);
    clocks(4096);
    char label[32]; snprintf(label,sizeof label,"preset-%d",preset); trace(label);
    write_reg(0x20,0x29); clocks(1024); trace("release");
    OPLL_delete(chip);
  }
  for (int bit=0; bit<5; bit++) {
    chip=OPLL_new(3579545,49716);
    for(int ch=6;ch<9;ch++) {
      write_reg(0x10+ch,0x80+ch*7); write_reg(0x20+ch,0x0b); write_reg(0x30+ch,0);
    }
    write_reg(14,32|(1<<bit)); clocks(256);
    trace("rhythm"); OPLL_delete(chip);
  }
  chip=OPLL_new(3579545,49716);
  int custom[]={0xf5,0xf2,0x9b,0xdf,0x85,0xb6,0x32,0x48};
  for(int r=0;r<8;r++) write_reg(r,custom[r]);
  for(int ch=0;ch<9;ch++) {
    write_reg(0x30+ch,ch); write_reg(0x10+ch,0x22+ch*13); write_reg(0x20+ch,0x19);
    trace("key-on");
  }
  clocks(2048); trace("am-pm-feedback-scaling");
  write_reg(1,0x71); write_reg(3,7); trace("live-custom-update");
  write_reg(0x27,0x39); trace("sustain-on");
  write_reg(14,0x3f); trace("enter-rhythm");
  write_reg(0x27,9); write_reg(14,32); trace("rhythm-key-off");
  write_reg(14,0); trace("leave-rhythm");
  OPLL_delete(chip);

}
