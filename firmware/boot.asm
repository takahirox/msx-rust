; Original MIT-licensed boot firmware. firmware.rs emits these bytes without an assembler.
; This is NOT a replacement for the MSX BIOS API. Weave BASIC runs as a Rust
; firmware service in msx-core, through two private ports only enabled for this ROM.
; Standard MSX cartridges and supplied BIOS ROMs use the Z80/hardware path.
        org 0
        jp 0100h        ; MSX2: jp 0080h, mapper initialization below
        org 002dh
        db 0            ; MSX1 generation; the MSX2 Weave BASIC profile sets 1
        org 0038h
        reti
        org 0066h
        retn
        org 0100h
        di
        ld sp,0f380h
        xor a
        out (0f1h),a     ; native BASIC boot service
        ld a,(4000h)
        cp 'A'
        jr nz,next
        ld a,(4001h)
        cp 'B'
        jr nz,next
        ld hl,(4002h)
        ld a,h
        or l
        jr z,poll
        ld de,poll
        push de         ; allow cartridge init to return to BASIC
        jp (hl)         ; plain AB cartridge initialization
next:   ld a,(8000h)
        cp 'A'
        jr nz,poll
        ld a,(8001h)
        cp 'B'
        jr nz,poll
        ld hl,(8002h)
        ld a,h
        or l
        jr z,poll
        ld de,poll
        push de
        jp (hl)
poll:   in a,(0f0h)     ; queued ASCII/editor key; zero means no key
        or a
        jr z,poll
        out (0f1h),a     ; BASIC line editor/interpreter service
        jr poll

; MSX2 only: reset jumps to this trampoline before the routine at 0100h.
; MSX1 retains the direct JP 0100h and unconnected FCh..FFh ports.
; The original diagnostic and disk loaders use the same trampoline before
; their own 0100h routines. C-BIOS and supplied firmware initialize themselves.
        org 0080h
        di
        ld a,3
        out (0fch),a
        ld a,2
        out (0fdh),a
        ld a,1
        out (0feh),a
        ld a,0
        out (0ffh),a
        jp 0100h
