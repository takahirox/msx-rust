use msx_core::{
    basic::Basic,
    psg::Psg,
    vdp::{Vdp, PALETTE},
};
fn run(b: &mut Basic, v: &mut Vdp, p: &mut Psg, line: &str) {
    b.submit(line).unwrap();
    for _ in 0..20 {
        b.pump(v, p, 200);
        if !b.running {
            break;
        }
    }
}
fn fixture() -> (Basic, Vdp, Psg) {
    let mut b = Basic::default();
    let mut v = Vdp::default();
    b.boot(&mut v);
    (b, v, Psg::default())
}
#[test]
fn assignment_if_goto_strings_expressions_and_negative_step() {
    let (mut b, mut v, mut p) = fixture();
    for source in [
        "10 LET A=2+3*4",
        "20 IF A<>14 THEN 100",
        "30 A$=\"GOOD\"+\"!\"",
        "40 FOR I=3 TO 1 STEP -1",
        "50 PRINT A$;I",
        "60 NEXT I",
        "70 GOTO 110",
        "100 PRINT \"FAIL\"",
        "110 IF A=14 THEN PRINT \"PASS\"",
        "120 END",
    ] {
        b.submit(source).unwrap();
    }
    run(&mut b, &mut v, &mut p, "RUN");
    assert!(b
        .console
        .transcript
        .ends_with("GOOD!3\nGOOD!2\nGOOD!1\nPASS\nREADY\n"));
    assert!(!b.console.transcript.contains("FAIL\n"));
    run(
        &mut b,
        &mut v,
        &mut p,
        "PRINT -2^2;\",\";2^3^2;\",\";INT(2.9);\",\";ABS(-3)",
    );
    assert!(b.console.transcript.ends_with("-4,512,2,3\nREADY\n"));
}
#[test]
fn nested_zero_trip_loops_and_false_if_skips_remaining_line() {
    let (mut b, mut v, mut p) = fixture();
    run(
        &mut b,
        &mut v,
        &mut p,
        "FOR I=2 TO 1 : FOR J=1 TO 3 : PRINT 99 : NEXT J : NEXT I : PRINT 7",
    );
    assert!(b.console.transcript.ends_with("7\nREADY\n"));
    run(
        &mut b,
        &mut v,
        &mut p,
        "FOR I=1 TO 2 : FOR J=1 TO 2 : PRINT I*10+J : NEXT J : NEXT I",
    );
    assert!(b.console.transcript.ends_with("11\n12\n21\n22\nREADY\n"));
    run(&mut b, &mut v, &mut p, "IF 0 THEN PRINT 99 : PRINT 88");
    assert!(!b.console.transcript.contains("99\n"));
    assert!(!b.console.transcript.contains("88\n"));
}
#[test]
fn syntax_and_runtime_errors_do_not_panic_or_hang() {
    let (mut b, mut v, mut p) = fixture();
    for (line, error) in [
        ("PRINT 1/0", "DIVISION BY ZERO"),
        ("NEXT I", "NEXT WITHOUT FOR"),
        ("FOR I=1 TO 2 STEP 0", "ZERO STEP"),
        ("A$=3", "TYPE MISMATCH"),
        ("PRINT (2+3", "EXPECTED )"),
        ("GOTO 99", "UNDEFINED LINE"),
        ("PSET 0,0,1", "USE SCREEN 2 OR 3"),
    ] {
        run(&mut b, &mut v, &mut p, line);
        assert!(
            b.console.transcript.contains(error),
            "{line}: {}",
            b.console.transcript
        );
        assert!(!b.running);
    }
    assert!(b.submit("PRINT \"oops").is_err());
    assert!(b.submit("999999 PRINT 1").is_err());
}
#[test]
fn graphics_and_sound_commands_use_emulated_devices() {
    let (mut b, mut v, mut p) = fixture();
    run(
        &mut b,
        &mut v,
        &mut p,
        "SCREEN 2 : FOR X=0 TO 255 : PSET X,96,10 : NEXT X : SOUND 0,254 : SOUND 7,62 : SOUND 8,15",
    );
    v.render();
    for x in 0..256 {
        assert_eq!(
            &v.pixels[(96 * 256 + x) * 4..(96 * 256 + x) * 4 + 3],
            &PALETTE[10]
        );
    }
    assert_eq!(p.registers[0], 254);
    assert_eq!(p.registers[8], 15);
    b.key(13, &mut v, &mut p);
    assert_eq!(v.registers[1] & 0x10, 0x10);
}
#[test]
fn infinite_program_yields_and_break_restores_prompt() {
    let (mut b, mut v, mut p) = fixture();
    b.submit("10 GOTO 10").unwrap();
    b.submit("RUN").unwrap();
    b.pump(&mut v, &mut p, 1000);
    assert!(b.running);
    b.key(27, &mut v, &mut p);
    assert!(!b.running);
    assert!(b.console.transcript.ends_with("BREAK\nREADY\n"));
    run(&mut b, &mut v, &mut p, "PRINT 42");
    assert!(b.console.transcript.ends_with("42\nREADY\n"));
}
#[test]
fn multicolor_coordinates_cover_the_full_screen_and_cls_keeps_tile_font() {
    let (mut b, mut v, mut p) = fixture();
    run(
        &mut b,
        &mut v,
        &mut p,
        "SCREEN 3 : FOR Y=0 TO 191 STEP 4 : PSET 120,Y,10 : NEXT Y",
    );
    v.render();
    for y in 0..192 {
        assert_eq!(
            &v.pixels[(y * 256 + 120) * 4..(y * 256 + 120) * 4 + 3],
            &PALETTE[10],
            "row {y}"
        );
    }
    run(&mut b, &mut v, &mut p, "SCREEN 1 : CLS : VPOKE 6144,65");
    v.render();
    assert!(v.pixels.chunks(4).any(|p| p[..3] == PALETTE[15]));
    assert!(b.submit("PRINT \"\u{80}\"").is_err());
}

fn program(b: &mut Basic, source: &str) {
    for line in source.lines() {
        b.submit(line).unwrap();
    }
}
fn ends(b: &Basic, expected: &str) {
    assert!(
        b.console.transcript.ends_with(expected),
        "{}",
        b.console.transcript
    );
}
fn keys(b: &mut Basic, v: &mut Vdp, p: &mut Psg, text: &str) {
    for key in text.bytes() {
        b.key(key, v, p);
    }
    b.key(13, v, p);
}
#[test]
fn original_data_example_and_repeated_run_reset_arrays_scalars_and_data() {
    let (mut b, mut v, mut p) = fixture();
    program(&mut b, include_str!("../../../examples/basic/scores.bas"));
    for _ in 0..2 {
        run(&mut b, &mut v, &mut p, "RUN");
        ends(&b, "COMET:3\nORBIT:1\nMOON:2\nREADY\n");
    }
    run(&mut b, &mut v, &mut p, "A=99:Z$=\"OLD\"");
    b.submit("10 PRINT A;Z$:END").unwrap();
    run(&mut b, &mut v, &mut p, "RUN");
    ends(&b, "0\nREADY\n");
}
#[test]
fn nested_subroutines_preserve_caller_loop_and_return_to_colon_tail() {
    let (mut b, mut v, mut p) = fixture();
    program(&mut b, "10 FOR I=1 TO 2:IF I>0 THEN GOSUB 100\n20 PRINT I:NEXT I:END\n100 GOSUB 200:PRINT 3:RETURN\n200 FOR J=1 TO 2:PRINT J:RETURN");
    run(&mut b, &mut v, &mut p, "RUN");
    ends(&b, "1\n3\n1\n1\n3\n2\nREADY\n");
    run(&mut b, &mut v, &mut p, "GOSUB 100:PRINT 4");
    ends(&b, "1\n3\n4\nREADY\n");
    run(&mut b, &mut v, &mut p, "RETURN");
    ends(&b, "?RETURN WITHOUT GOSUB\nREADY\n");
    // A false direct IF must not fall through into stored program lines.
    run(&mut b, &mut v, &mut p, "IF 0 THEN GOSUB 100:PRINT 99");
    ends(&b, "READY\n");
    assert!(!b.console.transcript.contains("99\n"));
    run(&mut b, &mut v, &mut p, "GOTO 200");
    ends(&b, "1\n?RETURN WITHOUT GOSUB IN 200\nREADY\n");
}
#[test]
fn subroutine_stop_end_error_break_new_and_run_clear_control_state() {
    for finish in ["STOP", "END", "PRINT 1/0", "NEW", "GOTO 300"] {
        let (mut b, mut v, mut p) = fixture();
        program(
            &mut b,
            &format!("10 GOSUB 100:PRINT 99\n100 {finish}\n300 REM END OF PROGRAM"),
        );
        run(&mut b, &mut v, &mut p, "RUN");
        assert!(!b.running);
        assert!(!b.console.transcript.contains("99\n"));
        run(&mut b, &mut v, &mut p, "RETURN");
        ends(&b, "?RETURN WITHOUT GOSUB\nREADY\n");
    }
    let (mut b, mut v, mut p) = fixture();
    program(&mut b, "10 GOSUB 100\n100 GOTO 100");
    run(&mut b, &mut v, &mut p, "RUN");
    assert!(b.running);
    b.key(27, &mut v, &mut p);
    ends(&b, "BREAK\nREADY\n");
    run(&mut b, &mut v, &mut p, "RETURN");
    ends(&b, "?RETURN WITHOUT GOSUB\nREADY\n");
    b.submit("100 GOSUB 100").unwrap();
    run(&mut b, &mut v, &mut p, "RUN");
    ends(&b, "?TOO MANY GOSUBS IN 100\nREADY\n");
    b.submit("10 RUN").unwrap();
    b.submit("RUN").unwrap();
    b.pump(&mut v, &mut p, 50);
    assert!(b.running); // RUN in a program also cooperatively yields.
    b.key(27, &mut v, &mut p);
}
#[test]
fn arrays_are_typed_zero_based_and_one_or_two_dimensional() {
    let (mut b, mut v, mut p) = fixture();
    run(
        &mut b,
        &mut v,
        &mut p,
        "DIM A(2,3),S$(1):A=7:A(2,3)=8:S$(0)=\"HI\":LET A(0,0)=2:PRINT A;A(2,3);A(0,0);S$(0);S$(1)",
    );
    ends(&b, "782HI\nREADY\n");
    for (line, error) in [
        ("A(3,0)=1", "SUBSCRIPT OUT OF RANGE"),
        ("PRINT A(-1,0)", "SUBSCRIPT OUT OF RANGE"),
        ("PRINT A(0.5,0)", "SUBSCRIPT OUT OF RANGE"),
        ("PRINT A(1)", "SUBSCRIPT OUT OF RANGE"),
        ("PRINT Q(1)", "UNDIMENSIONED ARRAY"),
        ("A(0,0)=\"X\"", "TYPE MISMATCH"),
        ("S$(0)=1", "TYPE MISMATCH"),
        ("DIM A(1)", "REDIMENSIONED ARRAY"),
        ("DIM B(1),B(2)", "REDIMENSIONED ARRAY"),
        ("DIM C(4096)", "SUBSCRIPT OUT OF RANGE"),
        ("DIM C(1,1,1)", "SUBSCRIPT OUT OF RANGE"),
        ("DIM C(128,128)", "ARRAY LIMIT"),
        ("DIM C()", "MISSING EXPRESSION"),
        ("DIM C(2", "EXPECTED )"),
    ] {
        run(&mut b, &mut v, &mut p, line);
        ends(&b, &format!("?{error}\nREADY\n"));
    }
    run(&mut b, &mut v, &mut p, "NEW:DIM A(1)");
    assert!(b.program.is_empty());
    run(
        &mut b,
        &mut v,
        &mut p,
        "DIM A(4095),B(4095),C(4095),D(4095)",
    );
    ends(&b, "READY\n");
    run(&mut b, &mut v, &mut p, "DIM E(0)");
    ends(&b, "?ARRAY LIMIT\nREADY\n");
}
#[test]
fn data_follows_line_order_and_restore_rewinds_with_inline_literals() {
    let (mut b, mut v, mut p) = fixture();
    program(
        &mut b,
        "30 DATA LAST\n10 DATA -2,\"A:B,C\":DATA HELLO WORLD,3.5\n20 REM DATA 99",
    );
    run(
        &mut b,
        &mut v,
        &mut p,
        "DIM A(1):READ A(1),S$,T$,N,U$:PRINT A(1);S$;T$;N;U$",
    );
    ends(&b, "-2A:B,CHELLO WORLD3.5LAST\nREADY\n");
    run(&mut b, &mut v, &mut p, "READ X");
    ends(&b, "?OUT OF DATA\nREADY\n");
    run(&mut b, &mut v, &mut p, "RESTORE:READ X:PRINT X");
    ends(&b, "-2\nREADY\n");
    run(&mut b, &mut v, &mut p, "READ X");
    ends(&b, "?TYPE MISMATCH\nREADY\n");
    // A failed numeric conversion does not consume the offending datum.
    run(&mut b, &mut v, &mut p, "READ S$:PRINT S$");
    ends(&b, "A:B,C\nREADY\n");
    run(&mut b, &mut v, &mut p, "RESTORE 20:READ S$:PRINT S$");
    ends(&b, "LAST\nREADY\n");
    run(&mut b, &mut v, &mut p, "RESTORE 99");
    ends(&b, "?UNDEFINED LINE\nREADY\n");
    run(&mut b, &mut v, &mut p, "NEW");
    run(&mut b, &mut v, &mut p, "READ X");
    ends(&b, "?OUT OF DATA\nREADY\n");
    assert!(b.submit("10 DATA \"BROKEN").is_err());
}
#[test]
fn input_retries_atomically_and_resumes_subroutine_with_array_targets() {
    let (mut b, mut v, mut p) = fixture();
    program(
        &mut b,
        "10 DIM A(1):GOSUB 100:PRINT A(1);S$:END\n100 INPUT \"VALUE,NAME\";A(1),S$:RETURN",
    );
    run(&mut b, &mut v, &mut p, "RUN");
    assert!(b.running);
    let before = b.console.transcript.clone();
    b.pump(&mut v, &mut p, 10000);
    assert_eq!(before, b.console.transcript);
    assert!(v.vram[..960].windows(12).any(|s| s == b"VALUE,NAME? "));
    keys(&mut b, &mut v, &mut p, "BAD,WHO");
    ends(&b, "?REDO FROM START\nVALUE,NAME? ");
    keys(&mut b, &mut v, &mut p, "2");
    ends(&b, "?REDO FROM START\nVALUE,NAME? ");
    keys(&mut b, &mut v, &mut p, "2,\"A,B\"");
    ends(&b, "2A,B\nREADY\n");
    assert!(!b.running);
    run(&mut b, &mut v, &mut p, "INPUT X,Y");
    keys(&mut b, &mut v, &mut p, "1,NaN");
    assert!(b.running);
    b.key(b'9', &mut v, &mut p);
    b.key(27, &mut v, &mut p);
    ends(&b, "BREAK\nREADY\n");
    run(&mut b, &mut v, &mut p, "PRINT X;Y");
    ends(&b, "00\nREADY\n");
    run(&mut b, &mut v, &mut p, "INPUT \"X\",X");
    ends(&b, "?EXPECTED ;\nREADY\n");
}
fn pixel(v: &Vdp, x: usize, y: usize) -> &[u8] {
    &v.pixels[(y * 256 + x) * 4..(y * 256 + x) * 4 + 3]
}
#[test]
fn original_shapes_draw_and_fill_real_vram_with_clipping() {
    let (mut b, mut v, mut p) = fixture();
    program(&mut b, include_str!("../../../examples/basic/shapes.bas"));
    run(&mut b, &mut v, &mut p, "RUN");
    v.render();
    for (x, y, c) in [
        (16, 96, 15),
        (128, 56, 15),
        (128, 136, 15),
        (128, 96, 10),
        (32, 32, 6),
        (63, 55, 6),
        (0, 0, 4),
        (128, 40, 4),
    ] {
        assert_eq!(pixel(&v, x, y), PALETTE[c], "{x},{y}");
    }
    assert_ne!(v.vram[0x2000 + 8 * 256 + 16 * 8], 0xf4);
    run(
        &mut b,
        &mut v,
        &mut p,
        "SCREEN 2:LINE (-20,100)-(280,100),10:CIRCLE (-5,-5),12,15:PAINT (-1,-1),6",
    );
    v.render();
    for x in 0..256 {
        assert_eq!(pixel(&v, x, 100), PALETTE[10]);
    }
    run(&mut b, &mut v, &mut p, "SCREEN 2:PAINT (128,96),4,15");
    v.render();
    assert!(v
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|px| px[..3] == PALETTE[4]));
    run(&mut b, &mut v, &mut p, "PAINT (128,96),10,15");
    v.render();
    assert!(v
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|px| px[..3] == PALETTE[10]));
}
#[test]
fn multicolor_fill_boxes_relative_points_and_graphics_errors() {
    let (mut b, mut v, mut p) = fixture();
    run(
        &mut b,
        &mut v,
        &mut p,
        "SCREEN 3:LINE (8,8)-(40,40),15,B:PAINT (20,20),10,15",
    );
    v.render();
    for (x, y, c) in [(8, 20, 15), (20, 20, 10), (44, 20, 4)] {
        assert_eq!(pixel(&v, x, y), PALETTE[c]);
    }
    run(
        &mut b,
        &mut v,
        &mut p,
        "PSET (80,80),6:LINE -STEP(20,0),6:LINE STEP(0,8)-STEP(8,8),10,BF",
    );
    v.render();
    assert_eq!(pixel(&v, 96, 80), PALETTE[6]);
    assert_eq!(pixel(&v, 104, 92), PALETTE[10]);
    run(
        &mut b,
        &mut v,
        &mut p,
        "SCREEN 2:COLOR 6,4:LINE (0,0)-(255,0)",
    );
    v.render();
    assert_eq!(pixel(&v, 128, 0), PALETTE[6]);
    for (line, error) in [
        ("CIRCLE (0,0),-1", "OUT OF RANGE"),
        ("SCREEN 2:CIRCLE (0,0),1025", "OUT OF RANGE"),
        ("SCREEN 2:CIRCLE (0,0),10,15,0,1", "SYNTAX ERROR"),
        ("SCREEN 2:LINE (0,0)-(1,1),10,Q", "SYNTAX ERROR"),
        ("SCREEN 2:LINE (40000,0)-(1,1)", "OUT OF RANGE"),
        ("SCREEN 2:LINE (32767,0)-STEP(1,0)", "OUT OF RANGE"),
        ("LINE (0,0)-(1,1)", "USE SCREEN 2 OR 3"),
    ] {
        run(&mut b, &mut v, &mut p, line);
        ends(&b, &format!("?{error}\nREADY\n"));
    }
}
#[test]
fn input_example_and_print_punctuation() {
    let (mut b, mut v, mut p) = fixture();
    program(&mut b, include_str!("../../../examples/basic/greeting.bas"));
    run(&mut b, &mut v, &mut p, "RUN");
    keys(&mut b, &mut v, &mut p, "SKY");
    keys(&mut b, &mut v, &mut p, "0");
    assert!(b.running);
    keys(&mut b, &mut v, &mut p, "2");
    ends(&b, "HELLO SKY\nHELLO SKY\nREADY\n");
    run(&mut b, &mut v, &mut p, "PRINT \"A\",\"B\";:PRINT \"C\"");
    ends(&b, "A             BC\nREADY\n");
}

#[test]
fn read_indices_use_previous_values_and_errors_preserve_types() {
    let (mut b, mut v, mut p) = fixture();
    b.submit("10 DATA 2,5").unwrap();
    run(
        &mut b,
        &mut v,
        &mut p,
        "DIM A(2):READ I,A(I):PRINT A(0);A(2)",
    );
    ends(&b, "05\nREADY\n");
    run(&mut b, &mut v, &mut p, "PRINT A(\"X\")");
    ends(&b, "?TYPE MISMATCH\nREADY\n");
    run(&mut b, &mut v, &mut p, "GOSUB 0");
    ends(&b, "?UNDEFINED LINE\nREADY\n");
    run(&mut b, &mut v, &mut p, "STOP");
    ends(&b, "BREAK\nREADY\n");
    program(&mut b, "10 GOSUB 100\n100 STOP");
    run(&mut b, &mut v, &mut p, "RUN");
    ends(&b, "BREAK IN 100\nREADY\n");
}
#[test]
fn repeated_large_graphics_yields_between_primitives_and_accepts_break() {
    let (mut b, mut v, mut p) = fixture();
    program(&mut b, "10 SCREEN 2\n20 PAINT (0,0),4,15:GOTO 20");
    b.submit("RUN").unwrap();
    b.pump(&mut v, &mut p, 10000);
    assert!(b.running);
    b.key(27, &mut v, &mut p);
    ends(&b, "BREAK\nREADY\n");
    assert!(!b.running);
}
