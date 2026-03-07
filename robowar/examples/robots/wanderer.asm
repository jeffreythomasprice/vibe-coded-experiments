; Wanderer -- random movement with scanning turret.
; Wanders forward, periodically picks a new random turn direction.
; Turret spins continuously; fires at robots, avoids close walls.
;
; Register allocation:
;   R0  = turn timer (int, counts down)
;   R1  = scratch (random value)
;   F0  = computed turn rate
;   F2  = wall threshold distance (60.0)

init:
    MOV  spd, 3.0
    MOV  trt, 4.0
    MOV  r0, 0
    MOV  f2, 60.0

main:
    CMP  r0, 0
    JGT  skip_new_turn
    MOV  r1, rnd
    MOD  r1, r1, 5
    SUB  r1, r1, 2
    ITOF f0, r1
    MOV  trn, f0
    MOV  r0, 60

skip_new_turn:
    DEC  r0

    SCAN
    CMP  sc_type, 3
    JNE  check_wall
    FIRE
    JMP  end_tick

check_wall:
    CMP  sc_type, 1
    JNE  end_tick
    FCMP sc_dist, f2
    JGE  end_tick
    MOV  trn, 4.0
    MOV  r0, 30

end_tick:
    YIELD
    JMP  main
