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
            let data = name == "DATA";
            out.push(Token::Name(name));
            if rem {
                break;
            }
            if data {
                let start = p;
                let mut quoted = false;
                while p < bytes.len() {
                    if bytes[p] == b'"' {
                        quoted = !quoted;
                    }
                    if bytes[p] == b':' && !quoted {
                        break;
                    }
                    p += 1;
                }
                for item in fields(&line[start..p])? {
                    out.push(Token::Text(item));
                }
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
// DATA and INPUT share comma-separated literal fields, never expression evaluation.
fn fields(source: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (i, c) in source.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        if c == ',' && !quoted {
            result.push(field(&source[start..i])?);
            start = i + 1;
        }
    }
    if quoted {
        return Err("UNTERMINATED STRING".into());
    }
    result.push(field(&source[start..])?);
    Ok(result)
}
fn field(source: &str) -> Result<String, String> {
    let s = source.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 && !s[1..s.len() - 1].contains('"') {
        Ok(s[1..s.len() - 1].into())
    } else if s.contains('"') {
        Err("SYNTAX ERROR".into())
    } else {
        Ok(s.into())
    }
}
struct Array {
    bounds: Vec<usize>,
    values: Vec<Value>,
}
#[derive(Clone)]
struct Target {
    name: String,
    index: Option<usize>,
}
struct Parser<'a> {
    tokens: &'a [Token],
    p: usize,
    vars: &'a BTreeMap<String, Value>,
    arrays: &'a BTreeMap<String, Array>,
}
impl<'a> Parser<'a> {
    fn new(
        tokens: &'a [Token],
        vars: &'a BTreeMap<String, Value>,
        arrays: &'a BTreeMap<String, Array>,
    ) -> Self {
        Self {
            tokens,
            p: 0,
            vars,
            arrays,
        }
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
    fn indices(&mut self) -> Result<Vec<usize>, String> {
        self.expect("(")?;
        let mut result = Vec::new();
        loop {
            result.push(self.integer(4095).map_err(|error| {
                if error == "OUT OF RANGE" {
                    "SUBSCRIPT OUT OF RANGE".into()
                } else {
                    error
                }
            })?);
            if !self.take(",") {
                break;
            }
        }
        self.expect(")")?;
        if result.len() > 2 {
            return Err("SUBSCRIPT OUT OF RANGE".into());
        }
        Ok(result)
    }
    fn target(&mut self) -> Result<Target, String> {
        let name = self.name()?;
        let index = if self.is("(") {
            let indices = self.indices()?;
            let array = self.arrays.get(&name).ok_or("UNDIMENSIONED ARRAY")?;
            if indices.len() != array.bounds.len() {
                return Err("SUBSCRIPT OUT OF RANGE".into());
            }
            let mut offset = 0;
            for (i, bound) in indices.iter().zip(&array.bounds) {
                if i > bound {
                    return Err("SUBSCRIPT OUT OF RANGE".into());
                }
                offset = offset * (bound + 1) + i;
            }
            Some(offset)
        } else {
            None
        };
        Ok(Target { name, index })
    }
    fn coordinate(&mut self) -> Result<i32, String> {
        let n = self.number()?;
        if n.is_finite() && (-32768.0..=32767.0).contains(&n) {
            Ok(n.round() as i32)
        } else {
            Err("OUT OF RANGE".into())
        }
    }
    fn starts_point(&self) -> bool {
        if !self.is("(") {
            return self.is("STEP");
        }
        // A comma inside the outer parentheses distinguishes (x,y) from
        // a parenthesized x expression in the legacy PSET x,y form.
        let mut depth = 0;
        for token in &self.tokens[self.p..] {
            match token {
                Token::Op(op) if op == "(" => depth += 1,
                Token::Op(op) if op == ")" => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                Token::Op(op) if op == "," && depth == 1 => return true,
                _ => {}
            }
        }
        false
    }
    fn point(&mut self, origin: (i32, i32)) -> Result<(i32, i32), String> {
        let relative = self.take("STEP");
        self.expect("(")?;
        let x = self.coordinate()?;
        self.expect(",")?;
        let y = self.coordinate()?;
        self.expect(")")?;
        let point = if relative {
            (origin.0 + x, origin.1 + y)
        } else {
            (x, y)
        };
        if !(-32768..=32767).contains(&point.0) || !(-32768..=32767).contains(&point.1) {
            return Err("OUT OF RANGE".into());
        }
        Ok(point)
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
                    if self.is("(") && !matches!(s.as_str(), "ABS" | "INT" | "SQR" | "SIN" | "COS")
                    {
                        self.p -= 1;
                        let target = self.target()?;
                        self.arrays[&target.name].values[target.index.unwrap()].clone()
                    } else if self.take("(") {
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
        self.edit_prompt(vdp, line, cursor, ">");
    }
    fn edit_prompt(&mut self, vdp: &mut Vdp, line: &str, cursor: usize, prefix: &str) {
        // Scroll within the remaining columns, preserving output before the editor.
        // The full line is committed on Enter.
        let width = 40 - self.x - prefix.len();
        let start = cursor.saturating_sub(width - 1);
        let row = &mut vdp.vram[self.y * 40 + self.x..self.y * 40 + 40];
        row.fill(b' ');
        row[..prefix.len()].copy_from_slice(prefix.as_bytes());
        for (i, c) in line.bytes().skip(start).take(width).enumerate() {
            row[i + prefix.len()] = c;
        }
        row[prefix.len() + cursor - start] = 127;
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
struct Input {
    targets: Vec<Target>,
    prompt: String,
}
#[derive(Default)]
pub struct Basic {
    pub console: Console,
    pub program: BTreeMap<u16, String>,
    vars: BTreeMap<String, Value>,
    arrays: BTreeMap<String, Array>,
    returns: Vec<(usize, usize)>,
    data: Vec<(u16, String)>,
    data_pos: usize,
    waiting: Option<Input>,
    input: String,
    cursor: usize,
    code: Vec<Statement>,
    pc: usize,
    loops: Vec<Loop>,
    pub running: bool,
    screen: u8,
    point: (i32, i32),
    yield_graphics: bool,
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
        self.clear_control();
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
            self.clear_control();
            self.input.clear();
            self.cursor = 0;
            if self.screen != 0 {
                self.screen = 0;
                self.console.setup(vdp);
            }
            self.console.write(vdp, "BREAK\n");
            self.ready(vdp);
            return;
        }
        if self.running && self.waiting.is_none() {
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
                vdp.vram[self.console.y * 40 + self.console.x..self.console.y * 40 + 40].fill(b' ');
                let prefix = if self.waiting.is_some() { "" } else { ">" };
                self.console.write(vdp, &format!("{prefix}{line}\n"));
                if self.waiting.is_some() {
                    self.accept_input(&line, vdp);
                } else {
                    match self.submit(&line) {
                        Ok(true) => {}
                        Ok(false) => self.ready(vdp),
                        Err(e) => self.error(vdp, e, 0),
                    }
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
        if (!self.running || self.waiting.is_some()) && self.screen == 0 {
            self.edit(vdp);
        }
    }
    fn edit(&mut self, vdp: &mut Vdp) {
        // INPUT already wrote its prompt and left the console at the answer column.
        let prompt = if self.waiting.is_some() { "" } else { ">" };
        self.console
            .edit_prompt(vdp, &self.input, self.cursor, prompt);
    }
    /// Enqueues a source line; execution is bounded by `pump` so BREAK stays usable.
    pub fn submit(&mut self, line: &str) -> Result<bool, String> {
        if self.waiting.is_some() {
            return Err("INPUT PENDING".into());
        }
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
            self.data_pos = 0;
            return Ok(false);
        }
        let tokens = lex(line)?;
        if matches!(tokens.first(),Some(Token::Name(s)) if s=="RUN") {
            if tokens.len() != 1 {
                return Err("SYNTAX ERROR".into());
            }
            self.code.clear();
            self.vars.clear();
            self.arrays.clear();
            self.data_pos = 0;
            self.clear_control();
            for (number, source) in &self.program {
                self.code.extend(Self::statements(*number, lex(source)?));
            }
        } else {
            self.code = Self::statements(0, tokens);
            // Direct GOSUB/GOTO can reach stored lines; END isolates the direct tail.
            self.code.push(Statement {
                line: 0,
                tokens: vec![Token::Name("END".into())],
            });
            for (number, source) in &self.program {
                self.code.extend(Self::statements(*number, lex(source)?));
            }
            self.clear_control();
        }
        self.data.clear();
        for (number, source) in &self.program {
            for statement in Self::statements(*number, lex(source)?) {
                if matches!(statement.tokens.first(), Some(Token::Name(s)) if s == "DATA") {
                    for token in &statement.tokens[1..] {
                        if let Token::Text(s) = token {
                            self.data.push((*number, s.clone()));
                        }
                    }
                }
            }
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
        if line == 0 {
            return Err("UNDEFINED LINE".into());
        }
        self.pc = self
            .code
            .iter()
            .position(|s| s.line == line)
            .ok_or("UNDEFINED LINE")?;
        Ok(())
    }
    pub fn pump(&mut self, vdp: &mut Vdp, psg: &mut Psg, budget: usize) {
        for _ in 0..budget {
            if !self.running || self.waiting.is_some() {
                return;
            }
            if self.pc >= self.code.len() {
                self.running = false;
                self.clear_control();
                self.ready(vdp);
                return;
            }
            let statement = self.code[self.pc].clone();
            self.pc += 1;
            if let Err(e) = self.execute(&statement.tokens, statement.line, vdp, psg) {
                self.error(vdp, e, statement.line);
                return;
            }
            // Large primitives are bounded, but should not run 200 times in one
            // frame. PSET remains cheap enough for the ordinary statement budget.
            if self.yield_graphics {
                self.yield_graphics = false;
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
        let mut p = Parser::new(tokens, &self.vars, &self.arrays);
        let command = p.name()?;
        match command.as_str() {
            "REM" | "DATA" => {}
            "HELP" => {
                p.done()?;
                self.console.write(vdp,"WEAVE BASIC: MSX-INSPIRED, ORIGINAL\nPRINT LET IF/THEN GOTO FOR/NEXT\nGOSUB/RETURN DIM A(10),S$(2,3)\nDATA 2,HELLO : READ A,S$ : RESTORE\nINPUT \"NAME\";N$\nLIST RUN NEW END STOP REM CLS\nSCREEN 0/1/2/3 COLOR FG,BG\nPSET (X,Y),C : LINE (X,Y)-(X,Y),C\nLINE (0,0)-(80,80),10,BF\nCIRCLE (128,96),40,10\nPAINT (128,96),10,15\nSOUND R,V BEEP VDP R,V VPOKE A,V\nESC: BREAK / ENTER: EXIT GRAPHICS\n");
            }
            "PRINT" => {
                let mut out = String::new();
                let mut newline = true;
                while p.p < tokens.len() {
                    out.push_str(&p.expr(0)?.display());
                    if p.take(";") {
                        newline = false;
                    } else if p.take(",") {
                        let column = (self.console.x + out.len()) % 40;
                        out.push_str(&" ".repeat(14 - column % 14));
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
                let target = p.target()?;
                p.expect("=")?;
                let v = p.expr(0)?;
                p.done()?;
                self.assign(&target, v)?;
            }
            "DIM" => {
                let mut definitions = Vec::new();
                loop {
                    let name = p.name()?;
                    let bounds = p.indices()?;
                    let size: usize = bounds.iter().map(|n| n + 1).product();
                    if self.arrays.contains_key(&name)
                        || definitions.iter().any(|(n, _, _)| n == &name)
                    {
                        return Err("REDIMENSIONED ARRAY".into());
                    }
                    definitions.push((name, bounds, size));
                    if !p.take(",") {
                        break;
                    }
                }
                p.done()?;
                let size: usize = definitions.iter().map(|(_, _, size)| size).sum();
                if size + self.arrays.values().map(|a| a.values.len()).sum::<usize>() > 16384 {
                    return Err("ARRAY LIMIT".into());
                }
                for (name, bounds, size) in definitions {
                    let default = if name.ends_with('$') {
                        Value::Text(String::new())
                    } else {
                        Value::Number(0.0)
                    };
                    self.arrays.insert(
                        name,
                        Array {
                            bounds,
                            values: vec![default; size],
                        },
                    );
                }
            }
            "READ" => {
                // Resolve each index after preceding READ assignments (READ I,A(I)).
                let mut pos = p.p;
                loop {
                    let mut item = Parser::new(&tokens[pos..], &self.vars, &self.arrays);
                    let target = item.target()?;
                    let more = item.take(",");
                    if !more {
                        item.done()?;
                    }
                    pos += item.p;
                    let s = &self.data.get(self.data_pos).ok_or("OUT OF DATA")?.1;
                    let value = Self::literal(&target, s)?;
                    self.assign(&target, value)?;
                    self.data_pos += 1;
                    if !more {
                        break;
                    }
                }
            }
            "INPUT" => {
                let prompt = if let Some(Token::Text(s)) = p.tokens.get(p.p) {
                    let s = s.clone();
                    p.p += 1;
                    p.expect(";")?;
                    format!("{s}? ")
                } else {
                    "? ".into()
                };
                let mut targets = Vec::new();
                loop {
                    targets.push(p.target()?);
                    if !p.take(",") {
                        break;
                    }
                }
                p.done()?;
                if self.screen != 0 {
                    return Err("USE SCREEN 0 FOR INPUT".into());
                }
                self.console.write(vdp, &prompt);
                self.waiting = Some(Input { targets, prompt });
                self.edit(vdp);
            }
            "RESTORE" => {
                let line = if p.p < tokens.len() {
                    p.integer(65535)? as u16
                } else {
                    0
                };
                p.done()?;
                if line != 0 && !self.program.contains_key(&line) {
                    return Err("UNDEFINED LINE".into());
                }
                self.data_pos = self
                    .data
                    .iter()
                    .position(|(n, _)| *n >= line)
                    .unwrap_or(self.data.len());
            }
            "GOSUB" => {
                let dest = p.integer(65535)? as u16;
                p.done()?;
                if self.returns.len() >= 64 {
                    return Err("TOO MANY GOSUBS".into());
                }
                let frame = (self.pc, self.loops.len());
                self.goto(dest)?;
                self.returns.push(frame);
            }
            "RETURN" => {
                p.done()?;
                let (pc, depth) = self.returns.pop().ok_or("RETURN WITHOUT GOSUB")?;
                self.pc = pc;
                self.loops.truncate(depth);
            }
            "RUN" => {
                p.done()?;
                self.submit("RUN")?;
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
                    if line == 0 {
                        self.pc = self.code.len();
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
                        if s.line == 0
                            && matches!(s.tokens.first(),Some(Token::Name(s)) if s=="END")
                        {
                            break;
                        }
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
                if self
                    .returns
                    .last()
                    .is_some_and(|(_, depth)| self.loops.len() <= *depth)
                {
                    return Err("NEXT WITHOUT FOR".into());
                }
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
                self.arrays.clear();
                self.data.clear();
                self.data_pos = 0;
                self.clear_control();
                self.running = false;
                self.ready(vdp);
            }
            "END" | "STOP" => {
                p.done()?;
                self.pc = self.code.len();
                self.clear_control();
                if command == "STOP" {
                    if self.screen != 0 {
                        self.screen = 0;
                        self.console.setup(vdp);
                    }
                    self.console.write(
                        vdp,
                        &if line == 0 {
                            "BREAK\n".into()
                        } else {
                            format!("BREAK IN {line}\n")
                        },
                    );
                }
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
                self.point = (0, 0);
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
            "PSET" | "LINE" | "CIRCLE" | "PAINT" => {
                if !matches!(self.screen, 2 | 3) {
                    return Err("USE SCREEN 2 OR 3".into());
                }
                let origin = self.point;
                let start = if command == "LINE" && p.is("-") {
                    origin
                } else if command == "PSET" && !p.starts_point() {
                    let x = p.coordinate()?;
                    p.expect(",")?;
                    (x, p.coordinate()?)
                } else {
                    p.point(origin)?
                };
                let end = if command == "LINE" {
                    p.expect("-")?;
                    p.point(start)?
                } else {
                    start
                };
                let radius = if command == "CIRCLE" {
                    p.expect(",")?;
                    p.integer(1024)? as i32
                } else {
                    0
                };
                let mut color = vdp.registers[7] >> 4;
                let mut border = None;
                let mut box_mode = String::new();
                if p.take(",") {
                    if !p.is(",") && p.p < tokens.len() {
                        color = p.integer(15)? as u8;
                    }
                    if p.take(",") {
                        if command == "LINE" {
                            box_mode = p.name()?;
                        } else if command == "PAINT" {
                            border = Some(p.integer(15)? as u8);
                        } else {
                            return Err("SYNTAX ERROR".into());
                        }
                    }
                }
                p.done()?;
                if !box_mode.is_empty() && !matches!(box_mode.as_str(), "B" | "BF") {
                    return Err("SYNTAX ERROR".into());
                }
                match command.as_str() {
                    "PSET" => self.pixel(vdp, start.0, start.1, color),
                    "LINE" => {
                        if box_mode.is_empty() {
                            self.draw_line(vdp, start, end, color);
                        } else {
                            let (x0, x1) = (start.0.min(end.0), start.0.max(end.0));
                            let (y0, y1) = (start.1.min(end.1), start.1.max(end.1));
                            if box_mode == "BF" {
                                for y in y0.max(0)..=y1.min(191) {
                                    for x in x0.max(0)..=x1.min(255) {
                                        self.pixel(vdp, x, y, color);
                                    }
                                }
                            } else {
                                for (a, b) in [
                                    ((x0, y0), (x1, y0)),
                                    ((x1, y0), (x1, y1)),
                                    ((x1, y1), (x0, y1)),
                                    ((x0, y1), (x0, y0)),
                                ] {
                                    self.draw_line(vdp, a, b, color);
                                }
                            }
                        }
                    }
                    "CIRCLE" => {
                        // Integer midpoint circle: bounded by the documented radius limit.
                        let (mut x, mut y, mut error) = (radius, 0, 1 - radius);
                        while x >= y {
                            for (dx, dy) in [
                                (x, y),
                                (y, x),
                                (-y, x),
                                (-x, y),
                                (-x, -y),
                                (-y, -x),
                                (y, -x),
                                (x, -y),
                            ] {
                                self.pixel(vdp, start.0 + dx, start.1 + dy, color);
                            }
                            y += 1;
                            if error < 0 {
                                error += 2 * y + 1;
                            } else {
                                x -= 1;
                                error += 2 * (y - x) + 1;
                            }
                        }
                    }
                    _ => self.paint(vdp, start, color, border.unwrap_or(color)),
                }
                self.point = end;
                self.yield_graphics = command != "PSET";
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
                let target = p.target()?;
                p.expect("=")?;
                let v = p.expr(0)?;
                p.done()?;
                self.assign(&target, v)?;
            }
        }
        Ok(())
    }
    fn clear_control(&mut self) {
        self.loops.clear();
        self.returns.clear();
        self.waiting = None;
        self.yield_graphics = false;
    }
    fn assign(&mut self, target: &Target, value: Value) -> Result<(), String> {
        Self::check_type(&target.name, &value)?;
        if let Some(index) = target.index {
            self.arrays
                .get_mut(&target.name)
                .ok_or("UNDIMENSIONED ARRAY")?
                .values[index] = value;
        } else {
            self.vars.insert(target.name.clone(), value);
        }
        Ok(())
    }
    fn literal(target: &Target, s: &str) -> Result<Value, String> {
        if target.name.ends_with('$') {
            Ok(Value::Text(s.into()))
        } else {
            let n: f64 = s.parse().map_err(|_| "TYPE MISMATCH")?;
            if !n.is_finite() {
                return Err("TYPE MISMATCH".into());
            }
            Ok(Value::Number(n))
        }
    }
    fn accept_input(&mut self, source: &str, vdp: &mut Vdp) {
        let pending = self.waiting.take().unwrap();
        let values = fields(source).and_then(|fields| {
            if fields.len() != pending.targets.len() {
                return Err("FIELD COUNT".into());
            }
            pending
                .targets
                .iter()
                .zip(&fields)
                .map(|(target, s)| Self::literal(target, s))
                .collect::<Result<Vec<_>, _>>()
        });
        match values {
            Ok(values) => {
                for (target, value) in pending.targets.iter().zip(values) {
                    // Resolved targets cannot change while the interpreter is waiting.
                    self.assign(target, value).expect("validated INPUT target");
                }
            }
            Err(_) => {
                self.console
                    .write(vdp, &format!("?REDO FROM START\n{}", pending.prompt));
                self.waiting = Some(pending);
            }
        }
    }
    fn pixel_offset(&self, x: usize, y: usize) -> usize {
        if self.screen == 2 {
            (y / 64) * 2048 + (y % 64 / 8) * 256 + (x / 8) * 8 + y % 8
        } else {
            ((y / 32) * 32 + x / 8) * 8 + (y / 4) % 8
        }
    }
    fn pixel_color(&self, vdp: &Vdp, x: usize, y: usize) -> u8 {
        let offset = self.pixel_offset(x, y);
        if self.screen == 2 {
            let pair = vdp.vram[0x2000 + offset];
            if vdp.vram[offset] & (0x80 >> (x % 8)) != 0 {
                pair >> 4
            } else {
                pair & 15
            }
        } else if x % 8 < 4 {
            vdp.vram[offset] >> 4
        } else {
            vdp.vram[offset] & 15
        }
    }
    fn pixel(&self, vdp: &mut Vdp, x: i32, y: i32, color: u8) {
        if !(0..256).contains(&x) || !(0..192).contains(&y) {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        let offset = self.pixel_offset(x, y);
        if self.screen == 2 {
            let bit = 0x80 >> (x % 8);
            let pair = &mut vdp.vram[0x2000 + offset];
            if color == *pair & 15 {
                vdp.vram[offset] &= !bit;
            } else {
                *pair = (color << 4) | (*pair & 15);
                vdp.vram[offset] |= bit;
            }
        } else if x % 8 < 4 {
            vdp.vram[offset] = (vdp.vram[offset] & 15) | (color << 4);
        } else {
            vdp.vram[offset] = (vdp.vram[offset] & 0xf0) | color;
        }
    }
    fn draw_line(&self, vdp: &mut Vdp, start: (i32, i32), end: (i32, i32), color: u8) {
        let (mut x, mut y) = start;
        let dx = (end.0 - x).abs();
        let dy = -(end.1 - y).abs();
        let sx = if x < end.0 { 1 } else { -1 };
        let sy = if y < end.1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            self.pixel(vdp, x, y, color);
            if (x, y) == end {
                break;
            }
            let e = 2 * error;
            if e >= dy {
                error += dy;
                x += sx;
            }
            if e <= dx {
                error += dx;
                y += sy;
            }
        }
    }
    fn paint(&self, vdp: &mut Vdp, start: (i32, i32), color: u8, border: u8) {
        if !(0..256).contains(&start.0) || !(0..192).contains(&start.1) {
            return;
        }
        let scale = if self.screen == 3 { 4 } else { 1 };
        let (width, height) = (256 / scale, 192 / scale);
        // Snapshot boundaries before modifying TMS9918 color pairs. Visit each
        // logical pixel once, including when the fill and background colors match.
        let mut seen = vec![false; width * height];
        for y in 0..height {
            for x in 0..width {
                seen[y * width + x] = self.pixel_color(vdp, x * scale, y * scale) == border;
            }
        }
        let first = (start.1 as usize / scale) * width + start.0 as usize / scale;
        if seen[first] {
            return;
        }
        seen[first] = true;
        let mut pending = vec![first];
        while let Some(i) = pending.pop() {
            let (x, y) = (i % width, i / width);
            self.pixel(vdp, (x * scale) as i32, (y * scale) as i32, color);
            for next in [
                if x > 0 { Some(i - 1) } else { None },
                if x + 1 < width { Some(i + 1) } else { None },
                if y > 0 { Some(i - width) } else { None },
                if y + 1 < height {
                    Some(i + width)
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            {
                if !seen[next] {
                    seen[next] = true;
                    pending.push(next);
                }
            }
        }
    }
    fn check_type(name: &str, v: &Value) -> Result<(), String> {
        if name.ends_with('$') == matches!(v, Value::Text(_)) {
            Ok(())
        } else {
            Err("TYPE MISMATCH".into())
        }
    }
}
