start:
    MOV spd, 0.0
    MOV trn, 0.0
    MOV trt, 3.0

scan_loop:
    SCAN
    CMP sc_type, 3
    JEQ found_target
    YIELD
    JMP scan_loop

found_target:
    MOV trt, 0.0
fire_loop:
    FIRE
    SCAN
    CMP sc_type, 3
    JEQ fire_loop
    MOV trt, 3.0
    YIELD
    JMP scan_loop
