; Orbiter -- scans arena walls to find center, then orbits while firing.
;
; Register allocation:
;   F0  = scratch / subroutine target bearing
;   F1  = scratch
;   F2  = scratch
;   F3  = scratch
;   F4  = hdg_rad (cached during computation)
;   F5  = cos(hdg_rad) (cached)
;   F6  = sin(hdg_rad) (cached)
;   F7  = scratch
;   R0  = scratch / phase flag
;
; Memory layout:
;   [0] = center_x    [1] = center_y
;   [2] = orbit_radius  [3] = orbit_turn_rate
;   [10] = scan_down(+Y)  [11] = scan_right(+X)  [12] = scan_up(-Y)  [13] = scan_left(-X)

; ============================================================
; Phase 1: Init & Wall Scanning
; ============================================================
init:
    MOV  spd, 0.0
    MOV  trn, 0.0
    MOV  trt, 0.0
    YIELD

    ; Scan right (absolute 0°, +X)
    FSUB f0, 0.0, hdg
    CALL align_turret
    SCAN
    ST   sc_dist, 11

    ; Scan down (absolute 90°, +Y)
    FSUB f0, 90.0, hdg
    CALL align_turret
    SCAN
    ST   sc_dist, 10

    ; Scan left (absolute 180°, -X)
    FSUB f0, 180.0, hdg
    CALL align_turret
    SCAN
    ST   sc_dist, 13

    ; Scan up (absolute 270°, -Y)
    FSUB f0, 270.0, hdg
    CALL align_turret
    SCAN
    ST   sc_dist, 12

    JMP  compute_orbit

; ============================================================
; align_turret subroutine
; Aligns turret to bearing in F0. Spins Trt until |delta| < 2 degrees.
; ============================================================
align_turret:
    ; delta = target - thr
    FSUB f1, f0, thr
    ; Normalize to [-180, 180]
    FCMP f1, 180.0
    JLE  at_check_low
    FSUB f1, f1, 360.0
at_check_low:
    FCMP f1, -180.0
    JGE  at_check_done
    FADD f1, f1, 360.0
at_check_done:
    ; If |delta| < 2, we're aligned
    FCMP f1, 2.0
    JGE  at_spin
    FCMP f1, -2.0
    JLE  at_spin
    MOV  trt, 0.0
    RET

at_spin:
    ; Clamp delta to [-30, 30] for proportional steering
    FCMP f1, 30.0
    JLE  at_clamp_low
    MOV  f1, 30.0
at_clamp_low:
    FCMP f1, -30.0
    JGE  at_clamp_done
    MOV  f1, -30.0
at_clamp_done:
    MOV  trt, f1
    YIELD
    JMP  align_turret

; ============================================================
; Phase 2: Compute Orbit Parameters
; ============================================================
compute_orbit:
    LD   f0, 11       ; scan_right (+X)
    LD   f1, 13       ; scan_left (-X)
    LD   f2, 10       ; scan_down (+Y)
    LD   f3, 12       ; scan_up (-Y)

    ; dim_x = right + left, dim_y = down + up
    FADD f7, f0, f1   ; f7 = dim_x
    FADD f4, f2, f3   ; f4 = dim_y

    ; orbit_radius = min(dim_x, dim_y) / 2 - 20
    FCMP f7, f4
    JLE  co_use_along
    MOV  f7, f4       ; f7 = min dimension
co_use_along:
    FDIV f7, f7, 2.0
    FSUB f7, f7, 20.0
    FMUL f7, f7, 0.95
    ; Clamp to minimum 10
    FCMP f7, 10.0
    JGE  co_radius_ok
    MOV  f7, 10.0
co_radius_ok:
    ST   f7, 2        ; store orbit_radius

    ; orbit_turn_rate = 171.887 / orbit_radius
    FDIV f4, 171.887, f7
    ST   f4, 3        ; store orbit_turn_rate

    ; center_x = x + (scan_right - scan_left) / 2
    LD   f0, 11
    LD   f1, 13
    FSUB f0, f0, f1
    FDIV f0, f0, 2.0
    FADD f0, x, f0
    ST   f0, 0        ; store center_x

    ; center_y = y + (scan_down - scan_up) / 2
    LD   f1, 10
    LD   f2, 12
    FSUB f1, f1, f2
    FDIV f1, f1, 2.0
    FADD f1, y, f1
    ST   f1, 1        ; store center_y

    JMP  approach

; ============================================================
; Phase 3: Approach Center
; ============================================================
approach:
    LD   f0, 0        ; center_x
    LD   f1, 1        ; center_y
    LD   f7, 2        ; orbit_radius

    ; dx = center_x - X, dy = center_y - Y
    FSUB f2, f0, x    ; f2 = dx
    FSUB f3, f1, y    ; f3 = dy

    ; distance = sqrt(dx*dx + dy*dy)
    FMUL f4, f2, f2
    FMUL f5, f3, f3
    FADD f4, f4, f5
    FSQRT f4, f4      ; f4 = distance

    ; Check if close enough: distance <= orbit_radius * 1.2
    FMUL f5, f7, 1.2
    FCMP f4, f5
    JLE  orbit

    ; Cross-product steering toward center
    ; cross = cos(hdg_rad)*dy - sin(hdg_rad)*dx
    FMUL f5, hdg, 0.01745329
    FCOS f6, f5
    FSIN f5, f5
    FMUL f6, f6, f3   ; cos(hdg)*dy
    FMUL f5, f5, f2   ; sin(hdg)*dx
    FSUB f6, f6, f5   ; cross = cos*dy - sin*dx

    ; Bang-bang steering: turn toward center
    FCMP f6, 0.0
    JGE  ap_turn_pos
    MOV  trn, -3.0
    JMP  ap_move
ap_turn_pos:
    MOV  trn, 3.0

ap_move:
    MOV  spd, 3.0

    ; Opportunistic scan/fire during approach
    SCAN
    CMP  sc_type, 3
    JNE  ap_yield
    FIRE

ap_yield:
    YIELD
    JMP  approach

; ============================================================
; Phase 4: Orbit & Fire
; Uses 2D controller: tangent alignment (cross product) + radial correction
; ============================================================
orbit:
    LD   f0, 0        ; center_x
    LD   f1, 1        ; center_y
    LD   f7, 2        ; orbit_radius

    ; dx = center_x - X, dy = center_y - Y
    FSUB f2, f0, x    ; f2 = dx
    FSUB f3, f1, y    ; f3 = dy

    ; distance = sqrt(dx*dx + dy*dy)
    FMUL f4, f2, f2
    FMUL f5, f3, f3
    FADD f4, f4, f5
    FSQRT f4, f4      ; f4 = distance

    ; Clamp distance to minimum 1.0 to avoid division by zero
    FCMP f4, 1.0
    JGE  orb_dist_ok
    MOV  f4, 1.0
orb_dist_ok:

    ; Heading unit vector
    FMUL f5, hdg, 0.01745329  ; f5 = hdg_rad
    FCOS f6, f5       ; f6 = hx
    FSIN f5, f5       ; f5 = hy

    ; CW orbit tangent direction: tx = dy, ty = -dx
    ; Cross product: hx*ty - hy*tx = hx*(-dx) - hy*dy = -(hx*dx + hy*dy)
    FMUL f0, f6, f2   ; hx * dx
    FMUL f1, f5, f3   ; hy * dy
    FADD f0, f0, f1   ; hx*dx + hy*dy
    FSUB f0, 0.0, f0  ; cross = -(hx*dx + hy*dy)

    ; Normalize cross by distance to get ~sin(heading_error)
    FDIV f0, f0, f4   ; tangent_error = cross / dist

    ; Radial error: dist - orbit_radius (positive = too far)
    FSUB f1, f4, f7   ; radial_error
    FMUL f1, f1, 0.1  ; radial_correction

    ; Combined turn: base_turn + tangent_error * 2.0 + radial_correction
    FMUL f0, f0, 2.0
    FADD f0, f0, f1
    ; Add feedforward base turn rate from mem[3]
    LD   f1, 3         ; orbit_turn_rate
    FADD f0, f0, f1

    ; Clamp to [-3, 3]
    FCMP f0, 3.0
    JLE  orb_clamp_low
    MOV  f0, 3.0
orb_clamp_low:
    FCMP f0, -3.0
    JGE  orb_clamp_done
    MOV  f0, -3.0
orb_clamp_done:
    MOV  trn, f0
    MOV  spd, 3.0

    ; Steer turret toward center using cross-product steering
    ; Recompute dx/dy (registers were clobbered)
    LD   f0, 0        ; center_x
    LD   f1, 1        ; center_y
    FSUB f2, f0, x    ; dx
    FSUB f3, f1, y    ; dy
    ; Cross of turret heading vs center vector
    ; turret_abs = hdg + thr
    FADD f0, hdg, thr
    FMUL f0, f0, 0.01745329
    FCOS f4, f0       ; turret hx
    FSIN f5, f0       ; turret hy
    ; cross = turret_hx * dy - turret_hy * dx
    FMUL f4, f4, f3
    FMUL f5, f5, f2
    FSUB f6, f4, f5   ; turret cross
    FMUL f6, f6, 0.5
    ; Clamp turret rate to [-30, 30]
    FCMP f6, 30.0
    JLE  orb_trt_low
    MOV  f6, 30.0
orb_trt_low:
    FCMP f6, -30.0
    JGE  orb_trt_done
    MOV  f6, -30.0
orb_trt_done:
    MOV  trt, f6

    FIRE
    YIELD
    JMP  orbit
