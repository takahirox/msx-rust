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
