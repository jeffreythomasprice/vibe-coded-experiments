use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use anyhow::{Result, anyhow};
use ariadne::{Color, Label, Report, ReportKind, Source};
use chumsky::prelude::*;

use super::instruction::{Instruction, Operand, Program, RegisterBank, RegisterId};

// ---------------------------------------------------------------------------
// AST types
// ---------------------------------------------------------------------------

type Span = Range<usize>;

#[derive(Debug, Clone)]
struct Spanned<T> {
    node: T,
    span: Span,
}

#[derive(Debug, Clone)]
enum AstOperand {
    Register(RegisterId),
    IntLiteral(i32),
    FloatLiteral(f32),
    LabelRef(String),
}

#[derive(Debug, Clone)]
struct AstInstruction {
    mnemonic: Spanned<String>,
    operands: Vec<Spanned<AstOperand>>,
}

#[derive(Debug, Clone)]
struct AstStatement {
    label: Option<Spanned<String>>,
    instruction: Option<AstInstruction>,
}

// ---------------------------------------------------------------------------
// Assembler errors
// ---------------------------------------------------------------------------

struct AssemblerError {
    span: Span,
    message: String,
}

fn render_error(source: &str, err: &AssemblerError) -> String {
    let mut buf = Vec::new();
    Report::build(ReportKind::Error, (), err.span.start)
        .with_message(&err.message)
        .with_label(
            Label::new(err.span.clone())
                .with_message(&err.message)
                .with_color(Color::Red),
        )
        .finish()
        .write(Source::from(source), &mut buf)
        .unwrap();
    String::from_utf8(buf).unwrap()
}

fn render_parse_errors(source: &str, errs: Vec<Simple<char>>) -> String {
    let mut buf = Vec::new();
    for err in &errs {
        let span = err.span();
        let msg = format!(
            "unexpected {}{}",
            match err.found() {
                Some(c) => format!("'{c}'"),
                None => "end of input".to_string(),
            },
            if err.expected().len() > 0 {
                let expected: Vec<_> = err
                    .expected()
                    .map(|e| match e {
                        Some(c) => format!("'{c}'"),
                        None => "end of input".to_string(),
                    })
                    .collect();
                format!(", expected {}", expected.join(" or "))
            } else {
                String::new()
            }
        );
        let mut out = Vec::new();
        Report::build(ReportKind::Error, (), span.start)
            .with_message(&msg)
            .with_label(
                Label::new(span)
                    .with_message(&msg)
                    .with_color(Color::Red),
            )
            .finish()
            .write(Source::from(source), &mut out)
            .unwrap();
        buf.extend(out);
    }
    String::from_utf8(buf).unwrap()
}

// ---------------------------------------------------------------------------
// Comment stripping (span-preserving)
// ---------------------------------------------------------------------------

fn strip_comments_preserving_spans(source: &str) -> String {
    let mut out = vec![b' '; source.len()];
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut in_block = false;

    while i < bytes.len() {
        if in_block {
            if bytes[i] == b'\n' {
                out[i] = b'\n';
            }
            if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                i += 2;
                in_block = false;
                continue;
            }
            i += 1;
            continue;
        }

        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            in_block = true;
            continue;
        }

        if bytes[i] == b';' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        out[i] = bytes[i];
        i += 1;
    }

    String::from_utf8(out).unwrap()
}

// ---------------------------------------------------------------------------
// Register parsing
// ---------------------------------------------------------------------------

fn parse_register_name(s: &str) -> Option<RegisterId> {
    match s.to_lowercase().as_str() {
        "r0" => Some(RegisterId::R0),
        "r1" => Some(RegisterId::R1),
        "r2" => Some(RegisterId::R2),
        "r3" => Some(RegisterId::R3),
        "r4" => Some(RegisterId::R4),
        "r5" => Some(RegisterId::R5),
        "r6" => Some(RegisterId::R6),
        "r7" => Some(RegisterId::R7),
        "f0" => Some(RegisterId::F0),
        "f1" => Some(RegisterId::F1),
        "f2" => Some(RegisterId::F2),
        "f3" => Some(RegisterId::F3),
        "f4" => Some(RegisterId::F4),
        "f5" => Some(RegisterId::F5),
        "f6" => Some(RegisterId::F6),
        "f7" => Some(RegisterId::F7),
        "sp" => Some(RegisterId::Sp),
        "spd" => Some(RegisterId::Spd),
        "trn" => Some(RegisterId::Trn),
        "trt" => Some(RegisterId::Trt),
        "hp" => Some(RegisterId::Hp),
        "x" => Some(RegisterId::X),
        "y" => Some(RegisterId::Y),
        "hdg" => Some(RegisterId::Hdg),
        "thr" => Some(RegisterId::Thr),
        "spd_cur" => Some(RegisterId::SpdCur),
        "sc_dist" => Some(RegisterId::ScDist),
        "sc_type" => Some(RegisterId::ScType),
        "sc_id" => Some(RegisterId::ScId),
        "tick" => Some(RegisterId::Tick),
        "rnd" => Some(RegisterId::Rnd),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// chumsky parser
// ---------------------------------------------------------------------------

fn inline_ws() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    filter(|c: &char| *c == ' ' || *c == '\t')
        .repeated()
        .ignored()
}

fn ident_raw() -> impl Parser<char, (String, Span), Error = Simple<char>> + Clone {
    filter(|c: &char| c.is_ascii_alphabetic() || *c == '_')
        .then(filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_').repeated())
        .map_with_span(|(first, rest), span: Span| {
            let mut s = String::with_capacity(1 + rest.len());
            s.push(first);
            s.extend(rest);
            (s, span)
        })
}

fn register_parser() -> impl Parser<char, Spanned<AstOperand>, Error = Simple<char>> + Clone {
    ident_raw().try_map(|(s, span), _| {
        parse_register_name(&s)
            .map(|r| Spanned {
                node: AstOperand::Register(r),
                span: span.clone(),
            })
            .ok_or(Simple::custom(span, format!("not a register: {s}")))
    })
}

fn hex_literal_parser() -> impl Parser<char, Spanned<AstOperand>, Error = Simple<char>> + Clone {
    just('0')
        .then(one_of("xX"))
        .then(filter(|c: &char| c.is_ascii_hexdigit()).repeated().at_least(1))
        .map_with_span(|((_, _), digits), span: Span| {
            let hex_str: String = digits.into_iter().collect();
            let val = i64::from_str_radix(&hex_str, 16).unwrap_or(0) as i32;
            Spanned {
                node: AstOperand::IntLiteral(val),
                span,
            }
        })
}

fn float_literal_parser() -> impl Parser<char, Spanned<AstOperand>, Error = Simple<char>> + Clone {
    let digit = filter(|c: &char| c.is_ascii_digit());
    let digits = digit.repeated().at_least(1);

    let with_dot = just('-')
        .or_not()
        .then(digits)
        .then(just('.'))
        .then(filter(|c: &char| c.is_ascii_digit()).repeated())
        .then(just('f').or_not())
        .map_with_span(|((((neg, int_d), _), frac_d), _suffix), span: Span| {
            let mut s = String::new();
            if neg.is_some() {
                s.push('-');
            }
            s.extend(int_d);
            s.push('.');
            s.extend(frac_d);
            let val: f32 = s.parse().unwrap_or(0.0);
            Spanned {
                node: AstOperand::FloatLiteral(val),
                span,
            }
        });

    let with_f_suffix = just('-')
        .or_not()
        .then(digits)
        .then(just('f'))
        .map_with_span(|((neg, int_d), _), span: Span| {
            let mut s = String::new();
            if neg.is_some() {
                s.push('-');
            }
            s.extend(int_d);
            let val: f32 = s.parse().unwrap_or(0.0);
            Spanned {
                node: AstOperand::FloatLiteral(val),
                span,
            }
        });

    with_dot.or(with_f_suffix)
}

fn int_literal_parser() -> impl Parser<char, Spanned<AstOperand>, Error = Simple<char>> + Clone {
    just('-')
        .or_not()
        .then(filter(|c: &char| c.is_ascii_digit()).repeated().at_least(1))
        .map_with_span(|(neg, digits), span: Span| {
            let mut s = String::new();
            if neg.is_some() {
                s.push('-');
            }
            s.extend(digits);
            let val: i32 = s.parse().unwrap_or(0);
            Spanned {
                node: AstOperand::IntLiteral(val),
                span,
            }
        })
}

fn label_ref_parser() -> impl Parser<char, Spanned<AstOperand>, Error = Simple<char>> + Clone {
    ident_raw().map(|(s, span)| Spanned {
        node: AstOperand::LabelRef(s),
        span,
    })
}

fn operand_parser() -> impl Parser<char, Spanned<AstOperand>, Error = Simple<char>> + Clone {
    choice((
        register_parser(),
        float_literal_parser(),
        hex_literal_parser(),
        int_literal_parser(),
        label_ref_parser(),
    ))
}

fn operand_list_parser(
) -> impl Parser<char, Vec<Spanned<AstOperand>>, Error = Simple<char>> + Clone {
    operand_parser()
        .separated_by(inline_ws().then(just(',')).then(inline_ws()))
        .at_least(1)
}

fn mnemonic_parser() -> impl Parser<char, Spanned<String>, Error = Simple<char>> + Clone {
    ident_raw().map(|(s, span)| Spanned {
        node: s.to_uppercase(),
        span,
    })
}

fn statement_parser() -> impl Parser<char, AstStatement, Error = Simple<char>> + Clone {
    let label_def = ident_raw()
        .then_ignore(just(':'))
        .map(|(s, span)| Spanned { node: s, span });

    let instruction = mnemonic_parser()
        .then(inline_ws().ignore_then(operand_list_parser()).or_not())
        .map(|(mnemonic, operands)| AstInstruction {
            mnemonic,
            operands: operands.unwrap_or_default(),
        });

    inline_ws()
        .ignore_then(
            label_def
                .then_ignore(inline_ws())
                .or_not()
                .then(instruction.or_not())
                .map(|(label, instruction)| AstStatement {
                    label,
                    instruction,
                }),
        )
        .then_ignore(inline_ws())
}

fn program_parser() -> impl Parser<char, Vec<AstStatement>, Error = Simple<char>> {
    statement_parser()
        .separated_by(just('\n').repeated().at_least(1))
        .allow_leading()
        .allow_trailing()
        .then_ignore(end())
}

// ---------------------------------------------------------------------------
// Pass 2: label resolution + instruction emission
// ---------------------------------------------------------------------------

struct EmitContext<'a> {
    source: &'a str,
    label_map: HashMap<String, usize>,
}

impl<'a> EmitContext<'a> {
    fn err(&self, span: Span, msg: impl fmt::Display) -> anyhow::Error {
        let ae = AssemblerError {
            span,
            message: msg.to_string(),
        };
        anyhow!("{}", render_error(self.source, &ae))
    }

    fn resolve_operand(&self, op: &Spanned<AstOperand>) -> Result<Operand> {
        match &op.node {
            AstOperand::Register(r) => Ok(Operand::Register(*r)),
            AstOperand::IntLiteral(i) => Ok(Operand::Immediate(*i)),
            AstOperand::FloatLiteral(f) => Ok(Operand::ImmediateFloat(*f)),
            AstOperand::LabelRef(name) => {
                let key = name.to_lowercase();
                self.label_map
                    .get(&key)
                    .map(|&addr| Operand::Immediate(addr as i32))
                    .ok_or_else(|| self.err(op.span.clone(), format!("undefined label '{name}'")))
            }
        }
    }

    fn resolve_register(&self, op: &Spanned<AstOperand>) -> Result<RegisterId> {
        match &op.node {
            AstOperand::Register(r) => Ok(*r),
            _ => Err(self.err(op.span.clone(), format!("expected register, got {:?}", op.node))),
        }
    }

    fn resolve_label_addr(&self, op: &Spanned<AstOperand>) -> Result<usize> {
        match &op.node {
            AstOperand::LabelRef(name) => {
                let key = name.to_lowercase();
                self.label_map
                    .get(&key)
                    .copied()
                    .ok_or_else(|| self.err(op.span.clone(), format!("undefined label '{name}'")))
            }
            AstOperand::IntLiteral(i) => Ok(*i as usize),
            _ => Err(self.err(op.span.clone(), "expected label or address".to_string())),
        }
    }

    fn expect_operand_count(
        &self,
        instr: &AstInstruction,
        expected: usize,
    ) -> Result<()> {
        if instr.operands.len() != expected {
            return Err(self.err(
                instr.mnemonic.span.clone(),
                format!(
                    "'{}' expects {} operand(s), got {}",
                    instr.mnemonic.node,
                    expected,
                    instr.operands.len()
                ),
            ));
        }
        Ok(())
    }

    fn expect_writable(&self, reg: RegisterId, span: Span) -> Result<()> {
        if !reg.is_writable() {
            return Err(self.err(span, format!("register {reg:?} is read-only")));
        }
        Ok(())
    }

    fn expect_bank(&self, reg: RegisterId, expected: RegisterBank, span: Span) -> Result<()> {
        if reg.bank() != expected {
            let bank_name = match expected {
                RegisterBank::Integer => "integer",
                RegisterBank::Float => "float",
            };
            return Err(self.err(
                span,
                format!("expected {bank_name} register, got {reg:?}"),
            ));
        }
        Ok(())
    }

    fn expect_int_operand(&self, op: &Spanned<AstOperand>) -> Result<()> {
        if let AstOperand::Register(reg) = &op.node {
            self.expect_bank(*reg, RegisterBank::Integer, op.span.clone())?;
        }
        Ok(())
    }

    fn expect_float_operand(&self, op: &Spanned<AstOperand>) -> Result<()> {
        if let AstOperand::Register(reg) = &op.node {
            self.expect_bank(*reg, RegisterBank::Float, op.span.clone())?;
        }
        Ok(())
    }

    fn emit_int_alu3(
        &self,
        instr: &AstInstruction,
        make: impl FnOnce(RegisterId, Operand, Operand) -> Instruction,
    ) -> Result<Instruction> {
        self.expect_operand_count(instr, 3)?;
        let dst = self.resolve_register(&instr.operands[0])?;
        let src1 = self.resolve_operand(&instr.operands[1])?;
        let src2 = self.resolve_operand(&instr.operands[2])?;
        self.expect_writable(dst, instr.operands[0].span.clone())?;
        self.expect_bank(dst, RegisterBank::Integer, instr.operands[0].span.clone())?;
        self.expect_int_operand(&instr.operands[1])?;
        self.expect_int_operand(&instr.operands[2])?;
        Ok(make(dst, src1, src2))
    }

    fn emit_int_alu2(
        &self,
        instr: &AstInstruction,
        make: impl FnOnce(RegisterId, Operand) -> Instruction,
    ) -> Result<Instruction> {
        self.expect_operand_count(instr, 2)?;
        let dst = self.resolve_register(&instr.operands[0])?;
        let src = self.resolve_operand(&instr.operands[1])?;
        self.expect_writable(dst, instr.operands[0].span.clone())?;
        self.expect_bank(dst, RegisterBank::Integer, instr.operands[0].span.clone())?;
        self.expect_int_operand(&instr.operands[1])?;
        Ok(make(dst, src))
    }

    fn emit_float_alu3(
        &self,
        instr: &AstInstruction,
        make: impl FnOnce(RegisterId, Operand, Operand) -> Instruction,
    ) -> Result<Instruction> {
        self.expect_operand_count(instr, 3)?;
        let dst = self.resolve_register(&instr.operands[0])?;
        let src1 = self.resolve_operand(&instr.operands[1])?;
        let src2 = self.resolve_operand(&instr.operands[2])?;
        self.expect_writable(dst, instr.operands[0].span.clone())?;
        self.expect_bank(dst, RegisterBank::Float, instr.operands[0].span.clone())?;
        self.expect_float_operand(&instr.operands[1])?;
        self.expect_float_operand(&instr.operands[2])?;
        Ok(make(dst, src1, src2))
    }

    fn emit_float_alu2(
        &self,
        instr: &AstInstruction,
        make: impl FnOnce(RegisterId, Operand) -> Instruction,
    ) -> Result<Instruction> {
        self.expect_operand_count(instr, 2)?;
        let dst = self.resolve_register(&instr.operands[0])?;
        let src = self.resolve_operand(&instr.operands[1])?;
        self.expect_writable(dst, instr.operands[0].span.clone())?;
        self.expect_bank(dst, RegisterBank::Float, instr.operands[0].span.clone())?;
        self.expect_float_operand(&instr.operands[1])?;
        Ok(make(dst, src))
    }

    fn emit_jump(
        &self,
        instr: &AstInstruction,
        make: impl FnOnce(usize) -> Instruction,
    ) -> Result<Instruction> {
        self.expect_operand_count(instr, 1)?;
        let addr = self.resolve_label_addr(&instr.operands[0])?;
        Ok(make(addr))
    }
}

fn build_label_map(source: &str, stmts: &[AstStatement]) -> Result<HashMap<String, usize>> {
    let mut map = HashMap::new();
    let mut instruction_index = 0;

    for stmt in stmts {
        if let Some(label) = &stmt.label {
            let key = label.node.to_lowercase();
            if map.contains_key(&key) {
                let ae = AssemblerError {
                    span: label.span.clone(),
                    message: format!("duplicate label '{}'", label.node),
                };
                return Err(anyhow!("{}", render_error(source, &ae)));
            }
            map.insert(key, instruction_index);
        }
        if stmt.instruction.is_some() {
            instruction_index += 1;
        }
    }

    Ok(map)
}

fn emit(source: &str, stmts: &[AstStatement]) -> Result<Program> {
    let label_map = build_label_map(source, stmts)?;
    let ctx = EmitContext { source, label_map };
    let mut instructions = Vec::new();

    for stmt in stmts {
        let Some(instr) = &stmt.instruction else {
            continue;
        };

        match instr.mnemonic.node.as_str() {
            "INC" => {
                ctx.expect_operand_count(instr, 1)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                ctx.expect_bank(dst, RegisterBank::Integer, instr.operands[0].span.clone())?;
                instructions.push(Instruction::Add {
                    dst,
                    src1: Operand::Register(dst),
                    src2: Operand::Immediate(1),
                });
            }
            "DEC" => {
                ctx.expect_operand_count(instr, 1)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                ctx.expect_bank(dst, RegisterBank::Integer, instr.operands[0].span.clone())?;
                instructions.push(Instruction::Sub {
                    dst,
                    src1: Operand::Register(dst),
                    src2: Operand::Immediate(1),
                });
            }
            "MOVI" => {
                ctx.expect_operand_count(instr, 2)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                let src = ctx.resolve_operand(&instr.operands[1])?;
                instructions.push(Instruction::Mov { dst, src });
            }

            "ADD" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Add { dst: d, src1: s1, src2: s2 })?),
            "SUB" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Sub { dst: d, src1: s1, src2: s2 })?),
            "MUL" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Mul { dst: d, src1: s1, src2: s2 })?),
            "DIV" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Div { dst: d, src1: s1, src2: s2 })?),
            "MOD" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Mod { dst: d, src1: s1, src2: s2 })?),
            "AND" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::And { dst: d, src1: s1, src2: s2 })?),
            "OR"  => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Or  { dst: d, src1: s1, src2: s2 })?),
            "XOR" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Xor { dst: d, src1: s1, src2: s2 })?),
            "SHL" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Shl { dst: d, src1: s1, src2: s2 })?),
            "SHR" => instructions.push(ctx.emit_int_alu3(instr, |d, s1, s2| Instruction::Shr { dst: d, src1: s1, src2: s2 })?),

            "NEG" => instructions.push(ctx.emit_int_alu2(instr, |d, s| Instruction::Neg { dst: d, src: s })?),
            "NOT" => instructions.push(ctx.emit_int_alu2(instr, |d, s| Instruction::Not { dst: d, src: s })?),

            "FADD" => instructions.push(ctx.emit_float_alu3(instr, |d, s1, s2| Instruction::FAdd { dst: d, src1: s1, src2: s2 })?),
            "FSUB" => instructions.push(ctx.emit_float_alu3(instr, |d, s1, s2| Instruction::FSub { dst: d, src1: s1, src2: s2 })?),
            "FMUL" => instructions.push(ctx.emit_float_alu3(instr, |d, s1, s2| Instruction::FMul { dst: d, src1: s1, src2: s2 })?),
            "FDIV" => instructions.push(ctx.emit_float_alu3(instr, |d, s1, s2| Instruction::FDiv { dst: d, src1: s1, src2: s2 })?),

            "FNEG"  => instructions.push(ctx.emit_float_alu2(instr, |d, s| Instruction::FNeg  { dst: d, src: s })?),
            "FSIN"  => instructions.push(ctx.emit_float_alu2(instr, |d, s| Instruction::FSin  { dst: d, src: s })?),
            "FCOS"  => instructions.push(ctx.emit_float_alu2(instr, |d, s| Instruction::FCos  { dst: d, src: s })?),
            "FSQRT" => instructions.push(ctx.emit_float_alu2(instr, |d, s| Instruction::FSqrt { dst: d, src: s })?),

            "FCEIL" | "FFLOOR" | "FROUND" => {
                ctx.expect_operand_count(instr, 2)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                let src = ctx.resolve_register(&instr.operands[1])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                ctx.expect_bank(dst, RegisterBank::Integer, instr.operands[0].span.clone())?;
                ctx.expect_bank(src, RegisterBank::Float, instr.operands[1].span.clone())?;
                let i = match instr.mnemonic.node.as_str() {
                    "FCEIL" => Instruction::FCeil { dst, src },
                    "FFLOOR" => Instruction::FFloor { dst, src },
                    "FROUND" => Instruction::FRound { dst, src },
                    _ => unreachable!(),
                };
                instructions.push(i);
            }

            "ITOF" => {
                ctx.expect_operand_count(instr, 2)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                let src = ctx.resolve_operand(&instr.operands[1])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                ctx.expect_bank(dst, RegisterBank::Float, instr.operands[0].span.clone())?;
                ctx.expect_int_operand(&instr.operands[1])?;
                instructions.push(Instruction::ItoF { dst, src });
            }
            "FTOI" => {
                ctx.expect_operand_count(instr, 2)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                let src = ctx.resolve_operand(&instr.operands[1])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                ctx.expect_bank(dst, RegisterBank::Integer, instr.operands[0].span.clone())?;
                ctx.expect_float_operand(&instr.operands[1])?;
                instructions.push(Instruction::FtoI { dst, src });
            }

            "MOV" => {
                ctx.expect_operand_count(instr, 2)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                let src = ctx.resolve_operand(&instr.operands[1])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                if let Operand::Register(src_reg) = &src
                    && dst.bank() != src_reg.bank()
                {
                    return Err(ctx.err(
                        instr.mnemonic.span.clone(),
                        format!(
                            "MOV requires same-bank registers, got {dst:?} and {src_reg:?}"
                        ),
                    ));
                }
                instructions.push(Instruction::Mov { dst, src });
            }
            "LD" => {
                ctx.expect_operand_count(instr, 2)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                let addr = ctx.resolve_operand(&instr.operands[1])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                ctx.expect_int_operand(&instr.operands[1])?;
                instructions.push(Instruction::Ld { dst, addr });
            }
            "ST" => {
                ctx.expect_operand_count(instr, 2)?;
                let src = ctx.resolve_operand(&instr.operands[0])?;
                let addr = ctx.resolve_operand(&instr.operands[1])?;
                ctx.expect_int_operand(&instr.operands[1])?;
                instructions.push(Instruction::St { src, addr });
            }
            "LDR" => {
                ctx.expect_operand_count(instr, 3)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                let base = ctx.resolve_register(&instr.operands[1])?;
                let offset = ctx.resolve_operand(&instr.operands[2])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                ctx.expect_bank(base, RegisterBank::Integer, instr.operands[1].span.clone())?;
                ctx.expect_int_operand(&instr.operands[2])?;
                instructions.push(Instruction::Ldr { dst, base, offset });
            }
            "STR" => {
                ctx.expect_operand_count(instr, 3)?;
                let src = ctx.resolve_operand(&instr.operands[0])?;
                let base = ctx.resolve_register(&instr.operands[1])?;
                let offset = ctx.resolve_operand(&instr.operands[2])?;
                ctx.expect_bank(base, RegisterBank::Integer, instr.operands[1].span.clone())?;
                ctx.expect_int_operand(&instr.operands[2])?;
                instructions.push(Instruction::Str { src, base, offset });
            }
            "PUSH" => {
                ctx.expect_operand_count(instr, 1)?;
                let src = ctx.resolve_operand(&instr.operands[0])?;
                instructions.push(Instruction::Push { src });
            }
            "POP" => {
                ctx.expect_operand_count(instr, 1)?;
                let dst = ctx.resolve_register(&instr.operands[0])?;
                ctx.expect_writable(dst, instr.operands[0].span.clone())?;
                instructions.push(Instruction::Pop { dst });
            }

            "CMP" => {
                ctx.expect_operand_count(instr, 2)?;
                let src1 = ctx.resolve_operand(&instr.operands[0])?;
                let src2 = ctx.resolve_operand(&instr.operands[1])?;
                ctx.expect_int_operand(&instr.operands[0])?;
                ctx.expect_int_operand(&instr.operands[1])?;
                instructions.push(Instruction::Cmp { src1, src2 });
            }
            "FCMP" => {
                ctx.expect_operand_count(instr, 2)?;
                let src1 = ctx.resolve_operand(&instr.operands[0])?;
                let src2 = ctx.resolve_operand(&instr.operands[1])?;
                ctx.expect_float_operand(&instr.operands[0])?;
                ctx.expect_float_operand(&instr.operands[1])?;
                instructions.push(Instruction::FCmp { src1, src2 });
            }

            "JMP"  => instructions.push(ctx.emit_jump(instr, |a| Instruction::Jmp  { addr: a })?),
            "JEQ"  => instructions.push(ctx.emit_jump(instr, |a| Instruction::Jeq  { addr: a })?),
            "JNE"  => instructions.push(ctx.emit_jump(instr, |a| Instruction::Jne  { addr: a })?),
            "JLT"  => instructions.push(ctx.emit_jump(instr, |a| Instruction::Jlt  { addr: a })?),
            "JGT"  => instructions.push(ctx.emit_jump(instr, |a| Instruction::Jgt  { addr: a })?),
            "JLE"  => instructions.push(ctx.emit_jump(instr, |a| Instruction::Jle  { addr: a })?),
            "JGE"  => instructions.push(ctx.emit_jump(instr, |a| Instruction::Jge  { addr: a })?),
            "CALL" => instructions.push(ctx.emit_jump(instr, |a| Instruction::Call { addr: a })?),

            "RET" => {
                ctx.expect_operand_count(instr, 0)?;
                instructions.push(Instruction::Ret);
            }
            "SCAN" => {
                ctx.expect_operand_count(instr, 0)?;
                instructions.push(Instruction::Scan);
            }
            "FIRE" => {
                ctx.expect_operand_count(instr, 0)?;
                instructions.push(Instruction::Fire);
            }
            "NOP" => {
                ctx.expect_operand_count(instr, 0)?;
                instructions.push(Instruction::Nop);
            }
            "YIELD" => {
                ctx.expect_operand_count(instr, 0)?;
                instructions.push(Instruction::Yield);
            }

            other => {
                return Err(ctx.err(
                    instr.mnemonic.span.clone(),
                    format!("unknown opcode '{other}'"),
                ));
            }
        }
    }

    Ok(Program { instructions })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn assemble(source: &str) -> Result<Program> {
    let stripped = strip_comments_preserving_spans(source);
    let ast = program_parser()
        .parse(stripped.clone())
        .map_err(|errs| anyhow!("{}", render_parse_errors(source, errs)))?;

    let non_empty: Vec<_> = ast
        .into_iter()
        .filter(|s| s.label.is_some() || s.instruction.is_some())
        .collect();

    emit(source, &non_empty)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_spinner() {
        let source = r#"
; "Spinner" — rotates and fires whenever it sees something.
start:
    MOV  spd, 0
    MOV  trn, 0
    MOV  trt, 3.0

main_loop:
    SCAN
    CMP  sc_type, 0
    JEQ  no_target

    CMP  sc_type, 3
    JNE  no_target

    FIRE
    JMP  done

no_target:
done:
    YIELD
    JMP  main_loop
"#;
        let program = assemble(source).unwrap();
        assert_eq!(program.instructions.len(), 12);

        assert_eq!(
            program.instructions[0],
            Instruction::Mov {
                dst: RegisterId::Spd,
                src: Operand::Immediate(0)
            }
        );
        assert_eq!(
            program.instructions[2],
            Instruction::Mov {
                dst: RegisterId::Trt,
                src: Operand::ImmediateFloat(3.0)
            }
        );
        assert_eq!(program.instructions[3], Instruction::Scan);
        assert_eq!(program.instructions[5], Instruction::Jeq { addr: 10 });
        assert_eq!(program.instructions[8], Instruction::Fire);
        assert_eq!(program.instructions[9], Instruction::Jmp { addr: 10 });
        assert_eq!(program.instructions[10], Instruction::Yield);
        assert_eq!(program.instructions[11], Instruction::Jmp { addr: 3 });
    }

    #[test]
    fn assemble_all_instruction_types() {
        let source = r#"
    ADD r0, r1, r2
    SUB r0, r1, 5
    MUL r3, r4, r5
    DIV r0, r1, r2
    MOD r0, r1, r2
    NEG r0, r1
    AND r0, r1, r2
    OR  r0, r1, r2
    XOR r0, r1, r2
    NOT r0, r1
    SHL r0, r1, r2
    SHR r0, r1, r2

    FADD f0, f1, f2
    FSUB f0, f1, f2
    FMUL f0, f1, f2
    FDIV f0, f1, f2
    FNEG f0, f1
    FSIN f0, f1
    FCOS f0, f1
    FSQRT f0, f1

    ITOF f0, r1
    FTOI r0, f1

    MOV r0, r1
    MOV r0, 42
    LD  r0, 100
    ST  r0, 100
    LDR r0, r1, 4
    STR r0, r1, 4
    PUSH r0
    POP  r1

    CMP r0, r1
    FCMP f0, f1

    FCEIL r0, f0
    FFLOOR r1, f1
    FROUND r2, f2

loop:
    JMP loop
    JEQ loop
    JNE loop
    JLT loop
    JGT loop
    JLE loop
    JGE loop
    CALL loop
    RET

    SCAN
    FIRE
    NOP
    YIELD
"#;
        let program = assemble(source).unwrap();
        assert_eq!(program.instructions.len(), 48);

        assert_eq!(
            program.instructions[0],
            Instruction::Add {
                dst: RegisterId::R0,
                src1: Operand::Register(RegisterId::R1),
                src2: Operand::Register(RegisterId::R2)
            }
        );
        assert_eq!(
            program.instructions[1],
            Instruction::Sub {
                dst: RegisterId::R0,
                src1: Operand::Register(RegisterId::R1),
                src2: Operand::Immediate(5)
            }
        );
    }

    #[test]
    fn assemble_pseudo_instructions() {
        let source = r#"
    MOVI r0, 42
    INC  r1
    DEC  r2
"#;
        let program = assemble(source).unwrap();
        assert_eq!(program.instructions.len(), 3);

        assert_eq!(
            program.instructions[0],
            Instruction::Mov {
                dst: RegisterId::R0,
                src: Operand::Immediate(42)
            }
        );
        assert_eq!(
            program.instructions[1],
            Instruction::Add {
                dst: RegisterId::R1,
                src1: Operand::Register(RegisterId::R1),
                src2: Operand::Immediate(1)
            }
        );
        assert_eq!(
            program.instructions[2],
            Instruction::Sub {
                dst: RegisterId::R2,
                src1: Operand::Register(RegisterId::R2),
                src2: Operand::Immediate(1)
            }
        );
    }

    #[test]
    fn error_unknown_opcode() {
        let result = assemble("FOOBAR r0, r1");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unknown opcode"));
        assert!(msg.contains("FOOBAR"));
    }

    #[test]
    fn error_undefined_label() {
        let result = assemble("JMP nonexistent");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("undefined label"));
    }

    #[test]
    fn error_wrong_operand_count() {
        let result = assemble("ADD r0, r1");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("expects 3 operand(s), got 2"));
    }

    #[test]
    fn error_duplicate_label() {
        let source = "foo:\nfoo:\n    NOP";
        let result = assemble(source);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("duplicate label"));
    }

    #[test]
    fn label_on_same_line_as_instruction() {
        let source = "loop: NOP\n    JMP loop";
        let program = assemble(source).unwrap();
        assert_eq!(program.instructions.len(), 2);
        assert_eq!(program.instructions[0], Instruction::Nop);
        assert_eq!(program.instructions[1], Instruction::Jmp { addr: 0 });
    }

    #[test]
    fn hex_immediates() {
        let source = "MOV r0, 0xFF";
        let program = assemble(source).unwrap();
        assert_eq!(
            program.instructions[0],
            Instruction::Mov {
                dst: RegisterId::R0,
                src: Operand::Immediate(255)
            }
        );
    }

    #[test]
    fn negative_immediates() {
        let source = "MOV r0, -10";
        let program = assemble(source).unwrap();
        assert_eq!(
            program.instructions[0],
            Instruction::Mov {
                dst: RegisterId::R0,
                src: Operand::Immediate(-10)
            }
        );
    }

    #[test]
    fn case_insensitive_opcodes_and_registers() {
        let source = "mov R0, r1\nadd R2, R3, R4";
        let program = assemble(source).unwrap();
        assert_eq!(program.instructions.len(), 2);
    }

    #[test]
    fn special_registers() {
        let source = "MOV spd, 0.0\nMOV trn, 0.0\nMOV trt, 3.0\nCMP sc_type, 0";
        let program = assemble(source).unwrap();
        assert_eq!(program.instructions.len(), 4);
        assert_eq!(
            program.instructions[3],
            Instruction::Cmp {
                src1: Operand::Register(RegisterId::ScType),
                src2: Operand::Immediate(0)
            }
        );
    }

    #[test]
    fn empty_program() {
        let program = assemble("").unwrap();
        assert!(program.instructions.is_empty());
    }

    #[test]
    fn semicolon_comments_only() {
        let program = assemble("; just a comment\n; another one").unwrap();
        assert!(program.instructions.is_empty());
    }

    #[test]
    fn line_comments() {
        let program = assemble("// line comment\nNOP // trailing").unwrap();
        assert_eq!(program.instructions.len(), 1);
        assert_eq!(program.instructions[0], Instruction::Nop);
    }

    #[test]
    fn block_comments() {
        let source = "/* block comment */\nNOP\n/* multi\nline\ncomment */\nYIELD";
        let program = assemble(source).unwrap();
        assert_eq!(program.instructions.len(), 2);
        assert_eq!(program.instructions[0], Instruction::Nop);
        assert_eq!(program.instructions[1], Instruction::Yield);
    }

    #[test]
    fn inline_block_comment() {
        let source = "MOV r0, /* the value */ 42";
        let program = assemble(source).unwrap();
        assert_eq!(
            program.instructions[0],
            Instruction::Mov {
                dst: RegisterId::R0,
                src: Operand::Immediate(42)
            }
        );
    }

    #[test]
    fn error_wrong_bank_int_alu() {
        let result = assemble("ADD f0, r1, r2");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("integer"));
    }

    #[test]
    fn error_wrong_bank_float_alu() {
        let result = assemble("FADD r0, f1, f2");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("float"));
    }

    #[test]
    fn error_cross_bank_mov() {
        let result = assemble("MOV r0, f0");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("same-bank"));
    }

    #[test]
    fn error_readonly_dst() {
        let result = assemble("MOV hp, 42");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("read-only"));
    }

    #[test]
    fn fceil_ffloor_fround_assembles() {
        let source = "FCEIL r0, f0\nFFLOOR r1, f1\nFROUND r2, f2";
        let program = assemble(source).unwrap();
        assert_eq!(program.instructions.len(), 3);
        assert_eq!(
            program.instructions[0],
            Instruction::FCeil { dst: RegisterId::R0, src: RegisterId::F0 }
        );
        assert_eq!(
            program.instructions[1],
            Instruction::FFloor { dst: RegisterId::R1, src: RegisterId::F1 }
        );
        assert_eq!(
            program.instructions[2],
            Instruction::FRound { dst: RegisterId::R2, src: RegisterId::F2 }
        );
    }

    #[test]
    fn yield_assembles() {
        let program = assemble("YIELD").unwrap();
        assert_eq!(program.instructions[0], Instruction::Yield);
    }

    #[test]
    fn assemble_example_robots() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let examples = ["dumb", "spinner", "patrol", "wanderer"];
        for name in examples {
            let path = workspace_root.join(format!("examples/robots/{}.asm", name));
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("could not read {}", path.display()));
            assemble(&source).unwrap_or_else(|e| panic!("{}.asm should assemble: {}", name, e));
        }
    }
}
