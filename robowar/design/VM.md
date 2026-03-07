# BotASM Specification

## Overview

BotASM is the assembly language for programming robots in RoboWar. Programs are executed by the VM once per simulation tick, up to a configurable cycle budget (default: 100 cycles). When the budget is exhausted or a `YIELD` instruction is reached, execution pauses until the next tick.

## Registers

### General-Purpose Registers

Two separate register banks with no implicit conversion between them:

| Bank    | Registers | Storage | Count |
|---------|-----------|---------|-------|
| Integer | `r0`-`r7` | 32-bit signed/unsigned | 8 |
| Float   | `f0`-`f7` | 32-bit IEEE 754 | 8 |

Integer registers store raw `u32` values, interpreted as `i32` by integer arithmetic. Float registers store `f32` bit patterns. Moving data between banks requires explicit conversion instructions (`ITOF`, `FTOI`, `FCEIL`, `FFLOOR`, `FROUND`).

### Special-Purpose Registers

Special registers are typed. Float-typed specials can only be used in float instructions; int-typed specials can only be used in integer instructions. Using a special register with the wrong instruction type is an assembler error.

#### Float-Typed (RW = read/write, RO = read-only)

| Name      | Access | Description |
|-----------|--------|-------------|
| `spd`     | RW     | Desired speed (units/tick) |
| `trn`     | RW     | Chassis rotation rate (degrees/tick) |
| `trt`     | RW     | Turret rotation rate (degrees/tick) |
| `x`       | RO     | Robot X position |
| `y`       | RO     | Robot Y position |
| `hdg`     | RO     | Chassis heading (degrees) |
| `thr`     | RO     | Turret heading (degrees, absolute) |
| `spd_cur` | RO     | Actual current speed |
| `sc_dist` | RO     | Distance to last scan hit |

#### Integer-Typed (RW = read/write, RO = read-only)

| Name      | Access | Description |
|-----------|--------|-------------|
| `sp`      | RW     | Stack pointer (initialized to 4095, grows downward) |
| `hp`      | RO     | Current hit points |
| `tick`    | RO     | Current simulation tick number |
| `sc_type` | RO     | Type of last scan hit (0=nothing, 1=wall, 2=obstacle, 3=robot) |
| `sc_id`   | RO     | ID of last scan hit (robot ID if sc_type=3, else 0) |
| `rnd`     | RO     | Random value (new value each read) |

Read-only registers are updated by the simulation layer between ticks. Programs cannot write to them; attempting to do so is an assembler error.

## Memory

4096 cells of 32-bit storage, addressed 0-4095. Out-of-bounds access halts the VM for the remainder of the tick.

The stack lives at the top of memory and grows downward from address 4095. The `sp` register tracks the current stack top.

## Syntax

### Instruction Format

```
[label:] MNEMONIC [operand1, operand2, ...]
```

- Labels end with `:` and resolve to instruction indices.
- Mnemonics are case-insensitive.
- Register names are case-insensitive.
- Operands are comma-separated.

### Operand Types

| Type | Syntax | Examples |
|------|--------|---------|
| Integer register | `r0`-`r7` | `r0`, `r3` |
| Float register | `f0`-`f7` | `f0`, `f5` |
| Integer special | `sp`, `hp`, `tick`, `sc_type`, `sc_id`, `rnd` | `hp` |
| Float special | `spd`, `trn`, `trt`, `x`, `y`, `hdg`, `thr`, `spd_cur`, `sc_dist` | `x` |
| Integer immediate | decimal or hex literal | `42`, `-1`, `0xFF` |
| Float immediate | decimal with `.` or scientific notation | `3.14`, `-0.5`, `1.0e3` |
| Label reference | identifier | `loop`, `fire_routine` |

### Comments

Three comment styles are supported:

```asm
; line comment (semicolon)
// line comment (C++ style)
/* block comment
   spanning multiple lines */
```

## Instructions

All instructions cost 1 cycle unless noted otherwise.

### Integer Arithmetic

Operate on integer registers and integer immediates. Use wrapping (modular) arithmetic on overflow.

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `ADD`    | `dst, src1, src2` | `dst = src1 + src2` |
| `SUB`    | `dst, src1, src2` | `dst = src1 - src2` |
| `MUL`    | `dst, src1, src2` | `dst = src1 * src2` |
| `DIV`    | `dst, src1, src2` | `dst = src1 / src2` (division by zero yields 0) |
| `MOD`    | `dst, src1, src2` | `dst = src1 % src2` (mod by zero yields 0) |
| `NEG`    | `dst, src`        | `dst = -src` |

- `dst`: integer register (GP or RW special)
- `src`, `src1`, `src2`: integer register, integer special, or integer immediate

### Float Arithmetic

Operate on float registers and float immediates. Follow IEEE 754 semantics.

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `FADD`   | `dst, src1, src2` | `dst = src1 + src2` |
| `FSUB`   | `dst, src1, src2` | `dst = src1 - src2` |
| `FMUL`   | `dst, src1, src2` | `dst = src1 * src2` |
| `FDIV`   | `dst, src1, src2` | `dst = src1 / src2` |
| `FNEG`   | `dst, src`        | `dst = -src` |
| `FSIN`   | `dst, src`        | `dst = sin(src)` (radians) |
| `FCOS`   | `dst, src`        | `dst = cos(src)` (radians) |
| `FSQRT`  | `dst, src`        | `dst = sqrt(src)` |

- `dst`: float register (GP or RW special)
- `src`, `src1`, `src2`: float register, float special, or float immediate

### Bitwise Operations

Operate on integer registers. Treat values as unsigned 32-bit.

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `AND`    | `dst, src1, src2` | Bitwise AND |
| `OR`     | `dst, src1, src2` | Bitwise OR |
| `XOR`    | `dst, src1, src2` | Bitwise XOR |
| `NOT`    | `dst, src`        | Bitwise NOT |
| `SHL`    | `dst, src1, src2` | Shift left (shift amount masked to 0-31) |
| `SHR`    | `dst, src1, src2` | Arithmetic shift right (shift amount masked to 0-31) |

- `dst`: integer register (GP or RW special)
- `src`, `src1`, `src2`: integer register, integer special, or integer immediate

### Type Conversion

Transfer values between integer and float banks with explicit conversion.

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `ITOF`   | `fdst, isrc` | Integer to float: `fdst = isrc as f32` |
| `FTOI`   | `idst, fsrc` | Float to integer, truncate toward zero |
| `FCEIL`  | `idst, fsrc` | Float to integer, round toward +infinity |
| `FFLOOR` | `idst, fsrc` | Float to integer, round toward -infinity |
| `FROUND` | `idst, fsrc` | Float to integer, round to nearest (ties to even) |

- `fdst`: float register (GP or RW special)
- `isrc`: integer register, integer special, or integer immediate
- `idst`: integer register (GP or RW special)
- `fsrc`: float register, float special, or float immediate

### Data Movement

#### Register Move

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `MOV`    | `dst, src` | Copy raw 32-bit value: `dst = src` |

`MOV` is type-preserving — both operands must be from the same bank (both integer or both float). To move between banks, use conversion instructions.

- `dst`: any writable register
- `src`: register of the same bank, or matching immediate type

#### Memory Access

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `LD`     | `dst, addr`          | Load from memory: `dst = mem[addr]` |
| `ST`     | `src, addr`          | Store to memory: `mem[addr] = src` |
| `LDR`    | `dst, base, offset`  | Load relative: `dst = mem[base + offset]` |
| `STR`    | `src, base, offset`  | Store relative: `mem[base + offset] = src` |

Memory operations transfer raw 32-bit values. `dst`/`src` can be any register type (integer or float). Address operands (`addr`, `base`, `offset`) must be integer-typed.

#### Stack

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `PUSH`   | `src` | Decrement `sp`, store `src` at `mem[sp]` |
| `POP`    | `dst` | Load `mem[sp]` into `dst`, increment `sp` |

`PUSH`/`POP` transfer raw 32-bit values. `src`/`dst` can be any register type.

### Compare and Branch

#### Integer Compare

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `CMP`    | `src1, src2` | Compare integers, set flags (signed comparison) |

Sets internal flags: equal, less, greater.

#### Float Compare

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `FCMP`   | `src1, src2` | Compare floats, set flags |

Sets the same internal flags using float ordering. NaN comparisons set no flags (all false).

#### Branch

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `JMP`    | `label` | Unconditional jump |
| `JEQ`    | `label` | Jump if equal |
| `JNE`    | `label` | Jump if not equal |
| `JLT`    | `label` | Jump if less than |
| `JGT`    | `label` | Jump if greater than |
| `JLE`    | `label` | Jump if less than or equal |
| `JGE`    | `label` | Jump if greater than or equal |

Branch targets are label references resolved to instruction indices by the assembler.

#### Subroutines

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `CALL`   | `label` | Push return address onto stack, jump to label |
| `RET`    | —       | Pop return address from stack, jump to it |

`CALL` decrements `sp`, writes `pc + 1` to `mem[sp]`, then jumps. `RET` reads `mem[sp]`, increments `sp`, then jumps to the read address.

### Actions

These interact with the simulation and cost more cycles.

| Mnemonic | Cost | Description |
|----------|------|-------------|
| `SCAN`   | 5    | Fire a ray along the turret heading. Results written to `sc_dist`, `sc_type`, `sc_id`. |
| `FIRE`   | 3    | Fire a projectile along the turret heading. Subject to cooldown. |

### Control

| Mnemonic | Operands | Description |
|----------|----------|-------------|
| `NOP`    | —        | No operation (costs 1 cycle) |
| `YIELD`  | —        | End execution for this tick. PC advances past `YIELD` for the next tick. |

### Pseudo-Instructions

The assembler expands these into real instructions. They are syntactic sugar.

| Pseudo | Expansion | Description |
|--------|-----------|-------------|
| `INC dst` | `ADD dst, dst, 1` | Increment integer register by 1 |
| `DEC dst` | `SUB dst, dst, 1` | Decrement integer register by 1 |
| `MOVI dst, imm` | `MOV dst, imm` | Alias for MOV with immediate |

## Execution Model

1. Each simulation tick, the VM executes instructions starting from the current PC.
2. Each instruction deducts its cost from the cycle budget.
3. Execution stops when:
   - `YIELD` is reached — PC advances past it, resumes there next tick.
   - The cycle budget is exhausted — PC stays at the next unexecuted instruction.
   - PC passes the end of the program — PC wraps to 0.
4. An empty program is treated as permanently yielded.
5. Integer overflow uses wrapping arithmetic. Division by zero yields 0.
6. Out-of-bounds memory access halts the VM for the remainder of the tick.

## Example: Charger

```asm
; Charger -- drives toward the nearest target and fires at close range.

start:
    MOV  spd, 0.0           ; start stationary
    MOV  trt, 0.0           ; turret faces forward

scan_loop:
    MOV  trt, 5.0           ; sweep turret to find targets
    SCAN
    CMP  sc_type, 3         ; is it a robot?
    JEQ  found_target
    YIELD
    JMP  scan_loop

found_target:
    MOV  trt, 0.0           ; lock turret forward
    MOV  trn, 2.0           ; turn chassis toward target
    MOV  spd, 10.0          ; full speed ahead

charge_loop:
    SCAN
    CMP  sc_type, 0         ; lost target?
    JEQ  lost_target

    FCMP sc_dist, 200.0     ; close enough to fire?
    JGT  keep_charging

    FIRE
    JMP  charge_loop

keep_charging:
    YIELD
    JMP  charge_loop

lost_target:
    MOV  spd, 0.0           ; stop
    MOV  trn, 0.0           ; stop turning
    YIELD
    JMP  scan_loop
```

## Example: Sniper

```asm
; Sniper -- sits still and sweeps turret slowly, fires at long range.

start:
    MOV  spd, 0.0           ; stationary
    MOV  trn, 0.0           ; no chassis rotation
    MOV  trt, 1.0           ; slow turret sweep

scan_loop:
    SCAN
    CMP  sc_type, 3         ; is it a robot?
    JEQ  target_found
    YIELD
    JMP  scan_loop

target_found:
    MOV  trt, 0.0           ; stop turret to aim
    FIRE

    YIELD                   ; brief pause
    MOV  trt, 1.0           ; resume sweep
    JMP  scan_loop
```
