//! Weave BASIC: original Rust interpreter, invoked by the bundled Z80 firmware.
//! It is a portable firmware service, not a copy or binary-compatible MSX BASIC.
use crate::{font, psg::Psg, vdp::Vdp};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
    Name(String),
    Op(String),
}
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Number(f64),
    Text(String),
}
impl Value {
    fn number(&self) -> Result<f64, String> {
        match self {
            Self::Number(v) => Ok(*v),
            _ => Err("TYPE MISMATCH".into()),
        }
    }
    fn display(&self) -> String {
        match self {
            Self::Number(v) => format!("{v}"),
            Self::Text(s) => s.clone(),
        }
    }
}
fn lex(line: &str) -> Result<Vec<Token>, String> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < bytes.len() {
        let c = bytes[p];
        if c.is_ascii_whitespace() {
            p += 1;
            continue;
        }
        if c == b'\'' {
            break;
        }
        if c == b'"' {
            p += 1;
            let start = p;
            while p < bytes.len() && bytes[p] != b'"' {
                p += 1;
            }
            if p == bytes.len() {
                return Err("UNTERMINATED STRING".into());
            }
            out.push(Token::Text(line[start..p].into()));
            p += 1;
        } else if c.is_ascii_digit() || c == b'.' {
            let start = p;
            p += 1;
            while p < bytes.len() && (bytes[p].is_ascii_digit() || bytes[p] == b'.') {
                p += 1;
            }
            out.push(Token::Number(
                line[start..p].parse().map_err(|_| "BAD NUMBER")?,
            ));
        } else if c.is_ascii_alphabetic() {
            let start = p;
            p += 1;
            while p < bytes.len()
                && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'$' || bytes[p] == b'_')
            {
                p += 1;
            }
            let name = line[start..p].to_ascii_uppercase();
            let rem = name == "REM";
            out.push(Token::Name(name));
            if rem {
                break;
            }
        } else {
            let mut op = (c as char).to_string();
            p += 1;
            if p < bytes.len()
                && ((c == b'<' && (bytes[p] == b'>' || bytes[p] == b'='))
                    || (c == b'>' && bytes[p] == b'='))
            {
                op.push(bytes[p] as char);
                p += 1;
            }
            out.push(Token::Op(op));
        }
    }
    Ok(out)
}
struct Parser<'a> {
    tokens: &'a [Token],
    p: usize,
    vars: &'a BTreeMap<String, Value>,
}
impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], vars: &'a BTreeMap<String, Value>) -> Self {
        Self { tokens, p: 0, vars }
    }
    fn is(&self, s: &str) -> bool {
        matches!(self.tokens.get(self.p),Some(Token::Name(v)|Token::Op(v)) if v==s)
    }
    fn take(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.p += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> Result<(), String> {
        if self.take(s) {
            Ok(())
        } else {
            Err(format!("EXPECTED {s}"))
        }
    }
    fn name(&mut self) -> Result<String, String> {
        if let Some(Token::Name(s)) = self.tokens.get(self.p) {
            self.p += 1;
            Ok(s.clone())
        } else {
            Err("EXPECTED VARIABLE".into())
        }
    }
    fn done(&self) -> Result<(), String> {
        if self.p == self.tokens.len() {
            Ok(())
        } else {
            Err("SYNTAX ERROR".into())
        }
    }
    fn number(&mut self) -> Result<f64, String> {
        self.expr(0)?.number()
    }
    fn integer(&mut self, max: usize) -> Result<usize, String> {
        let n = self.number()?;
        if n >= 0.0 && n <= max as f64 && n.fract() == 0.0 {
            Ok(n as usize)
        } else {
            Err("OUT OF RANGE".into())
        }
    }
    fn expr(&mut self, min: u8) -> Result<Value, String> {
        let mut lhs = if self.take("-") {
            Value::Number(-self.expr(6)?.number()?)
        } else if self.take("+") {
            Value::Number(self.expr(6)?.number()?)
        } else if self.take("NOT") {
            Value::Number(if self.expr(3)?.number()? == 0.0 {
                -1.0
            } else {
                0.0
            })
        } else if self.take("(") {
            let v = self.expr(0)?;
            self.expect(")")?;
            v
        } else {
            let tok = self.tokens.get(self.p).ok_or("MISSING EXPRESSION")?.clone();
            self.p += 1;
            match tok {
                Token::Number(v) => Value::Number(v),
                Token::Text(s) => Value::Text(s),
                Token::Name(s) => {
                    if self.take("(") {
                        let v = self.number()?;
                        self.expect(")")?;
                        Value::Number(match s.as_str() {
                            "ABS" => v.abs(),
                            "INT" => v.floor(),
                            "SQR" if v >= 0.0 => v.sqrt(),
                            "SIN" => v.sin(),
                            "COS" => v.cos(),
                            _ => return Err("UNKNOWN FUNCTION".into()),
                        })
                    } else {
                        self.vars.get(&s).cloned().unwrap_or(if s.ends_with('$') {
                            Value::Text(String::new())
                        } else {
                            Value::Number(0.0)
                        })
                    }
                }
                _ => return Err("MISSING EXPRESSION".into()),
            }
        };
        while let Some(Token::Name(s) | Token::Op(s)) = self.tokens.get(self.p) {
            let op = s.clone();
            let prec = match op.as_str() {
                "OR" => 1,
                "AND" => 2,
                "=" | "<>" | "<" | ">" | "<=" | ">=" => 3,
                "+" | "-" => 4,
                "*" | "/" | "MOD" => 5,
                "^" => 7,
                _ => break,
            };
            if prec < min {
                break;
            }
            self.p += 1;
            let rhs = self.expr(if op == "^" { prec } else { prec + 1 })?;
            lhs = if op == "+" && matches!(lhs, Value::Text(_)) {
                if let Value::Text(r) = rhs {
                    Value::Text(lhs.display() + &r)
                } else {
                    return Err("TYPE MISMATCH".into());
                }
            } else if prec == 3 {
                let ordering = match (&lhs, &rhs) {
                    (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
                    (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
                    _ => return Err("TYPE MISMATCH".into()),
                }
                .ok_or("BAD NUMBER")?;
                use std::cmp::Ordering::*;
                let yes = match op.as_str() {
                    "=" => ordering == Equal,
                    "<>" => ordering != Equal,
                    "<" => ordering == Less,
                    ">" => ordering == Greater,
                    "<=" => ordering != Greater,
                    _ => ordering != Less,
                };
                Value::Number(if yes { -1.0 } else { 0.0 })
            } else {
                let a = lhs.number()?;
                let b = rhs.number()?;
                Value::Number(match op.as_str() {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" if b != 0.0 => a / b,
                    "MOD" if b != 0.0 => a % b,
                    "^" => a.powf(b),
                    "AND" => {
                        if a != 0.0 && b != 0.0 {
                            -1.0
                        } else {
                            0.0
                        }
                    }
                    "OR" => {
                        if a != 0.0 || b != 0.0 {
                            -1.0
                        } else {
                            0.0
                        }
                    }
                    _ => return Err("DIVISION BY ZERO".into()),
                })
            };
            if matches!(&lhs,Value::Number(n) if !n.is_finite()) {
                return Err("NUMBER OVERFLOW".into());
            }
        }
        Ok(lhs)
    }
}

#[derive(Default)]
pub struct Console {
    pub transcript: String,
    x: usize,
    y: usize,
}
impl Console {
    pub fn setup(&mut self, vdp: &mut Vdp) {
        vdp.registers[..8].copy_from_slice(&[0, 0x50, 0, 0, 1, 0x36, 7, 0xf4]);
        font::install(&mut vdp.vram, 0x800);
        vdp.vram[..960].fill(b' ');
        self.x = 0;
        self.y = 0;
    }
    fn newline(&mut self, vdp: &mut Vdp) {
        self.x = 0;
        self.y += 1;
        if self.y >= 24 {
            vdp.vram.copy_within(40..960, 0);
            vdp.vram[920..960].fill(b' ');
            self.y = 23;
        }
    }
    pub fn write(&mut self, vdp: &mut Vdp, s: &str) {
        self.transcript.push_str(s);
        if self.transcript.len() > 32768 {
            self.transcript.drain(..16384);
        }
        for c in s.bytes() {
            if c == b'\n' {
                self.newline(vdp);
            } else {
                vdp.vram[self.y * 40 + self.x] = c;
                self.x += 1;
                if self.x == 40 {
                    self.newline(vdp);
                }
            }
        }
    }
    fn edit(&mut self, vdp: &mut Vdp, line: &str, cursor: usize) {
        // One scrolling editor row; the full line is committed on Enter.
        let start = cursor.saturating_sub(37);
        let row = &mut vdp.vram[self.y * 40..self.y * 40 + 40];
        row.fill(b' ');
        row[0] = b'>';
        for (i, c) in line.bytes().skip(start).take(38).enumerate() {
            row[i + 1] = c;
        }
        row[1 + cursor - start] = 127;
    }
}
#[derive(Clone)]
struct Statement {
    line: u16,
    tokens: Vec<Token>,
}
struct Loop {
    name: String,
    limit: f64,
    step: f64,
    body: usize,
}
#[derive(Default)]
pub struct Basic {
    pub console: Console,
    pub program: BTreeMap<u16, String>,
    vars: BTreeMap<String, Value>,
    input: String,
    cursor: usize,
    code: Vec<Statement>,
    pc: usize,
    loops: Vec<Loop>,
    pub running: bool,
    screen: u8,
}
impl Basic {
    pub fn boot(&mut self, vdp: &mut Vdp) {
        self.console.setup(vdp);
        self.console.write(
            vdp,
            "WEAVE BASIC 0.1\nORIGINAL OPEN SOURCE FIRMWARE\nTYPE HELP FOR COMMANDS\nREADY\n",
        );
        self.console.edit(vdp, "", 0);
    }
    fn ready(&mut self, vdp: &mut Vdp) {
        if self.screen == 0 {
            self.console.write(vdp, "READY\n");
            self.console.edit(vdp, &self.input, self.cursor);
        }
    }
    fn error(&mut self, vdp: &mut Vdp, e: String, line: u16) {
        self.running = false;
        if self.screen != 0 {
            self.screen = 0;
            self.console.setup(vdp);
        }
        self.console.write(
            vdp,
            &format!(
                "?{e}{}\n",
                if line == 0 {
                    String::new()
                } else {
                    format!(" IN {line}")
                }
            ),
        );
        self.ready(vdp);
    }
    pub fn key(&mut self, key: u8, vdp: &mut Vdp, psg: &mut Psg) {
        if key == 27 {
            self.running = false;
            self.loops.clear();
            if self.screen != 0 {
                self.screen = 0;
                self.console.setup(vdp);
            }
            self.console.write(vdp, "BREAK\n");
            self.ready(vdp);
            return;
        }
        if self.running {
            return;
        }
        if self.screen != 0 {
            if key == 13 {
                self.screen = 0;
                self.console.setup(vdp);
                self.ready(vdp);
            }
            return;
        }
        match key {
            13 => {
                let line = std::mem::take(&mut self.input);
                self.cursor = 0;
                vdp.vram[self.console.y * 40..self.console.y * 40 + 40].fill(b' ');
                self.console.x = 0;
                self.console.write(vdp, &format!(">{line}\n"));
                match self.submit(&line) {
                    Ok(true) => {}
                    Ok(false) => self.ready(vdp),
                    Err(e) => self.error(vdp, e, 0),
                }
                self.pump(vdp, psg, 100);
            }
            8 => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.input.remove(self.cursor);
                }
            }
            127 => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                }
            }
            128 => self.cursor = self.cursor.saturating_sub(1),
            129 => self.cursor = (self.cursor + 1).min(self.input.len()),
            130 => self.cursor = 0,
            131 => self.cursor = self.input.len(),
            32..=126 if self.input.len() < 240 => {
                self.input.insert(self.cursor, key as char);
                self.cursor += 1;
            }
            _ => {}
        }
        if !self.running && self.screen == 0 {
            self.console.edit(vdp, &self.input, self.cursor);
        }
    }
    /// Enqueues a source line; execution is bounded by `pump` so BREAK stays usable.
    pub fn submit(&mut self, line: &str) -> Result<bool, String> {
        if !line.is_ascii() {
            return Err("ASCII SOURCE ONLY".into());
        }
        let line = line.trim();
        if line.is_empty() {
            return Ok(false);
        }
        let n = line.bytes().take_while(u8::is_ascii_digit).count();
        if n > 0 {
            let number: u16 = line[..n].parse().map_err(|_| "BAD LINE NUMBER")?;
            if number == 0 {
                return Err("BAD LINE NUMBER".into());
            }
            let source = line[n..].trim();
            lex(source)?;
            if source.is_empty() {
                self.program.remove(&number);
            } else {
                self.program.insert(number, source.into());
            }
            return Ok(false);
        }
        let tokens = lex(line)?;
        if matches!(tokens.first(),Some(Token::Name(s)) if s=="RUN") {
            if tokens.len() != 1 {
                return Err("SYNTAX ERROR".into());
            }
            self.code.clear();
            self.vars.clear();
            self.loops.clear();
            for (number, source) in &self.program {
                self.code.extend(Self::statements(*number, lex(source)?));
            }
        } else {
            self.code = Self::statements(0, tokens);
            self.loops.clear();
        }
        self.pc = 0;
        self.running = true;
        Ok(true)
    }
    fn statements(line: u16, tokens: Vec<Token>) -> Vec<Statement> {
        tokens
            .split(|t| matches!(t,Token::Op(s) if s==":"))
            .filter(|s| !s.is_empty())
            .map(|s| Statement {
                line,
                tokens: s.to_vec(),
            })
            .collect()
    }
    fn goto(&mut self, line: u16) -> Result<(), String> {
        self.pc = self
            .code
            .iter()
            .position(|s| s.line == line)
            .ok_or("UNDEFINED LINE")?;
        Ok(())
    }
    pub fn pump(&mut self, vdp: &mut Vdp, psg: &mut Psg, budget: usize) {
        for _ in 0..budget {
            if !self.running {
                return;
            }
            if self.pc >= self.code.len() {
                self.running = false;
                self.ready(vdp);
                return;
            }
            let statement = self.code[self.pc].clone();
            self.pc += 1;
            if let Err(e) = self.execute(&statement.tokens, statement.line, vdp, psg) {
                self.error(vdp, e, statement.line);
                return;
            }
        }
    }
    fn execute(
        &mut self,
        tokens: &[Token],
        line: u16,
        vdp: &mut Vdp,
        psg: &mut Psg,
    ) -> Result<(), String> {
        let mut p = Parser::new(tokens, &self.vars);
        let command = p.name()?;
        match command.as_str() {
            "REM" => {}
            "HELP" => {
                p.done()?;
                self.console.write(vdp,"PRINT LET IF/THEN GOTO FOR/NEXT\nLIST RUN NEW END REM CLS\nSCREEN 0/1/2/3 COLOR FG,BG\nPSET X,Y,C : SOUND R,V : BEEP\nVDP R,V : VPOKE A,V\nESC: BREAK / RETURN FROM GRAPHICS\n");
            }
            "PRINT" => {
                let mut out = String::new();
                let mut newline = true;
                while p.p < tokens.len() {
                    out.push_str(&p.expr(0)?.display());
                    if p.take(";") {
                        newline = false;
                    } else if p.take(",") {
                        out.push_str("    ");
                        newline = false;
                    } else {
                        newline = true;
                        p.done()?;
                        break;
                    }
                    if p.p < tokens.len() {
                        newline = true;
                    }
                }
                if newline {
                    out.push('\n');
                }
                self.console.write(vdp, &out);
            }
            "LET" => {
                let name = p.name()?;
                p.expect("=")?;
                let v = p.expr(0)?;
                p.done()?;
                Self::check_type(&name, &v)?;
                self.vars.insert(name, v);
            }
            "IF" => {
                let yes = p.number()? != 0.0;
                p.expect("THEN")?;
                if yes {
                    if let Some(Token::Number(_)) = tokens.get(p.p) {
                        let dest = p.integer(65535)?;
                        p.done()?;
                        self.goto(dest as u16)?;
                    } else {
                        let pos = p.p;
                        self.execute(&tokens[pos..], line, vdp, psg)?;
                    }
                } else {
                    while self.pc < self.code.len() && self.code[self.pc].line == line {
                        self.pc += 1;
                    }
                }
            }
            "GOTO" => {
                let dest = p.integer(65535)?;
                p.done()?;
                self.goto(dest as u16)?;
            }
            "FOR" => {
                let name = p.name()?;
                if name.ends_with('$') {
                    return Err("TYPE MISMATCH".into());
                }
                p.expect("=")?;
                let initial = p.number()?;
                p.expect("TO")?;
                let limit = p.number()?;
                let step = if p.take("STEP") { p.number()? } else { 1.0 };
                p.done()?;
                if step == 0.0 {
                    return Err("ZERO STEP".into());
                }
                self.vars.insert(name.clone(), Value::Number(initial));
                if (step > 0.0 && initial > limit) || (step < 0.0 && initial < limit) {
                    let mut depth = 1;
                    while self.pc < self.code.len() {
                        let s = &self.code[self.pc];
                        self.pc += 1;
                        if matches!(s.tokens.first(),Some(Token::Name(s)) if s=="FOR") {
                            depth += 1;
                        }
                        if matches!(s.tokens.first(),Some(Token::Name(s)) if s=="NEXT") {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    if depth != 0 {
                        return Err("FOR WITHOUT NEXT".into());
                    }
                } else {
                    if self.loops.len() >= 64 {
                        return Err("TOO MANY LOOPS".into());
                    }
                    self.loops.push(Loop {
                        name,
                        limit,
                        step,
                        body: self.pc,
                    });
                }
            }
            "NEXT" => {
                let name = if p.p < tokens.len() {
                    Some(p.name()?)
                } else {
                    None
                };
                p.done()?;
                let frame = self.loops.last().ok_or("NEXT WITHOUT FOR")?;
                if name.as_ref().is_some_and(|n| n != &frame.name) {
                    return Err("NEXT VARIABLE MISMATCH".into());
                }
                let n = self
                    .vars
                    .get(&frame.name)
                    .ok_or("MISSING LOOP VARIABLE")?
                    .number()?
                    + frame.step;
                self.vars.insert(frame.name.clone(), Value::Number(n));
                if (frame.step > 0.0 && n <= frame.limit) || (frame.step < 0.0 && n >= frame.limit)
                {
                    self.pc = frame.body;
                } else {
                    self.loops.pop();
                }
            }
            "LIST" => {
                p.done()?;
                for (n, s) in &self.program {
                    self.console.write(vdp, &format!("{n} {s}\n"));
                }
            }
            "NEW" => {
                p.done()?;
                self.program.clear();
                self.vars.clear();
                self.running = false;
                self.ready(vdp);
            }
            "END" | "STOP" => {
                p.done()?;
                self.pc = self.code.len();
            }
            "CLS" => {
                p.done()?;
                match self.screen {
                    0 => {
                        let colors = vdp.registers[7];
                        self.console.setup(vdp);
                        vdp.registers[7] = colors;
                    }
                    1 => vdp.vram[0x1800..0x1b00].fill(32),
                    2 => vdp.vram[..0x1800].fill(0),
                    _ => vdp.vram[..0x800].fill(0),
                }
            }
            "SCREEN" => {
                let mode = p.integer(3)? as u8;
                p.done()?;
                self.screen = mode;
                match mode {
                    0 => self.console.setup(vdp),
                    1 => {
                        vdp.registers[..8].copy_from_slice(&[0, 0x40, 6, 0x80, 0, 0x36, 7, 0xf4]);
                        font::install(&mut vdp.vram, 0);
                        vdp.vram[0x1800..0x1b00].fill(32);
                        vdp.vram[0x2000..0x2020].fill(0xf4);
                    }
                    2 => {
                        vdp.registers[..8].copy_from_slice(&[2, 0x40, 6, 0xff, 3, 0x36, 7, 0xf4]);
                        vdp.vram[..0x1800].fill(0);
                        vdp.vram[0x2000..0x3800].fill(0xf4);
                        for i in 0..768 {
                            vdp.vram[0x1800 + i] = (i % 256) as u8;
                        }
                    }
                    _ => {
                        vdp.registers[..8].copy_from_slice(&[0, 0x48, 6, 0, 0, 0x36, 7, 0xf4]);
                        vdp.vram[..0x800].fill(0);
                        for i in 0..768 {
                            vdp.vram[0x1800 + i] = ((i / 128) * 32 + i % 32) as u8;
                        }
                    }
                }
                if mode != 0 {
                    vdp.vram[0x1b00] = 208;
                }
            }
            "COLOR" => {
                let fg = p.integer(15)?;
                p.expect(",")?;
                let bg = p.integer(15)?;
                p.done()?;
                vdp.registers[7] = ((fg << 4) | bg) as u8;
            }
            "PSET" => {
                let x = p.integer(255)?;
                p.expect(",")?;
                let y = p.integer(191)?;
                p.expect(",")?;
                let c = p.integer(15)? as u8;
                p.done()?;
                match self.screen {
                    2 => {
                        let offset = (y / 64) * 2048 + (y % 64 / 8) * 256 + (x / 8) * 8 + y % 8;
                        vdp.vram[offset] |= 0x80 >> (x % 8);
                        vdp.vram[0x2000 + offset] = (c << 4) | (vdp.vram[0x2000 + offset] & 15);
                    }
                    3 => {
                        let offset = ((y / 32) * 32 + x / 8) * 8 + (y / 4) % 8;
                        if x % 8 < 4 {
                            vdp.vram[offset] = (vdp.vram[offset] & 15) | (c << 4);
                        } else {
                            vdp.vram[offset] = (vdp.vram[offset] & 0xf0) | c;
                        }
                    }
                    _ => return Err("USE SCREEN 2 OR 3".into()),
                }
            }
            "VDP" | "VPOKE" | "SOUND" => {
                let a = p.integer(if command == "VDP" {
                    7
                } else if command == "SOUND" {
                    15
                } else {
                    16383
                })?;
                p.expect(",")?;
                let v = p.integer(255)? as u8;
                p.done()?;
                match command.as_str() {
                    "VDP" => vdp.registers[a] = v,
                    "VPOKE" => vdp.vram[a] = v,
                    _ => {
                        psg.select(a as u8);
                        psg.write(v);
                    }
                }
            }
            "BEEP" => {
                p.done()?;
                for (r, v) in [(0, 254), (1, 0), (7, 0x3e), (8, 12)] {
                    psg.select(r);
                    psg.write(v);
                }
            }
            _ => {
                p.p = 0;
                let name = p.name()?;
                p.expect("=")?;
                let v = p.expr(0)?;
                p.done()?;
                Self::check_type(&name, &v)?;
                self.vars.insert(name, v);
            }
        }
        Ok(())
    }
    fn check_type(name: &str, v: &Value) -> Result<(), String> {
        if name.ends_with('$') == matches!(v, Value::Text(_)) {
            Ok(())
        } else {
            Err("TYPE MISMATCH".into())
        }
    }
}
