; Patrol -- moves forward, spins turret, fires at robots.
; When turret is roughly forward and a wall is detected nearby, turns left.
;
; Register allocation:
;   R0  = turn timer (int, counts down while turning)
;   F0  = turret bearing threshold (30.0 degrees)
;   F1  = wall detect distance (100.0)

init:
    MOV  r0, 0
    MOV  f0, 30.0
    MOV  f1, 100.0
    MOV  spd, 3.0
    MOV  trt, 5.0
    MOV  trn, 0.0

main:
    ; If currently turning, count down
    CMP  r0, 0
    JEQ  do_scan
    DEC  r0
    CMP  r0, 0
    JGT  end_tick
    ; Turn finished
    MOV  trn, 0.0
    JMP  do_scan

do_scan:
    SCAN

    ; Fire at robots
    CMP  sc_type, 3
    JNE  check_wall
    FIRE
    JMP  end_tick

check_wall:
    CMP  sc_type, 1
    JNE  end_tick

    ; Wall detected -- only avoid if turret is roughly forward-facing.
    ; thr is turret bearing relative to chassis (0 = forward).
    ; Check if thr < 30 or thr > 330 (i.e. within 30 degrees of forward).
    FCMP thr, f0
    JLT  wall_ahead
    FCMP thr, 330.0
    JGT  wall_ahead
    JMP  end_tick

wall_ahead:
    ; Only react if wall is close
    FCMP sc_dist, f1
    JGE  end_tick
    ; Turn left for 30 ticks (~90 degrees at 3 deg/tick)
    MOV  trn, 3.0
    MOV  r0, 30

end_tick:
    YIELD
    JMP  main
