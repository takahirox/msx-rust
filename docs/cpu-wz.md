# Z80 WZ / MEMPTR

The shared CPU selects a Zilog NMOS-compatible WZ profile for both MSX models.
`Cpu::wz` is a 16-bit internal latch. `default`, `new`, and machine reset initialize
it to zero as a deterministic emulator policy, not a universal physical reset
measurement. Cloning retains it. No CPU selection control or Wasm debug ABI is added.

`BIT b,(HL)` copies WZ bits 11/13 into F3/F5 without changing WZ. Register BIT
copies operand bits 3/5. Indexed BIT copies effective-address bits 11/13, after
latching that address into WZ. Carry and documented BIT flags keep their existing
semantics. WZ changes introduce no bus accesses, timing or refresh changes.

## Instruction contract

All arithmetic below wraps at 16 bits unless explicitly restricted to the low
byte. “Old” means before the instruction/iteration; “new” means after it.
`nn` is the immediate address, `rp` a supported register pair, and `index` IX/IY.
Aliases decoded as the same instruction follow the same rule.

| Instruction family | WZ after execution |
| --- | --- |
| `LD A,(BC/DE/nn)` | Old address + 1 |
| `LD (BC/DE/nn),A` | High byte = old A; low byte = low(address + 1), without carry into high byte |
| `LD rp,(nn)` / `LD (nn),rp`, including HL/IX/IY and ED forms | nn + 1 |
| `ADD HL/IX/IY,rp`; `ADC/SBC HL,rp` | Old destination pair + 1, independent of arithmetic result/carry |
| `EX (SP),HL/IX/IY` | Word read from stack (new pair) |
| `RRD` / `RLD` | HL + 1 |
| Any `(IX/IY+d)` access, including indexed CB and register-copy forms | Old index + signed displacement |
| Immediate `JP` / `CALL`, conditional or unconditional | nn, including untaken conditions |
| Taken `JR`, `DJNZ`, `RET`, `RETI`, `RETN`, `RST` | Resolved destination |
| Untaken `JR`, `DJNZ`, `RET`; `JP (HL/IX/IY)` | Preserved |
| `IN A,(n)` | ((old A << 8) OR n) + 1, full-word carry |
| `OUT (n),A` | High byte = A; low byte = low(n + 1) |
| `IN r,(C)`, flag-only `IN (C)`, `OUT (C),r/0` | Old BC + 1, even when input replaces B/C |
| `LDI` / `LDD`; terminal `LDIR` / `LDDR` | Preserved |
| `CPI` / `CPD`; terminal `CPIR` / `CPDR` | Previous WZ + 1 / −1 |
| Nonterminal `LDIR/LDDR/CPIR/CPDR/INIR/INDR/OTIR/OTDR` | Address of ED prefix + 1 |
| `INI/IND`; terminal `INIR/INDR` | Old BC + 1 / −1 (before decrementing B) |
| `OUTI/OUTD`; terminal `OTIR/OTDR` | New BC + 1 / −1 (after decrementing B) |
| NMI, IM1, IM2 entry | Resolved handler address, not the IM2 vector address |
| Supported IM0 instruction | Instruction's own rule; supplied `JP (HL)` preserves WZ |
| All other supported instructions, ignored prefixes, HALT idle | Preserved |

A repeating compare terminates on a match or exhausted BC; repeating transfers
terminate on exhausted BC; repeating I/O terminates on exhausted B. Each step
executes one iteration. Ignored DD/FD bytes preceding ED are outside the repeat
address. Rewinding PC by two locates ED, including across FFFFh. Interrupts can
replace WZ between iterations; the terminal iteration then uses that current WZ.

## Evidence and limits

Behavior was independently implemented from the [2006 chip investigation by
Boo-boo and collaborators](https://raw.githubusercontent.com/floooh/emu-info/master/z80/memptr_eng.txt)
and the [expanded redcode instruction table](https://github.com/redcode/Z80/wiki/MEMPTR).
The historical report's repeating-I/O rule is superseded by [z80test correction
#3](https://github.com/raxoft/z80test/pull/3), whose discussion reports Zilog and NEC
confirmation. Nonterminal block I/O uses ED-address+1; terminal I/O retains the
old/new BC distinction. No emulator implementation or upstream test bytes were copied.

The original [native sequences](../crates/msx-core/tests/memptr.rs) use literal
WZ, flags, destination and cycle expectations under both fetch policies. They
cover all update families, both conditional paths, all BIT numbers and carries,
signed displacement boundaries, wrap/carry/borrow through later CPI/CPD and BIT,
preservation, interrupted repeats, opcode-overwrite input and IM0 `JP (HL)` probes.
The shared [original diagnostic](../tests/fixtures/wz-probe.hex) captures eight
flag bytes in mapped RAM and displays them as a VDP glyph. Native Machine and
[compiled-Wasm tests](../tests/wz.spec.js) check MSX1/MSX2 and Reset; browser tests
read actual Canvas pixels at `/` and `/msx-rust/` without a debug interface.

These synthetic checks do not establish complete Z80 conformance, physical
reset universality or commercial-game compatibility. Q-dependent SCF/CCF flags,
other block-repeat flag corrections, arbitrary IM0 decoding, contention and
access-phase synchronization remain outside this change. See the [validation
record](validation.md#issue-73-wz-memptr-and-bit-hl-flags) for actual check results.
