# Bool/int/float ordering cross-type works because bool is an int subclass
# and int↔float go through lossy compare. Pins each cross-type lt path
# through types::dispatch_lt + per-builtin lt slots.
print(True < 2)
print(False < 1)
print(1 < 1.5)
print(1.5 > 1)
print(True == 1)
print(False == 0)
print(0 < True)
print(True <= 1)
